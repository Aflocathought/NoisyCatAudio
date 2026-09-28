//! Envelope checks use both rendered audio and internal state. Sub-hop decay
//! is meaningful for feedback but cannot undo the FFT synthesis window.
use super::*;

fn rms(audio: &[f32]) -> f32 {
    (audio.iter().map(|v| v * v).sum::<f32>() / audio.len() as f32).sqrt()
}

fn render(rate: usize, attack_ms: Option<f32>, midi: bool) -> Vec<f32> {
    let mut engine = SpectralResonator::new(1, 4096, 512, rate as f32).unwrap();
    let controls = ResonatorControls {
        note: Some(69),
        harmonics: 1,
        attack_ms,
        t60: 2.0,
        hf_damp: 0.0,
        wet_level: 1.0,
        low_hz: 1.0,
        high_hz: 12000.0,
        ..Default::default()
    };
    if midi {
        engine.note_on(69, 1, 1.0);
    }
    let mut audio = vec![0.0; rate * 4];
    for (i, sample) in audio.iter_mut().enumerate() {
        // Two onsets within one held note verify this is audio-responsive,
        // rather than a note-on-only envelope. The second transient is louder.
        let amplitude = if (rate / 5..rate).contains(&i) {
            0.1
        } else if (rate * 3 / 2..rate * 5 / 2).contains(&i) {
            0.25
        } else {
            0.0
        };
        *sample = amplitude * (std::f64::consts::TAU * 440.0 * i as f64 / rate as f64).sin() as f32;
        if midi {
            engine.process_poly_frame([&mut *sample], controls);
        } else {
            engine.process_frame([&mut *sample], controls);
        }
        *sample = engine.output_parts().1[0];
    }
    audio
}

#[test]
fn independent_attack_speeds_repeated_onsets_without_shortening_the_rendered_tail() {
    for rate in [44100, 48000, 96000] {
        let natural = render(rate, None, false);
        let fast = render(rate, Some(10.0), false);
        let slow = render(rate, Some(400.0), false);
        assert_eq!(
            fast,
            render(rate, Some(10.0), true),
            "Internal/MIDI envelope mismatch"
        );
        for (onset, amplitude) in [(rate / 5, 0.1), (rate * 3 / 2, 0.25)] {
            let threshold = amplitude * std::f32::consts::FRAC_1_SQRT_2 * 0.9;
            let crossing = |audio: &[f32]| {
                (onset..onset + rate)
                    .step_by(rate / 1000)
                    .find(|&i| rms(&audio[i..i + rate / 100]) >= threshold)
                    .map(|i| (i - onset) as f32 / rate as f32)
                    .expect("onset must reach 90%")
            };
            let (n, f, s) = (crossing(&natural), crossing(&fast), crossing(&slow));
            eprintln!(
                "rate={rate} onset={onset} time to 90%: natural={n:.4}s fast={f:.4}s slow={s:.4}s"
            );
            assert!(
                f < 0.16 && n > f + 0.3 && s > f + 0.15,
                "attack did not control onset"
            );
        }
        for audio in [&natural, &fast, &slow] {
            let start = rate * 5 / 2 + 8192;
            let ratio = rms(&audio[start + rate..start + rate + rate / 10])
                / rms(&audio[start..start + rate / 10]);
            // T60=2 s means -30 dB after one second, regardless of Attack.
            assert!(
                (ratio / 10.0_f32.powf(-1.5) - 1.0).abs() < 0.02,
                "tail ratio {ratio}"
            );
            assert!(audio.iter().all(|x| x.is_finite() && x.abs() < 0.251));
        }
    }
}

#[test]
fn five_millisecond_decay_reaches_feedback_in_both_models_and_note_off_keeps_t60() {
    for rate in [44100.0, 48000.0, 96000.0] {
        for curve in [false, true] {
            let mut engine = SpectralResonator::new(1, 4096, 512, rate).unwrap();
            let mut decay_curve = DecayCurve::default();
            for point in &mut decay_curve.points {
                point.seconds = crate::MIN_DECAY_SECONDS;
            }
            let controls = ResonatorControls {
                note: None,
                harmonics: 1,
                attack_ms: Some(0.0),
                t60: if curve {
                    12.0
                } else {
                    crate::MIN_DECAY_SECONDS
                },
                hf_damp: 0.0,
                decay_curve: curve.then_some(decay_curve),
                ..Default::default()
            };
            engine.note_on(69, 1, 1.0);
            for _ in 0..2048 {
                engine.process_poly_frame([&mut 0.0], controls);
            }
            engine.voices[0].state[0][0] = Complex32::new(1.0, 0.0);
            engine.note_off(1);
            for _ in 0..512 {
                engine.process_poly_frame([&mut 0.0], controls);
            }
            let expected = 10.0_f32.powf(-3.0 * 512.0 / (rate * 0.005));
            let actual = engine.voices[0].state[0][0].norm();
            assert!(
                (actual / expected - 1.0).abs() < 0.005,
                "rate={rate} curve={curve} ratio={actual}/{expected}"
            );
        }
    }
}

#[test]
fn independent_attack_preserves_stereo_dry_and_survives_live_switches() {
    let mut stereo = SpectralResonator::new(2, 4096, 512, 48000.0).unwrap();
    let mut mono = SpectralResonator::new(1, 4096, 512, 48000.0).unwrap();
    for i in 0..24000 {
        let input = (i as f32 * 0.0576).sin() * 0.2;
        let mut frame = [input, if i >= 12000 { input * 0.5 } else { 0.0 }];
        let mut single = input;
        let controls = ResonatorControls {
            attack_ms: [Some(0.0), None, Some(2000.0), Some(f32::NAN)][i / 6000],
            t60: [0.005, 12.0, 2.0, 0.005][i / 6000],
            ..Default::default()
        };
        stereo.process_frame(frame.iter_mut(), controls);
        mono.process_frame([&mut single], controls);
        assert_eq!(frame[0], single);
        if i < 12000 {
            assert_eq!(frame[1], 0.0);
        }
        assert!(frame.iter().all(|v| v.is_finite() && v.abs() < 8.0));
        assert_eq!(stereo.output_parts().0[0], mono.output_parts().0[0]);
    }
    stereo.reset();
    for _ in 0..8192 {
        let mut frame = [0.0; 2];
        stereo.process_frame(frame.iter_mut(), ResonatorControls::default());
        assert_eq!(frame, [0.0; 2]);
    }
}

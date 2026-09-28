use super::*;

fn tone(hz: f32, sample: usize, rate: f32) -> f32 {
    (std::f32::consts::TAU * hz * sample as f32 / rate).sin()
}

#[test]
fn outer_band_mutes_keep_middle_and_independent_stereo_at_every_fft_size() {
    // Both channels contain independent low/mid/high tones. Measure the
    // rendered signals rather than merely checking the crossover formula.
    let frequencies = [[50.0, 1000.0, 14000.0], [85.0, 1600.0, 12000.0]];
    for (size, hop, rate) in [
        (1024, 256, 96000.0),
        (2048, 512, 48000.0),
        (3072, 512, 48000.0),
        (4096, 512, 44100.0),
    ] {
        let start = size * 3;
        let end = start + rate as usize / 4;
        let mut renders = Vec::new();
        for (mute_low, mute_high, mid_mix) in [
            (false, false, 0.0),
            (true, false, 0.0),
            (false, true, 0.0),
            (true, true, 0.0),
            (true, true, 1.0),
        ] {
            let mut engine = SpectralResonator::new(2, size, hop, rate).unwrap();
            let controls = ResonatorControls {
                note: None,
                mid_mix,
                mute_low,
                mute_high,
                ..Default::default()
            };
            let mut output = Vec::with_capacity(end);
            for i in 0..end {
                let mut frame =
                    frequencies.map(|tones| tones.iter().map(|&hz| 0.05 * tone(hz, i, rate)).sum());
                engine.process_frame(&mut frame, controls);
                output.push(frame);
            }
            renders.push(output);
        }
        assert!(
            renders[4].iter().all(|frame| *frame == [0.0; 2]),
            "All dry contributions muted from startup"
        );
        let level = |samples: &[[f32; 2]], channel, hz| {
            let mut real = 0.0_f64;
            let mut imag = 0.0_f64;
            for (i, sample) in samples.iter().enumerate().take(end).skip(start) {
                let angle = std::f64::consts::TAU * f64::from(hz) * i as f64 / f64::from(rate);
                real += f64::from(sample[channel]) * angle.cos();
                imag += f64::from(sample[channel]) * angle.sin();
            }
            real.hypot(imag)
        };
        for (channel, tones) in frequencies.iter().enumerate() {
            for (band, &hz) in tones.iter().enumerate() {
                let baseline = level(&renders[0], channel, hz);
                assert!(baseline > 1.0);
                for (case, muted) in [(1, band == 0), (2, band == 2), (3, band != 1)] {
                    let ratio = level(&renders[case], channel, hz) / baseline;
                    if muted {
                        // The 1024/96k window has deliberately broad bass
                        // selectivity; do not expect a brick-wall crossover.
                        assert!(
                            ratio < 0.1,
                            "N={size} channel={channel} band={band} ratio={ratio}"
                        );
                    } else {
                        assert!(
                            (ratio - 1.0).abs() < 0.025,
                            "N={size} channel={channel} band={band} ratio={ratio}"
                        );
                    }
                }
            }
            for ((all, low_off), (high_off, both_off)) in renders[0][start..end]
                .iter()
                .zip(&renders[1][start..end])
                .zip(renders[2][start..end].iter().zip(&renders[3][start..end]))
            {
                assert!(
                    (low_off[channel] + high_off[channel] - all[channel] - both_off[channel]).abs()
                        < 2e-6
                );
            }
        }
    }
}

#[test]
fn outer_mute_automation_preserves_resonant_states_and_released_wet_tail() {
    for mode in [UnisonMode::Spectral, UnisonMode::Post] {
        let mut reference = SpectralResonator::new(2, 3072, 512, 48000.0).unwrap();
        let mut muted = SpectralResonator::new(2, 3072, 512, 48000.0).unwrap();
        let mut tail_energy = 0.0;
        for i in 0..32000 {
            let controls = ResonatorControls {
                note: (i < 8000).then_some(69),
                harmonics: 16,
                t60: 0.3,
                align_wet: 0.5,
                unison_mode: mode,
                modulation: ModulationControls {
                    unison_voices: 3,
                    unison_detune_cents: 12.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let input = if i < 8000 {
                0.1 * (tone(440.0, i, 48000.0) + tone(60.0, i, 48000.0))
            } else {
                0.0
            };
            let mut a = [input, 0.0];
            let mut b = a;
            reference.process_frame(&mut a, controls);
            muted.process_frame(
                &mut b,
                ResonatorControls {
                    mute_low: (3501..17000).contains(&i),
                    mute_high: (6103..22000).contains(&i),
                    ..controls
                },
            );
            let wet_a = reference.output_parts().1;
            let wet_b = muted.output_parts().1;
            assert!((wet_a[0] - wet_b[0]).abs() < 2e-6);
            assert_eq!(b[1], 0.0);
            if i > 15000 {
                tail_energy += wet_a[0] * wet_a[0];
            }
        }
        assert!(tail_energy > 1e-5);
        for (a, b) in reference.voices.iter().zip(muted.voices.iter()) {
            assert_eq!(
                a.state, b.state,
                "Band mutes changed resonance excitation/decay"
            );
        }
    }
}

#[test]
fn outer_mute_switches_fade_and_reset_applies_saved_mute_immediately() {
    let mut engine = SpectralResonator::new(1, 1024, 256, 48000.0).unwrap();
    let mut previous = 1.0_f32;
    let mut largest_step = 0.0_f32;
    for i in 0..16000 {
        let mut sample = 1.0;
        engine.process_frame(
            [&mut sample],
            ResonatorControls {
                note: None,
                mute_low: (5003..10007).contains(&i),
                ..Default::default()
            },
        );
        if i > 4096 {
            largest_step = largest_step.max((sample - previous).abs());
        }
        if (8000..10000).contains(&i) {
            assert!(sample.abs() < 1e-6);
        }
        if i > 14000 {
            assert!((sample - 1.0).abs() < 1e-6);
        }
        previous = sample;
    }
    assert!(largest_step < 0.005, "Hard mute edge: {largest_step}");
    engine.reset();
    // Reset must forget an old fade position and apply the saved target on
    // the first synthesized frame, without an unmuted startup frame.
    engine.render_hop(ResonatorControls {
        mute_low: true,
        mute_high: true,
        ..Default::default()
    });
    assert_eq!(engine.outer_band_gains, Some([0.0, 0.0]));
}

//! Polyphony regressions inspect independent state and rendered audio rather
//! than only checking that note bookkeeping contains multiple entries.
use super::*;
use crate::{DecayPoint, ModulationMode};

fn engine() -> SpectralResonator {
    SpectralResonator::new(2, 4096, 512, 48_000.0).unwrap()
}

fn controls() -> ResonatorControls {
    ResonatorControls {
        note: None,
        harmonics: 8,
        hf_damp: 0.0,
        ..Default::default()
    }
}

fn hop(engine: &mut SpectralResonator) {
    for i in 0..512 {
        let mut frame = [(i as f32 * 0.037).sin() * 0.3, 0.0];
        engine.process_poly_frame(frame.iter_mut(), controls());
        assert!(frame.into_iter().all(f32::is_finite));
        assert_eq!(frame[1], 0.0);
    }
}

fn filled() -> SpectralResonator {
    let mut engine = engine();
    for i in 0..MAX_VOICES {
        engine.note_on(57 + i as i32, i as u64 + 1, 1.0);
    }
    for _ in 0..12 {
        hop(&mut engine);
    }
    assert_eq!(engine.active_voice_count(), MAX_VOICES);
    engine
}

#[test]
fn variable_polyphony_retires_excess_voices_and_preserves_retained_state() {
    let mut engine = filled();
    let retained = engine.voices[0].state;
    let retiring = engine.voices[15].state;
    engine.set_voice_limit(2);
    assert_eq!(engine.voices[0].state, retained);
    assert_eq!(engine.voices[15].state, retiring);
    assert!(engine.voices[15].fading);
    hop(&mut engine);
    assert!(engine.voices[15].gain > 0.0);
    hop(&mut engine);
    assert_eq!(engine.active_voice_count(), 2);
    for token in 100..120 {
        engine.note_on(60, token, 1.0);
        hop(&mut engine);
    }
    for _ in 0..3 {
        hop(&mut engine);
    }
    assert_eq!(engine.active_voice_count(), 2);
    engine.set_voice_limit(16);
    engine.note_on(64, 200, 1.0);
    assert_eq!(engine.active_voice_count(), 3);
}

#[test]
fn sixteen_voices_with_eight_unison_sum_independently_and_keep_release_tails() {
    let mut engines = [engine(), engine(), engine(), engine()];
    let controls = ResonatorControls {
        harmonics: 1,
        modulation: ModulationControls {
            unison_voices: 8,
            ..Default::default()
        },
        ..controls()
    };
    let frequencies: Vec<f32> = (48..64)
        .map(|note| 440.0 * 2.0_f32.powf((note - 69) as f32 / 12.0))
        .collect();
    for (index, engine) in engines.iter_mut().enumerate().take(3) {
        for token in 1..=16 {
            if index == 0 || (index == 1 && token <= 8) || (index == 2 && token > 8) {
                engine.note_on(47 + token as i32, token, 1.0);
            }
        }
    }
    assert_eq!(engines[0].active_voice_count(), 16);
    let mut tail_energy = 0.0;
    for i in 0..16_384 {
        if i == 8192 {
            for engine in &mut engines[..3] {
                for token in 1..=16 {
                    engine.note_off(token);
                }
            }
            assert!(
                engines[0]
                    .voices
                    .iter()
                    .all(|voice| voice.state[0][0].norm() > 1e-4
                        && !voice.fading
                        && voice.pending.is_none())
            );
        }
        let input = if i < 8192 {
            frequencies
                .iter()
                .map(|hz| (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin())
                .sum::<f32>()
                * 0.02
        } else {
            0.0
        };
        let mut outputs = [[input, 0.0]; 4];
        for (engine, frame) in engines.iter_mut().zip(&mut outputs) {
            engine.process_poly_frame(frame.iter_mut(), controls);
            assert!(frame[0].is_finite());
            assert_eq!(frame[1], 0.0);
        }
        assert!((outputs[0][0] - (outputs[1][0] + outputs[2][0] - outputs[3][0])).abs() < 5e-6);
        if i > 12_288 {
            tail_energy += outputs[0][0].powi(2);
        }
    }
    assert!(tail_energy > 0.01);
    assert_eq!(engines[0].active_voice_count(), 16);
    eprintln!(
        "voice_bytes={}, voice_pool_bytes={}",
        std::mem::size_of::<Voice>(),
        MAX_VOICES * std::mem::size_of::<Voice>()
    );
}

#[test]
fn post_unison_changes_wet_audio_without_changing_resonance_or_dry() {
    let mut engines = [engine(), engine(), engine()];
    for engine in &mut engines {
        engine.note_on(69, 1, 1.0);
    }
    let mut difference = 0.0_f64;
    let mut tail = 0.0_f64;
    for i in 0..24_000 {
        if i == 12_000 {
            for engine in &mut engines {
                engine.note_off(1);
            }
        }
        let input = if i < 12_000 {
            (i as f32 * 0.0576).sin() * 0.2
        } else {
            0.0
        };
        let mut output = [[input, 0.0]; 3];
        for (index, (engine, frame)) in engines.iter_mut().zip(&mut output).enumerate() {
            engine.process_poly_frame(
                frame.iter_mut(),
                ResonatorControls {
                    unison_mode: UnisonMode::Post,
                    mid_mix: if index == 2 { 0.0 } else { 1.0 },
                    modulation: ModulationControls {
                        unison_voices: if index == 0 { 1 } else { 8 },
                        unison_detune_cents: 50.0,
                        ..Default::default()
                    },
                    ..controls()
                },
            );
            assert_eq!(frame[1], 0.0);
            assert!(frame[0].is_finite());
        }
        assert_eq!(engines[0].voices[0].state, engines[1].voices[0].state);
        let dry = if (4096..16_096).contains(&i) {
            ((i - 4096) as f32 * 0.0576).sin() * 0.2
        } else {
            0.0
        };
        assert!((output[2][0] - dry).abs() < 3e-6);
        if i > 8192 {
            difference += f64::from(output[1][0] - output[0][0]).powi(2);
        }
        if i > 20_000 {
            tail += f64::from(output[1][0]).powi(2);
        }
    }
    assert!(difference > 0.01 && tail > 0.001);
}

#[test]
fn post_nyquist_guard_suppresses_unshiftable_top_partials() {
    let mut reference = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
    let mut post = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
    for engine in [&mut reference, &mut post] {
        engine.voices[0].note = Some(69);
        engine.voices[0].gain = 1.0;
        engine.voices[0].state[0][51] = Complex32::new(1.0, 0.0); // 22,880 Hz
    }
    let controls = ResonatorControls {
        harmonics: 52,
        hf_damp: 0.0,
        lf_damp: 0.0,
        high_hz: 12_000.0,
        t60: 12.0,
        ..controls()
    };
    let mut energy = [0.0_f64; 2];
    for i in 0..24_000 {
        let mut a = 0.0;
        let mut b = 0.0;
        reference.process_poly_frame([&mut a], controls);
        post.process_poly_frame(
            [&mut b],
            ResonatorControls {
                unison_mode: UnisonMode::Post,
                modulation: ModulationControls {
                    unison_voices: 8,
                    unison_detune_cents: 50.0,
                    ..Default::default()
                },
                ..controls
            },
        );
        if i > 8192 {
            energy[0] += f64::from(a).powi(2);
            energy[1] += f64::from(b).powi(2);
        }
    }
    assert!(energy[0] > 1e-6);
    eprintln!(
        "post top-partial relative RMS={}",
        (energy[1] / energy[0]).sqrt()
    );
    assert!(energy[1] < energy[0] * 1e-5);
}

#[test]
fn chord_output_is_sum_of_independent_notes_with_one_dry_path() {
    // Superposition detects hidden mono priority, altered old-voice gains and
    // accidental summation of the dry path once for every note.
    let mut engines = [engine(), engine(), engine(), engine()];
    let mut difference = [0.0_f64; 2];
    for i in 0..24_000 {
        for (index, engine) in engines.iter_mut().enumerate() {
            if i == 1000 && (index == 0 || index == 1) {
                engine.note_on(69, 1, 0.8);
            }
            if i == 5000 && (index == 0 || index == 2) {
                engine.note_on(72, 2, 0.6);
            }
            if i == 6500 {
                engine.note_off(1);
            }
            if i == 12_000 {
                engine.note_off(2);
            }
        }
        let input = [
            ((i as f32 * 0.0576).sin() + (i as f32 * 0.0685).sin()) * 0.2,
            0.0,
        ];
        let mut outputs = [input; 4];
        for (engine, output) in engines.iter_mut().zip(&mut outputs) {
            engine.process_poly_frame(output.iter_mut(), controls());
            assert_eq!(output[1], 0.0);
        }
        let expected = outputs[1][0] + outputs[2][0] - outputs[3][0];
        assert!((outputs[0][0] - expected).abs() < 3e-6);
        if i > 15_000 {
            for (index, energy) in difference.iter_mut().enumerate() {
                *energy += f64::from(outputs[index + 1][0] - outputs[3][0]).powi(2);
            }
        }
    }
    assert!(
        difference.into_iter().all(|energy| energy > 0.01),
        "both released notes must still ring"
    );
}

#[test]
fn new_notes_and_release_preserve_previous_phase_and_natural_decay() {
    let mut engine = engine();
    engine.note_on(69, 1, 1.0);
    for _ in 0..12 {
        hop(&mut engine);
    }
    let before = engine.voices[0].state;
    engine.note_off(1);
    engine.note_on(72, 2, 1.0);
    assert_eq!(engine.voices[0].state, before);
    assert!(!engine.voices[0].fading);
    hop(&mut engine);
    let template = &engine.templates[(69 - FIRST_NOTE) as usize];
    for (h, partial) in template.partials.iter().take(8).enumerate() {
        assert_eq!(
            engine.voices[0].state[0][h],
            before[0][h] * partial.rotation * engine.radii[h]
        );
    }
    assert_eq!(engine.voices[0].gain, 1.0);
    assert_eq!(engine.active_voice_count(), 2);
}

#[test]
fn short_notes_in_one_hop_and_same_pitch_retriggers_are_independent() {
    let mut engine = engine();
    for i in 0..8192 {
        if i == 8000 {
            engine.note_on(69, 1, 1.0);
        }
        if i == 8032 {
            engine.note_off(1);
            engine.note_on(69, 2, 0.5);
        }
        if i == 8064 {
            engine.note_off(2);
        }
        let mut frame = [(i as f32 * 0.0576).sin(), 0.0];
        engine.process_poly_frame(frame.iter_mut(), controls());
    }
    assert_eq!(engine.active_voice_count(), 2);
    assert!(engine.voices[0].state[0][0].norm() > 1e-4);
    assert_eq!(
        engine.voices[1].state[0][0],
        engine.voices[0].state[0][0] * 0.5
    );
    let untouched = engine.voices[1].state;
    engine.choke(1);
    assert!(engine.voices[0].fading);
    assert!(!engine.voices[1].fading);
    assert_eq!(engine.voices[1].state, untouched);
    for _ in 0..3 {
        hop(&mut engine);
    }
    assert!(engine.voices[0].note.is_none());
    assert_eq!(engine.voices[1].token, 2);
    assert!(engine.voices[1].note.is_some());
}

#[test]
fn voice_stealing_fades_quietest_released_tail_then_oldest_held_note() {
    let next = MAX_VOICES as u64 + 1;
    let mut engine = filled();
    engine.note_off(3);
    engine.note_off(5);
    // Seed a known energy ordering without relying on the fixture's spectrum.
    engine.voices[2].energy = 1.0;
    engine.voices[4].energy = 0.01;
    let before = engine.voices[4].state;
    engine.note_on(72, next, 1.0);
    assert_eq!(engine.voices[4].state, before);
    assert_eq!(engine.voices[4].token, 5);
    assert_eq!(engine.voices[4].pending.unwrap().token, next);
    hop(&mut engine);
    assert_eq!(engine.voices[4].token, 5);
    assert!(engine.voices[4].gain > 0.0 && engine.voices[4].gain < 1.0);
    hop(&mut engine);
    assert_eq!(engine.voices[4].token, next);
    assert!(engine.voices[4].gate);
    assert!(engine.voices[4].state[0][0].norm() > 0.0);

    let mut engine = filled();
    engine.note_on(72, next, 1.0);
    assert_eq!(engine.voices[0].token, 1);
    assert_eq!(engine.voices[0].pending.unwrap().token, next);
    assert!(engine.voices.iter().skip(1).all(|v| v.pending.is_none()));
}

#[test]
fn pending_notes_honor_release_choke_and_dense_overload() {
    let next = MAX_VOICES as u64 + 1;
    let mut engine = filled();
    engine.note_on(72, next, 1.0);
    hop(&mut engine);
    engine.note_off(next);
    assert!(!engine.voices[0].pending.unwrap().gate);
    hop(&mut engine);
    assert_eq!(engine.voices[0].token, next);
    assert!(!engine.voices[0].gate);
    assert!(
        engine.voices[0].state[0][0].norm() > 0.0,
        "short waiting note lost its excitation"
    );

    let mut engine = filled();
    for token in next..next + MAX_VOICES as u64 {
        engine.note_on(69, token, 1.0);
    }
    hop(&mut engine);
    let gain = engine.voices[0].gain;
    let overflow = next + MAX_VOICES as u64;
    engine.note_on(72, overflow, 1.0);
    assert_eq!(engine.voices[0].gain, gain);
    assert_eq!(engine.voices[0].pending.unwrap().token, overflow);
    engine.choke(next + 1);
    assert!(engine.voices[1].pending.is_none());
    hop(&mut engine);
    assert_eq!(engine.voices[0].token, overflow);
    assert!(engine.voices[1].note.is_none());
    assert!(engine.voices.iter().all(|voice| voice.pending.is_none()));
    for token in overflow + 1..400 {
        engine.note_on(33 + (token % 61) as i32, token, 1.0);
        hop(&mut engine);
    }
    assert!(engine.active_voice_count() <= MAX_VOICES);
    engine.panic();
    for _ in 0..20 {
        hop(&mut engine);
    }
    assert_eq!(engine.active_voice_count(), 0);
    assert!(engine.voices.iter().all(|voice| voice.pending.is_none()));
}

#[test]
fn released_silence_frees_slots_but_held_silence_does_not() {
    let mut engine = engine();
    engine.note_on(69, 1, 1.0);
    engine.note_on(72, 2, 1.0);
    engine.note_off(1);
    for _ in 0..1024 {
        engine.process_poly_frame([&mut 0.0, &mut 0.0], controls());
    }
    assert_eq!(engine.active_voice_count(), 1);
    assert_eq!(engine.voices[1].token, 2);
}

#[test]
fn higher_partials_are_excited_and_retire_when_the_count_is_lowered() {
    // Test the actual state at the requested harmonic, including the 512th
    // at a rate where it lies below Nyquist. Merely checking UI bounds would
    // miss an old DSP clamp or a template list still limited to 64 entries.
    for (rate, harmonic) in [(48_000.0, 128), (96_000.0, 256), (192_000.0, 512)] {
        let mut engine = SpectralResonator::new(2, 4096, 512, rate).unwrap();
        let mut controls = ResonatorControls {
            harmonics: harmonic,
            hf_damp: 0.0,
            high_hz: 12_000.0,
            ..controls()
        };
        engine.note_on(33, 1, 1.0);
        for i in 0..8192 {
            let mut frame = [
                (std::f64::consts::TAU * 55.0 * harmonic as f64 * i as f64 / rate as f64).cos()
                    as f32
                    * 0.5,
                0.0,
            ];
            engine.process_poly_frame(frame.iter_mut(), controls);
            assert!(frame.into_iter().all(f32::is_finite));
            assert_eq!(frame[1], 0.0);
        }
        let before = engine.voices[0].state[0][harmonic - 1];
        assert!(
            before.norm() > 1e-4,
            "rate={rate} harmonic={harmonic} state={before}"
        );
        controls.harmonics = 64;
        for _ in 0..512 {
            engine.process_poly_frame([&mut 0.0, &mut 0.0], controls);
        }
        let after = engine.voices[0].state[0][harmonic - 1];
        assert!(
            after.norm() > 0.0 && after.norm() < before.norm(),
            "removed partial must fade, not reset"
        );
        let rotation = engine.templates[0].partials[harmonic - 1].rotation;
        let expected = before * rotation * 10.0_f32.powf(-3.0 * 512.0 / (rate * 0.02));
        assert_eq!(after, expected);
        engine.reset();
        assert_eq!(engine.active_voice_count(), 0);
        assert!(
            engine.voices.iter().all(|voice| voice
                .state
                .iter()
                .flatten()
                .all(|&state| state == ZERO))
        );
    }
}

#[test]
fn curve_decay_uses_absolute_frequency_across_notes_and_preserves_existing_tails() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        let mut engine = SpectralResonator::new(1, 4096, 512, rate).unwrap();
        let mut curve = DecayCurve::default();
        curve.points[1] = DecayPoint {
            hz: 220.0,
            seconds: 0.4,
        };
        curve.points[2] = DecayPoint {
            hz: 880.0,
            seconds: 1.6,
        };
        let controls = ResonatorControls {
            decay_curve: Some(curve),
            harmonics: 4,
            hf_damp: 1.0,
            lf_damp: 1.0,
            t60: 12.0,
            ..controls()
        };
        // Both states resonate at 440 Hz, but one is harmonic 2 of A3 and the
        // other harmonic 1 of A4. Harmonic-index damping cannot satisfy this.
        for (voice, note, harmonic) in [(0, 57, 1), (1, 69, 0)] {
            engine.voices[voice].note = Some(note);
            engine.voices[voice].gain = 1.0;
            engine.voices[voice].state[0][harmonic] = Complex32::new(1.0, 0.0);
        }
        for _ in 0..2048 {
            engine.process_poly_frame([&mut 0.0], controls);
        }
        let before = [engine.voices[0].state[0][1], engine.voices[1].state[0][0]];
        assert!(before.iter().all(|state| state.norm() > 0.0));
        for _ in 0..512 * 20 {
            engine.process_poly_frame([&mut 0.0], controls);
        }
        for (index, (voice, harmonic)) in [(0, 1), (1, 0)].into_iter().enumerate() {
            let ratio = engine.voices[voice].state[0][harmonic].norm() / before[index].norm();
            let expected = 10.0_f32.powf(-3.0 * 512.0 * 20.0 / (rate * 0.8));
            assert!(
                (ratio / expected - 1.0).abs() < 1e-4,
                "rate={rate} ratio={ratio}"
            );
        }
        assert_eq!(
            engine.voices[0].curve_radii[1],
            engine.voices[1].curve_radii[0]
        );
        let before = engine.voices[0].state[0][1];
        let mut changed = controls;
        changed.decay_curve.as_mut().unwrap().points[2].seconds = 6.0;
        engine.process_poly_frame([&mut 0.0], changed);
        assert_eq!(
            engine.voices[0].state[0][1], before,
            "curve edit reset a live tail"
        );
        for _ in 0..511 {
            engine.process_poly_frame([&mut 0.0], changed);
        }
        assert!(engine.voices[0].state[0][1].norm() > 0.0);
    }
}

#[test]
fn spectral_modes_are_audible_preserve_stereo_and_return_to_unmodulated_state() {
    for mode in [
        ModulationMode::Chorus,
        ModulationMode::Wander,
        ModulationMode::Granular,
    ] {
        let mut stereo = engine();
        let mut mono = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
        let mut dry_mode = engine();
        for engine in [&mut stereo, &mut mono, &mut dry_mode] {
            engine.note_on(69, 1, 1.0);
        }
        let mut difference = 0.0_f64;
        for i in 0..24_000 {
            let left = (i as f32 * 0.0576).sin() * 0.3;
            let mut output = [
                left,
                if i < 12000 {
                    0.0
                } else {
                    (i as f32 * 0.071).sin() * 0.2
                },
            ];
            let mut alone = left;
            let mut reference = [left, 0.0];
            let mut controls = controls();
            if i < 12000 {
                controls.modulation = ModulationControls {
                    mode,
                    amount: 1.0,
                    pitch_semitones: 0.4,
                    rate_hz: 5.0,
                    unison_voices: 3,
                    unison_detune_cents: 12.0,
                    ..Default::default()
                };
            }
            stereo.process_poly_frame(output.iter_mut(), controls);
            mono.process_poly_frame([&mut alone], controls);
            dry_mode.process_poly_frame(
                reference.iter_mut(),
                ResonatorControls {
                    modulation: ModulationControls::default(),
                    ..controls
                },
            );
            assert_eq!(output[0], alone);
            if i < 12000 {
                assert_eq!(output[1], 0.0);
            }
            assert!(output.into_iter().all(f32::is_finite));
            if (8000..12000).contains(&i) {
                difference += f64::from(output[0] - reference[0]).powi(2);
            }
            if i > 22_000 {
                assert_eq!(
                    output[0], reference[0],
                    "Off did not return to original wet path"
                );
            }
        }
        assert!(difference > 0.01, "mode {mode:?} did not alter audio");
    }
}

#[test]
fn unison_creates_two_detuned_tones_without_replacing_the_resonator_tail() {
    let mut engine = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
    engine.voices[0].note = Some(69);
    engine.voices[0].gain = 1.0;
    engine.voices[0].state[0][0] = Complex32::new(1.0, 0.0);
    let controls = ResonatorControls {
        harmonics: 1,
        t60: 12.0,
        hf_damp: 0.0,
        lf_damp: 0.0,
        low_hz: 1.0,
        high_hz: 12_000.0,
        modulation: ModulationControls {
            unison_voices: 2,
            unison_detune_cents: 20.0,
            ..Default::default()
        },
        ..controls()
    };
    let mut samples = vec![0.0; 96_000];
    for sample in &mut samples {
        engine.process_poly_frame([sample], controls);
    }
    let amplitude = |hz: f64| {
        let mut sum = rustfft::num_complex::Complex64::default();
        for (i, &sample) in samples[24_000..72_000].iter().enumerate() {
            sum += rustfft::num_complex::Complex64::from_polar(
                sample as f64,
                -std::f64::consts::TAU * hz * i as f64 / 48_000.0,
            );
        }
        sum.norm() / 48_000.0
    };
    let center = amplitude(440.0);
    let down = amplitude(440.0 * 2.0_f64.powf(-20.0 / 1200.0));
    let up = amplitude(440.0 * 2.0_f64.powf(20.0 / 1200.0));
    eprintln!("unison carrier={center:e}, lower={down:e}, upper={up:e}");
    assert!(down > center * 3.0 && up > center * 3.0 && down > 0.01 && up > 0.01);
    assert!(
        engine.voices[0].state[0][0].norm() > 0.2,
        "unison cleared the base tail"
    );
}

#[test]
fn spread_assigns_eight_positions_without_repositioning_tails_or_reused_slots() {
    let mut engine = engine();
    let controls = ResonatorControls {
        voice_spread: 1.0,
        ..controls()
    };
    for token in 1..=MAX_VOICES as u64 {
        engine.note_on(56 + token as i32, token, 1.0);
    }
    for _ in 0..4 {
        engine.render_hop(controls);
    }
    for (voice, pan) in engine
        .voices
        .iter()
        .zip(VOICE_PAN_POSITIONS.into_iter().cycle())
    {
        assert_eq!(voice.pan, pan);
        assert_eq!(voice.pan_gains[usize::from(pan > 0.0)], 1.0);
    }
    // A released resonance retains its spatial position and complex state.
    engine.voices[1].state[0][0] = Complex32::new(0.8, 0.2);
    engine.note_off(2);
    engine.note_off(3); // This silent voice frees physical slot 2.
    engine.render_hop(controls);
    assert_eq!(engine.voices[1].pan, 1.0);
    assert!(engine.voices[1].state[0][0].norm() > 0.0);
    assert!(engine.voices[2].note.is_none());
    let next = MAX_VOICES as u64 + 1;
    engine.note_on(69, next, 1.0);
    for _ in 0..4 {
        engine.render_hop(controls);
    }
    assert_eq!(
        engine.voices[2].pan, -1.0,
        "the next note restarts the spatial cycle, independent of slot 2"
    );
    assert_eq!(
        engine.voices[1].pan, 1.0,
        "new note moved the old release tail"
    );
    // Stealing fades the released voice at its old position before replacing it.
    engine.note_on(72, next + 1, 1.0);
    engine.render_hop(controls);
    assert_eq!(engine.voices[1].token, 2);
    assert_eq!(engine.voices[1].pan, 1.0);
    for _ in 0..4 {
        engine.render_hop(controls);
    }
    assert_eq!(engine.voices[1].token, next + 1);
    assert_eq!(engine.voices[1].pan, 1.0);
    // Direct DSP automation also slews instead of jumping/resetting states.
    let before = engine.voices[2].pan;
    engine.render_hop(ResonatorControls {
        voice_spread: 0.0,
        ..controls
    });
    assert!(engine.voices[2].pan > before && engine.voices[2].pan < 0.0);
    for _ in 0..4 {
        engine.render_hop(ResonatorControls {
            voice_spread: 0.0,
            ..controls
        });
    }
    assert!(
        engine
            .voices
            .iter()
            .all(|voice| voice.pan_gains == [1.0; 2])
    );
}

#[test]
fn spread_renders_wet_balance_without_crossfeed_and_is_ignored_in_mono() {
    for (order, mode) in [
        (0, ModulationMode::Off),
        (1, ModulationMode::Granular),
        (2, ModulationMode::Off),
        (7, ModulationMode::Wander),
    ] {
        let mut panned = engine();
        let mut centered = engine();
        let base_controls = ResonatorControls {
            harmonics: 1,
            t60: 12.0,
            low_hz: 1.0,
            high_hz: 12_000.0,
            modulation: ModulationControls {
                mode,
                unison_voices: 2,
                ..Default::default()
            },
            ..controls()
        };
        let spread_controls = ResonatorControls {
            voice_spread: 1.0,
            ..base_controls
        };
        for engine in [&mut panned, &mut centered] {
            engine.note_order = order;
            engine.note_on(69, 1, 1.0);
        }
        for _ in 0..4096 {
            panned.process_poly_frame([&mut 0.0, &mut 0.0], spread_controls);
            centered.process_poly_frame([&mut 0.0, &mut 0.0], base_controls);
        }
        for engine in [&mut panned, &mut centered] {
            engine.voices[0].state[0][0] = Complex32::new(0.7, -0.2);
            engine.voices[0].state[1][0] = Complex32::new(0.1, 0.4);
        }
        let gains = panned.voices[0].pan_gains;
        let mut energy = [0.0_f64; 2];
        for _ in 0..12_000 {
            let mut pan = [0.0; 2];
            let mut center = [0.0; 2];
            panned.process_poly_frame(pan.iter_mut(), spread_controls);
            centered.process_poly_frame(center.iter_mut(), base_controls);
            for channel in 0..2 {
                assert!((pan[channel] - center[channel] * gains[channel]).abs() < 3e-6);
                energy[channel] += f64::from(center[channel]).powi(2);
            }
        }
        assert!(energy.into_iter().all(|x| x > 0.01));
        assert_eq!(
            panned.voices[0].state, centered.voices[0].state,
            "Pan changed resonance rather than output balance"
        );
    }
    let mut a = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
    let mut b = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
    a.note_on(69, 1, 1.0);
    b.note_on(69, 1, 1.0);
    for i in 0..12_000 {
        let mut x = (i as f32 * 0.0576).sin() * 0.3;
        let mut y = x;
        a.process_poly_frame(
            [&mut x],
            ResonatorControls {
                voice_spread: 1.0,
                ..controls()
            },
        );
        b.process_poly_frame([&mut y], controls());
        assert_eq!(x, y, "mono audio was attenuated by Spread");
    }
}

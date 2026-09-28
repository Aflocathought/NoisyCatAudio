//! The failure was wet pre-echo before the aligned dry impulse, not a mismatch
//! in the host latency report. Check rendered samples, including Post Unison.
use super::*;

#[test]
fn half_and_full_alignment_shift_exactly_and_full_wet_never_precedes_dry() {
    for rate in [44100.0, 48000.0, 96000.0] {
        for offset in [0, 255, 511] {
            for mode in [UnisonMode::Spectral, UnisonMode::Post] {
                for alignment in [0.5, 1.0] {
                    let mut original = SpectralResonator::new(2, 4096, 512, rate).unwrap();
                    let mut aligned = SpectralResonator::new(2, 4096, 512, rate).unwrap();
                    // Include an impulse on the very first input sample: startup
                    // must not briefly crossfade through the early legacy tap.
                    let input_at = if offset == 0 { 0 } else { 8192 + offset };
                    let dry_at = input_at + 4096;
                    let controls = ResonatorControls {
                        note: Some(69),
                        harmonics: 8,
                        hf_damp: 0.0,
                        wet_level: 1.0,
                        attack_ms: if offset == 0 { None } else { Some(0.0) },
                        unison_mode: mode,
                        modulation: ModulationControls {
                            unison_voices: 3,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let mut wet_history = Vec::new();
                    let mut early_energy = 0.0;
                    let mut aligned_energy = 0.0;
                    for i in 0_usize..dry_at + 8192 {
                        let input = if i == input_at { 1.0 } else { 0.0 };
                        let mut a = [input, 0.0];
                        let mut b = a;
                        original.process_frame(a.iter_mut(), controls);
                        aligned.process_frame(
                            b.iter_mut(),
                            ResonatorControls {
                                align_wet: alignment,
                                ..controls
                            },
                        );
                        let (dry, wet) = original.output_parts();
                        let (aligned_dry, aligned_wet) = aligned.output_parts();
                        assert_eq!(dry, aligned_dry, "alignment changed dry audio");
                        assert_eq!(aligned_wet[1], 0.0, "wet delay crossfed stereo");
                        wet_history.push(wet[0]);
                        let delay = (alignment * 4096.0) as usize;
                        let expected = i.checked_sub(delay).map_or(0.0, |j| wet_history[j]);
                        assert_eq!(
                            aligned_wet[0], expected,
                            "wet waveform changed beyond its delay"
                        );
                        if i < dry_at {
                            early_energy += wet[0] * wet[0];
                            if alignment == 1.0 {
                                assert_eq!(
                                    aligned_wet[0], 0.0,
                                    "wet led dry: {rate}/{offset}/{mode:?}"
                                );
                            }
                        }
                        aligned_energy += aligned_wet[0] * aligned_wet[0];
                    }
                    assert!(
                        early_energy > 1e-9,
                        "fixture did not expose original pre-echo"
                    );
                    assert!(
                        aligned_energy > 1e-7,
                        "aligned wet was muted instead of delayed"
                    );
                    assert_eq!(aligned.latency_samples(), 4096);
                    assert!(aligned.effect_tail_samples() >= 4096);
                    aligned.reset();
                    for _ in 0..12288 {
                        let mut silent = [0.0; 2];
                        aligned.process_frame(
                            silent.iter_mut(),
                            ResonatorControls {
                                align_wet: alignment,
                                ..controls
                            },
                        );
                        assert_eq!(silent, [0.0; 2]);
                    }
                }
            }
        }
    }
}

#[test]
fn switching_alignment_keeps_running_tails_and_reaches_each_tap() {
    let mut reference = SpectralResonator::new(1, 4096, 512, 48000.0).unwrap();
    let mut switched = SpectralResonator::new(1, 4096, 512, 48000.0).unwrap();
    let mut wet = Vec::new();
    for i in 0..48000 {
        let controls = ResonatorControls {
            harmonics: 4,
            t60: 2.0,
            ..Default::default()
        };
        let input = if i < 12000 {
            (i as f32 * 0.0576).sin() * 0.2
        } else {
            0.0
        };
        reference.process_frame([&mut { input }], controls);
        let aligned = if (16000..28000).contains(&i) {
            0.5
        } else if (28000..40000).contains(&i) {
            1.0
        } else {
            0.0
        };
        switched.process_frame(
            [&mut { input }],
            ResonatorControls {
                align_wet: aligned,
                ..controls
            },
        );
        let sample = switched.output_parts().1[0];
        wet.push(reference.output_parts().1[0]);
        assert!(sample.is_finite());
        if (18000..28000).contains(&i) {
            assert_eq!(sample, wet[i - 2048]);
        }
        if (30000..40000).contains(&i) {
            assert_eq!(sample, wet[i - 4096]);
        }
        if i >= 42000 {
            assert_eq!(sample, wet[i]);
        }
        assert_eq!(reference.output_parts().0, switched.output_parts().0);
        assert_eq!(
            reference.active_voice_count(),
            switched.active_voice_count()
        );
    }
}

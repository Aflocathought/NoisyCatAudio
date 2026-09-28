//! Rendered transient checks: input-reference timing, independent stereo,
//! sample-exact legacy bypass, and tail preservation after the accent expires.
use super::*;

#[test]
fn reshape_removes_isolated_pre_echo_and_preserves_dry_and_late_tail() {
    for rate in [44100.0, 48000.0, 96000.0] {
        for t60 in [0.005, 2.0] {
            for mode in [UnisonMode::Spectral, UnisonMode::Post] {
                let mut plain = SpectralResonator::new(2, 4096, 512, rate).unwrap();
                let mut shaped = SpectralResonator::new(2, 4096, 512, rate).unwrap();
                let input_at = if t60 == 0.005 { 0 } else { 8192 + 127 };
                let dry_at = input_at + 4096;
                let mut early_energy = 0.0;
                let mut wet_energy = 0.0;
                let mut peak = (0.0, 0);
                for i in 0..dry_at + rate as usize {
                    let controls = ResonatorControls {
                        note: Some(69),
                        harmonics: 8,
                        t60,
                        attack_ms: Some(0.0),
                        hf_damp: 0.0,
                        wet_level: 1.0,
                        mid_mix: 0.5,
                        align_wet: 0.0,
                        unison_mode: mode,
                        modulation: ModulationControls {
                            unison_voices: 3,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let input = if i == input_at { 0.1 } else { 0.0 };
                    let mut a = [input, 0.0];
                    let mut b = a;
                    plain.process_frame(a.iter_mut(), controls);
                    shaped.process_frame(
                        b.iter_mut(),
                        ResonatorControls {
                            transient: Some(TransientControls {
                                attack_ms: 0.0,
                                emphasis: 2.0,
                            }),
                            ..controls
                        },
                    );
                    let (dry, wet) = shaped.output_parts();
                    assert_eq!(dry, plain.output_parts().0);
                    assert_eq!(wet[1], 0.0);
                    assert!(wet[0].is_finite());
                    if i < dry_at {
                        early_energy += plain.output_parts().1[0].powi(2);
                        assert_eq!(wet[0], 0.0, "shaped wet precedes delayed input");
                    }
                    wet_energy += wet[0].powi(2);
                    if wet[0].abs() > peak.0 {
                        peak = (wet[0].abs(), i);
                    }
                    if i > dry_at + (rate * 0.5) as usize {
                        assert_eq!(wet, plain.output_parts().1, "shaper altered the late tail");
                    }
                }
                assert!(early_energy > 1e-12 && wet_energy > 1e-10);
                eprintln!(
                    "reshape rate={rate} t60={t60} mode={mode:?} peak_after_dry_ms={:.3}",
                    (peak.1 - dry_at) as f32 / rate * 1000.0
                );
                assert_eq!(shaped.latency_samples(), 4096);
                shaped.reset();
                for _ in 0..8192 {
                    let mut audio = [0.0; 2];
                    shaped.process_frame(
                        audio.iter_mut(),
                        ResonatorControls {
                            transient: Some(TransientControls::default()),
                            ..Default::default()
                        },
                    );
                    assert_eq!(audio, [0.0; 2]);
                }
            }
        }
    }
}

#[test]
fn repeated_onset_emphasis_does_not_gate_or_shorten_an_existing_tail() {
    use crate::transient::TransientShaper;
    let mut shaper = TransientShaper::new(48000.0);
    let mut emphasized = [false; 2];
    let controls = Some(TransientControls {
        attack_ms: 0.0,
        emphasis: 2.0,
    });
    for i in 0..48000 {
        // A constant wet fixture isolates gain movement from oscillator phase.
        // The right tail receives no second input strike and must stay at unity.
        let reference = [
            if i == 0 || i == 24000 { 1.0 } else { 0.0 },
            if i == 0 { 1.0 } else { 0.0 },
        ];
        let wet = shaper.process([0.1; 2], reference, controls);
        assert!(wet.iter().all(|v| v.is_finite() && *v >= 0.0 && *v <= 0.2));
        if i > 100 {
            assert!(wet[0] >= 0.1, "repeated onset chopped the tail");
        }
        if (24000..28000).contains(&i) {
            emphasized[0] |= wet[0] > 0.15;
            assert_eq!(wet[1], 0.1, "left onset changed the right tail");
        }
        if i > 40000 {
            assert_eq!(wet, [0.1; 2]);
            emphasized[1] = true;
        }
    }
    assert_eq!(emphasized, [true; 2]);
}

#[test]
fn attack_controls_wet_rise_and_mode_switch_returns_to_exact_bypass() {
    use crate::transient::TransientShaper;
    for attack_ms in [0.0, 10.0, 100.0, 2000.0] {
        let rate = 48000.0;
        let mut shaper = TransientShaper::new(rate);
        let mut crossing = None;
        for i in 0..150000 {
            let controls = if i < 130000 {
                Some(TransientControls {
                    attack_ms,
                    emphasis: 1.0,
                })
            } else {
                None
            };
            let sample = shaper.process([0.2, 0.0], [0.1, 0.0], controls)[0];
            if sample >= 0.18 && crossing.is_none() {
                crossing = Some(i + 1);
            }
            if i > 131000 {
                assert_eq!(sample, 0.2);
            }
        }
        let expected = attack_ms.max(0.25) * rate / 1000.0;
        assert!(
            (crossing.unwrap() as f32 - expected).abs() <= 150.0,
            "rise time {attack_ms}: {crossing:?}"
        );
    }
    // The reference only controls gain. Even loud source transients can never
    // appear in the output when the resonant wet input is silent.
    let mut shaper = TransientShaper::new(48000.0);
    for i in 0..48000 {
        let reference = [(i as f32 * 0.07).sin(), (i as f32 * 0.11).cos()];
        assert_eq!(
            shaper.process([0.0; 2], reference, Some(TransientControls::default())),
            [0.0; 2]
        );
    }
}

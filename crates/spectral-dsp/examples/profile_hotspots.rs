//! Coarse stage timings are opt-in and never included in the normal plugin.
//! Run with --features profiling; timings exclude setup and file writes.
use spectral_dsp::{
    DecayCurve, ModulationControls, ModulationMode, ResonatorControls, SpectralResonator,
};
use std::time::Instant;

fn main() {
    let mut engine = SpectralResonator::new(2, 4096, 512, 48_000.0).unwrap();
    for (name, mode, unison, automate) in [
        ("off-1", ModulationMode::Off, 1, false),
        ("granular-4", ModulationMode::Granular, 4, false),
        (
            "granular-4-curve-automation",
            ModulationMode::Granular,
            4,
            true,
        ),
    ] {
        engine.reset();
        #[cfg(feature = "profiling")]
        engine.take_profile();
        for voice in 0..8 {
            engine.note_on(33 + voice, voice as u64 + 1, 1.0);
        }
        let mut controls = ResonatorControls {
            note: None,
            harmonics: 256,
            decay_curve: Some(DecayCurve::default()),
            modulation: ModulationControls {
                mode,
                unison_voices: unison,
                rate_hz: 3.0,
                amount: 0.7,
                pitch_semitones: 0.2,
                unison_detune_cents: 12.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut seed = 1234_u32;
        let mut audio: Vec<[f32; 2]> = (0..48_000 * 4)
            .map(|_| {
                std::array::from_fn(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed as f64 / u32::MAX as f64 - 0.5) as f32 * 0.4
                })
            })
            .collect();
        let start = Instant::now();
        for (i, frame) in audio.iter_mut().enumerate() {
            if automate {
                controls.decay_curve.as_mut().unwrap().points[2].seconds =
                    1.5 + (i as f32 / 24_000.0).sin();
            }
            engine.process_poly_frame(frame.iter_mut(), controls);
        }
        println!(
            "{name}: wall_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        assert!(audio.iter().flatten().all(|x| x.is_finite()));
        #[cfg(feature = "profiling")]
        {
            let profile = engine.take_profile();
            let total: f64 = profile.iter().map(|t| t.as_secs_f64()).sum();
            for (label, time) in [
                "controls/decay",
                "modulation prepare",
                "input FFT",
                "projection/state",
                "wet synthesis",
                "mix/IFFT/OLA",
            ]
            .into_iter()
            .zip(profile)
            {
                println!(
                    "  {label}: {:.3} ms ({:.1}%)",
                    time.as_secs_f64() * 1000.0,
                    time.as_secs_f64() / total * 100.0
                );
            }
        }
    }
}

//! Measure wet pre-echo against the declared dry latency at different hop phases.
use spectral_dsp::{ResonatorControls, SpectralResonator};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};
fn main() {
    let mut alignment = 0.0;
    let mut decay_seconds = 2.0;
    let mut csv_dir = None;
    let mut reshape_attack = None;
    let mut emphasis_db = 6.0;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--aligned" => alignment = 1.0,
            "--alignment" => {
                alignment = args
                    .next()
                    .expect("expected window fraction")
                    .parse::<f32>()
                    .expect("invalid window fraction")
            }
            "--decay-seconds" => {
                decay_seconds = args
                    .next()
                    .expect("expected decay seconds")
                    .parse::<f32>()
                    .expect("invalid decay seconds");
            }
            "--csv-dir" => {
                csv_dir = Some(PathBuf::from(args.next().expect("expected CSV directory")))
            }
            "--reshape-attack-ms" => {
                reshape_attack = Some(
                    args.next()
                        .expect("expected attack ms")
                        .parse::<f32>()
                        .expect("invalid attack ms"),
                )
            }
            "--emphasis-db" => {
                emphasis_db = args
                    .next()
                    .expect("expected emphasis dB")
                    .parse::<f32>()
                    .expect("invalid emphasis dB")
            }
            _ => panic!("unknown argument: {arg}"),
        }
    }
    assert!((0.0..=1.0).contains(&alignment));
    assert!((0.005..=12.0).contains(&decay_seconds));
    assert!((0.0..=12.0).contains(&emphasis_db));
    assert!(reshape_attack.is_none_or(|ms| (0.0..=2000.0).contains(&ms)));
    if let Some(dir) = &csv_dir {
        std::fs::create_dir_all(dir).unwrap();
    }
    for attack_ms in [None, Some(0.0)] {
        for offset in [0, 127, 255, 383, 511] {
            let mut engine = SpectralResonator::new(1, 4096, 512, 48000.0).unwrap();
            let controls = ResonatorControls {
                note: Some(69),
                harmonics: 8,
                t60: decay_seconds,
                attack_ms,
                transient: reshape_attack.map(|attack_ms| spectral_dsp::TransientControls {
                    attack_ms,
                    emphasis: 10.0_f32.powf(emphasis_db / 20.0),
                }),
                hf_damp: 0.0,
                wet_level: 1.0,
                mid_mix: 0.5,
                align_wet: alignment,
                low_hz: 1.0,
                high_hz: 12000.0,
                ..Default::default()
            };
            let input_at = 8192 + offset;
            let dry_at = input_at + engine.latency_samples() as usize;
            // Capture the actual dry and wet contributions of an internal 50%
            // mix. CSV I/O happens only after rendering, outside the DSP path.
            let rendered: Vec<_> = (0..dry_at + 12000)
                .map(|i| {
                    let mut x = if i == input_at { 1.0 } else { 0.0 };
                    engine.process_frame([&mut x], controls);
                    let (dry, wet) = engine.output_parts();
                    [dry[0], wet[0]]
                })
                .collect();
            let wet: Vec<_> = rendered.iter().map(|parts| parts[1]).collect();
            let peak = wet.iter().fold(0.0_f32, |p, x| p.max(x.abs()));
            let onset = wet.iter().position(|x| x.abs() >= peak * 0.001).unwrap();
            let peak_at = wet.iter().position(|x| x.abs() == peak).unwrap();
            println!(
                "alignment={alignment} decay={decay_seconds} attack={attack_ms:?} reshape_ms={reshape_attack:?} emphasis_db={emphasis_db} hop_offset={offset} wet_lead_samples={} wet_lead_ms={:.3} wet_peak_after_dry_ms={:.3}",
                dry_at as i32 - onset as i32,
                (dry_at as f32 - onset as f32) / 48.0,
                (peak_at as f32 - dry_at as f32) / 48.0
            );
            if let Some(dir) = &csv_dir {
                let attack = if attack_ms.is_some() {
                    "fast"
                } else {
                    "natural"
                };
                let path = dir.join(format!("{attack}-offset-{offset}.csv"));
                let mut out = BufWriter::new(File::create(path).unwrap());
                writeln!(out, "sample,time_from_dry_ms,input,dry,wet").unwrap();
                for (i, parts) in rendered.iter().enumerate() {
                    let impulse = usize::from(i == input_at);
                    writeln!(
                        out,
                        "{i},{:.6},{impulse},{},{}",
                        (i as f64 - dry_at as f64) / 48.0,
                        parts[0],
                        parts[1]
                    )
                    .unwrap();
                }
            }
        }
    }
}

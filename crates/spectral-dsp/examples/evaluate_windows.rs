//! Offline window-size evaluation. Uses the current production DSP at different
//! N/H values without changing plugin constants, host latency, or installed DLLs.
use spectral_dsp::{
    ModulationControls, ModulationMode, ResonatorControls, SpectralResonator, UnisonMode,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

const RATE: usize = 48000;
const CONFIGS: [(usize, usize); 6] = [
    (4096, 512),
    (2048, 256),
    (1024, 128),
    (2048, 512),
    (1024, 256),
    (1024, 512),
];
fn hz(note: i32) -> f64 {
    440.0 * 2.0_f64.powf((note - 69) as f64 / 12.0)
}
fn rms(audio: &[f32]) -> f64 {
    (audio.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / audio.len() as f64).sqrt()
}
fn db(value: f64) -> f64 {
    20.0 * value.max(1e-15).log10()
}
fn tail_residual(audio: &[f32], frequency: f64) -> f64 {
    // Fit the known target sine/cosine after undoing the intended T60 envelope.
    // Residual includes frame ripple and spectral images, beyond pitch alone.
    let mut ss = 0.0;
    let mut cc = 0.0;
    let mut sc = 0.0;
    let mut ys = 0.0;
    let mut yc = 0.0;
    for (i, &sample) in audio.iter().enumerate() {
        let phase = std::f64::consts::TAU * frequency * i as f64 / RATE as f64;
        let (s, c) = phase.sin_cos();
        let y = sample as f64 * 10.0_f64.powf(3.0 * i as f64 / (RATE as f64 * 2.0));
        ss += s * s;
        cc += c * c;
        sc += s * c;
        ys += y * s;
        yc += y * c;
    }
    let determinant = ss * cc - sc * sc;
    let a = (ys * cc - yc * sc) / determinant;
    let b = (yc * ss - ys * sc) / determinant;
    let mut error = 0.0;
    let mut total = 0.0;
    for (i, &sample) in audio.iter().enumerate() {
        let (s, c) = (std::f64::consts::TAU * frequency * i as f64 / RATE as f64).sin_cos();
        let y = sample as f64 * 10.0_f64.powf(3.0 * i as f64 / (RATE as f64 * 2.0));
        error += (y - a * s - b * c).powi(2);
        total += y * y;
    }
    db((error / total).sqrt())
}
fn controls(note: i32) -> ResonatorControls {
    ResonatorControls {
        note: Some(note),
        harmonics: 1,
        t60: 2.0,
        hf_damp: 0.0,
        lf_damp: 0.0,
        low_hz: 1.0,
        high_hz: 12000.0,
        wet_level: 1.0,
        mid_mix: 1.0,
        align_wet: 0.0,
        ..Default::default()
    }
}

fn render(
    engine: &mut SpectralResonator,
    input: &[f32],
    settings: ResonatorControls,
    stop: usize,
) -> Vec<f32> {
    engine.reset();
    input
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let mut x = x;
            engine.process_frame(
                [&mut x],
                ResonatorControls {
                    note: if i < stop { settings.note } else { None },
                    ..settings
                },
            );
            assert!(x.is_finite());
            engine.output_parts().1[0]
        })
        .collect()
}

fn wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    let bytes = samples.len() as u32 * 4;
    file.write_all(b"RIFF")?;
    file.write_all(&(bytes + 36).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&(RATE as u32).to_le_bytes())?;
    file.write_all(&(RATE as u32 * 4).to_le_bytes())?;
    file.write_all(&4_u16.to_le_bytes())?;
    file.write_all(&32_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}

fn main() -> std::io::Result<()> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/window-evaluation".into());
    let root = Path::new(&output);
    std::fs::create_dir_all(root)?;
    let mut tails = BufWriter::new(File::create(root.join("tails.csv"))?);
    let mut response = BufWriter::new(File::create(root.join("selectivity.csv"))?);
    let mut impulses = BufWriter::new(File::create(root.join("impulses.csv"))?);
    writeln!(
        tails,
        "size,hop,note,hz,pitch_cents,t60_error_percent,residual_db"
    )?;
    writeln!(response, "size,hop,note,hz,mode,t60,input_ratio,wet_rms")?;
    writeln!(
        impulses,
        "size,hop,mode,offset,onset_ms,peak_ms,pre_energy_percent,width90_ms"
    )?;
    for (size, hop) in CONFIGS {
        let mut engine = SpectralResonator::new(1, size, hop, RATE as f32).unwrap();
        for note in [33, 50, 51, 57, 69, 93] {
            let stop = RATE;
            let input: Vec<_> = (0..RATE * 3)
                .map(|i| {
                    if i < stop {
                        (0.1 * (std::f64::consts::TAU * hz(note) * i as f64 / RATE as f64).sin())
                            as f32
                    } else {
                        0.0
                    }
                })
                .collect();
            let audio = render(&mut engine, &input, controls(note), stop);
            // Exclude the whole analysis/synthesis flush. Only free decay is
            // measured, so source pitch cannot conceal a wrong resonant pitch.
            let start = stop + 2 * size + RATE / 10;
            let crossings: Vec<_> = (start..start + RATE)
                .filter_map(|i| {
                    let (a, b) = (audio[i] as f64, audio[i + 1] as f64);
                    (a <= 0.0 && b > 0.0).then(|| i as f64 - a / (b - a))
                })
                .collect();
            assert!(crossings.len() > 20);
            let frequency = RATE as f64 * (crossings.len() - 1) as f64
                / (crossings.last().unwrap() - crossings[0]);
            let ratio = rms(&audio[start + RATE / 2..start + RATE / 2 + RATE / 5])
                / rms(&audio[start..start + RATE / 5]);
            let measured_t60 = -3.0 * 0.5 / ratio.log10();
            writeln!(
                tails,
                "{size},{hop},{note},{},{},{:.6},{:.6}",
                hz(note),
                1200.0 * (frequency / hz(note)).log2(),
                (measured_t60 / 2.0 - 1.0) * 100.0,
                tail_residual(&audio[start..start + RATE / 4], hz(note))
            )?;
        }
        for note in [33, 50, 69] {
            for (mode, attack, t60) in [
                ("natural", None, 2.0),
                ("fast", Some(0.0), 2.0),
                ("short", None, 0.005),
            ] {
                for ratio in [1.0, 2.0_f64.powf(1.0 / 12.0), 2.0] {
                    let input: Vec<_> = (0..RATE * 2)
                        .map(|i| {
                            (0.1 * (std::f64::consts::TAU * hz(note) * ratio * i as f64
                                / RATE as f64)
                                .sin()) as f32
                        })
                        .collect();
                    let audio = render(
                        &mut engine,
                        &input,
                        ResonatorControls {
                            t60,
                            attack_ms: attack,
                            ..controls(note)
                        },
                        input.len(),
                    );
                    writeln!(
                        response,
                        "{size},{hop},{note},{},{mode},{t60},{ratio},{}",
                        hz(note),
                        rms(&audio[RATE * 3 / 2..])
                    )?;
                }
            }
        }
        for (mode, attack, t60) in [
            ("natural", None, 2.0),
            ("fast", Some(0.0), 2.0),
            ("short", None, 0.005),
        ] {
            for offset in [0, hop / 2, hop - 1] {
                let at = 8192 + offset;
                let dry_at = at + size;
                let input: Vec<_> = (0..dry_at + RATE / 2)
                    .map(|i| if i == at { 1.0 } else { 0.0 })
                    .collect();
                let audio = render(
                    &mut engine,
                    &input,
                    ResonatorControls {
                        harmonics: 8,
                        t60,
                        attack_ms: attack,
                        ..controls(69)
                    },
                    input.len(),
                );
                let peak = audio.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
                let onset = audio.iter().position(|v| v.abs() >= peak * 0.001).unwrap();
                let peak_at = audio.iter().position(|v| v.abs() == peak).unwrap();
                let total: f64 = audio.iter().map(|&v| (v as f64).powi(2)).sum();
                let pre: f64 = audio[..dry_at].iter().map(|&v| (v as f64).powi(2)).sum();
                let mut energy = 0.0;
                let mut p5 = None;
                let mut p95 = None;
                for (i, &v) in audio.iter().enumerate() {
                    energy += (v as f64).powi(2);
                    if energy >= total * 0.05 && p5.is_none() {
                        p5 = Some(i);
                    }
                    if energy >= total * 0.95 && p95.is_none() {
                        p95 = Some(i);
                    }
                }
                writeln!(
                    impulses,
                    "{size},{hop},{mode},{offset},{:.6},{:.6},{:.6},{:.6}",
                    (onset as f64 - dry_at as f64) / 48.0,
                    (peak_at as f64 - dry_at as f64) / 48.0,
                    pre / total * 100.0,
                    (p95.unwrap() - p5.unwrap()) as f64 / 48.0
                )?;
                if offset == 0 && mode == "short" {
                    let mut csv = BufWriter::new(File::create(
                        root.join(format!("impulse-{size}-{hop}.csv")),
                    )?);
                    writeln!(csv, "time_from_dry_ms,wet")?;
                    for (i, &v) in audio.iter().enumerate() {
                        writeln!(csv, "{:.6},{v}", (i as f64 - dry_at as f64) / 48.0)?;
                    }
                }
            }
        }
        // Same percussive noise fixture at identical gain. Save wet only; file
        // creation and input generation are outside any benchmark interval.
        let mut seed = 1234_u32;
        let source: Vec<_> = (0..RATE * 3)
            .map(|i| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let noise = (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32;
                if i < RATE * 2 {
                    noise * (-((i % 12000) as f32) / 700.0).exp() * 0.2
                } else {
                    0.0
                }
            })
            .collect();
        let audio = render(
            &mut engine,
            &source,
            ResonatorControls {
                harmonics: 256,
                t60: 0.4,
                attack_ms: Some(0.0),
                ..controls(50)
            },
            source.len(),
        );
        wav(&root.join(format!("wet-drums-{size}-{hop}.wav")), &audio)?;
        println!(
            "quality N={size} H={hop} completed; latency={:.3} ms",
            size as f64 / 48.0
        );
    }
    tails.flush()?;
    response.flush()?;
    impulses.flush()?;
    if std::env::args().any(|arg| arg == "--quality-only") {
        return Ok(());
    }

    let mut bench = BufWriter::new(File::create(root.join("performance.csv"))?);
    writeln!(
        bench,
        "size,hop,scenario,trial,audio_ms,wall_ms,p95_block_ms,p99_block_ms,max_block_ms,deadline_ms"
    )?;
    // Rotate order across three trials to reduce fixed-order warmup bias. This
    // is serial offline wall time, not a claim about a DAW's real-time safety.
    for trial in 0..3 {
        for config in 0..CONFIGS.len() {
            let (size, hop) = CONFIGS[(config + trial) % CONFIGS.len()];
            let mut engine = SpectralResonator::new(2, size, hop, RATE as f32).unwrap();
            for (name, voices, unison, mode, algorithm) in [
                ("single", 1, 1, ModulationMode::Off, UnisonMode::Spectral),
                (
                    "16voice-8spectral",
                    16,
                    8,
                    ModulationMode::Granular,
                    UnisonMode::Spectral,
                ),
                (
                    "16voice-8post",
                    16,
                    8,
                    ModulationMode::Granular,
                    UnisonMode::Post,
                ),
            ] {
                engine.reset();
                for v in 0..voices {
                    engine.note_on(50 + v, v as u64 + 1, 1.0);
                }
                let settings = ResonatorControls {
                    note: None,
                    harmonics: 256,
                    t60: 0.4,
                    attack_ms: Some(0.0),
                    unison_mode: algorithm,
                    modulation: ModulationControls {
                        mode,
                        unison_voices: unison,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut seed = 777_u32;
                let mut audio: Vec<[f32; 2]> = (0..RATE / 2)
                    .map(|_| {
                        std::array::from_fn(|_| {
                            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                            (seed as f64 / u32::MAX as f64 - 0.5) as f32 * 0.1
                        })
                    })
                    .collect();
                for i in 0..RATE / 4 {
                    let mut x = audio[i % audio.len()];
                    engine.process_poly_frame(x.iter_mut(), settings);
                }
                let mut times = Vec::with_capacity(audio.len().div_ceil(128));
                let start = Instant::now();
                for block in audio.chunks_mut(128) {
                    let t = Instant::now();
                    for frame in block {
                        engine.process_poly_frame(frame.iter_mut(), settings);
                    }
                    times.push(t.elapsed().as_secs_f64() * 1000.0);
                }
                let wall = start.elapsed().as_secs_f64() * 1000.0;
                assert!(audio.iter().flatten().all(|v| v.is_finite()));
                times.sort_by(f64::total_cmp);
                writeln!(
                    bench,
                    "{size},{hop},{name},{trial},500,{wall:.6},{:.6},{:.6},{:.6},{:.6}",
                    times[times.len() * 95 / 100],
                    times[times.len() * 99 / 100],
                    times.last().unwrap(),
                    128.0 / 48.0
                )?;
                println!(
                    "bench N={size} H={hop} {name} trial={trial}: {wall:.1} ms / 500 ms audio"
                );
            }
        }
    }
    bench.flush()?;
    // Keep the helper used here so diagnostics always use amplitude dB.
    println!(
        "Finished. Reference RMS 0.1 sine: {:.3} dBFS",
        db(0.1 / std::f64::consts::SQRT_2)
    );
    Ok(())
}

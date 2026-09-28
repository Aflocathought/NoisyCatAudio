//! Reproducible wet-only onset/short-decay audition. No live host or speakers.
//! The WAVs share input amplitude and gain; no per-file normalization is used.
use spectral_dsp::{ResonatorControls, SpectralResonator};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

fn wav(path: &Path, audio: &[f32], rate: u32) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    let bytes = audio.len() as u32 * 4;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 4).to_le_bytes())?;
    file.write_all(&4_u16.to_le_bytes())?;
    file.write_all(&32_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for sample in audio {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}

fn main() -> std::io::Result<()> {
    let destination = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/envelope-0.11.0/audio".into());
    let destination = Path::new(&destination);
    std::fs::create_dir_all(destination)?;
    let rate = 48000;
    let mut report = BufWriter::new(File::create(destination.join("measurements.csv"))?);
    writeln!(
        report,
        "stimulus,response,decay_s,t90_from_input_ms,peak,energy"
    )?;
    for transient in [false, true] {
        let stimulus = if transient { "bursts" } else { "sustained" };
        let input: Vec<f32> = (0..rate * 4)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let amplitude = if transient {
                    // Three short audio onsets within the same held internal note.
                    if (0.2..1.8).contains(&t) && (t - 0.2) % 0.6 < 0.03 {
                        0.2
                    } else {
                        0.0
                    }
                } else if (0.2..1.5).contains(&t) {
                    0.2
                } else {
                    0.0
                };
                (amplitude * (std::f64::consts::TAU * 440.0 * t).sin()) as f32
            })
            .collect();
        wav(
            &destination.join(format!("{stimulus}_input.wav")),
            &input,
            rate as u32,
        )?;
        for (name, attack_ms, t60, reshape) in [
            ("natural", None, 2.0, None),
            ("attack0", Some(0.0), 2.0, None),
            ("attack10", Some(10.0), 2.0, None),
            ("attack400", Some(400.0), 2.0, None),
            ("natural", None, 0.05, None),
            ("natural", None, 0.005, None),
            ("attack10", Some(10.0), 0.005, None),
            ("reshape0", None, 2.0, Some(0.0)),
            ("reshape10", None, 2.0, Some(10.0)),
            ("reshape100", None, 2.0, Some(100.0)),
            ("reshape0", None, 0.005, Some(0.0)),
        ] {
            let mut engine = SpectralResonator::new(1, 4096, 512, rate as f32).unwrap();
            let controls = ResonatorControls {
                note: Some(69),
                harmonics: 1,
                attack_ms,
                transient: reshape.map(|attack_ms| spectral_dsp::TransientControls {
                    attack_ms,
                    emphasis: 2.0,
                }),
                t60,
                hf_damp: 0.0,
                wet_level: 1.0,
                low_hz: 1.0,
                high_hz: 12000.0,
                ..Default::default()
            };
            let output: Vec<f32> = input
                .iter()
                .map(|&v| {
                    let mut sample = v;
                    engine.process_frame([&mut sample], controls);
                    engine.output_parts().1[0]
                })
                .collect();
            let crossing = if transient {
                None
            } else {
                (rate / 5..rate * 3 / 2).step_by(48).find(|&i| {
                    let rms =
                        (output[i..i + 480].iter().map(|v| v * v).sum::<f32>() / 480.0).sqrt();
                    rms >= 0.2 * std::f32::consts::FRAC_1_SQRT_2 * 0.9
                })
            };
            let ms = crossing
                .map(|i| ((i - rate / 5) as f32 / rate as f32 * 1000.0).to_string())
                .unwrap_or_default();
            let peak = output.iter().fold(0.0_f32, |p, v| p.max(v.abs()));
            let energy = output.iter().map(|&v| (v as f64).powi(2)).sum::<f64>();
            assert!(output.iter().all(|v| v.is_finite()));
            writeln!(report, "{stimulus},{name},{t60},{ms},{peak},{energy}")?;
            wav(
                &destination.join(format!("{stimulus}_{name}_decay{t60}.wav")),
                &output,
                rate as u32,
            )?;
        }
    }
    report.flush()
}

//! Reproducible stereo fixture and a bounded, offline DSP block-time benchmark.
//! File writing, timing collection and input generation stay outside the DSP.
use spectral_dsp::{ResonatorControls, SpectralResonator};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    time::Instant,
};

fn wav(path: &Path, audio: &[[f32; 2]], rate: u32) -> std::io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    let bytes = (audio.len() * 8) as u32;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&3_u16.to_le_bytes())?; // IEEE float, stereo, 32-bit.
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 8).to_le_bytes())?;
    file.write_all(&8_u16.to_le_bytes())?;
    file.write_all(&32_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for frame in audio {
        for sample in frame {
            file.write_all(&sample.to_le_bytes())?;
        }
    }
    file.flush()
}

fn main() -> std::io::Result<()> {
    let destination = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/fixtures/m3".into());
    std::fs::create_dir_all(&destination)?;
    let rate = 48_000_usize;
    let frames = rate * 8;
    let mut input = vec![[0.0_f32; 2]; frames];
    let mut seed = 0x1234_5678_u32;
    for (i, frame) in input.iter_mut().enumerate() {
        for (channel, sample) in frame.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32;
            let beat = (i + channel * 6000) % 12_000;
            if i < rate * 5 {
                *sample = noise * (-(beat as f32) / 1200.0).exp() * 0.45;
            }
        }
    }
    let prepared = Instant::now();
    let mut engine = SpectralResonator::new(2, 4096, 512, rate as f32).unwrap();
    let prepare_ms = prepared.elapsed().as_secs_f64() * 1000.0;
    let mut output = input.clone();
    for (i, frame) in output.iter_mut().enumerate() {
        let controls = ResonatorControls {
            note: if i < rate * 5 {
                Some([57, 60, 64, 67, 69][i / rate])
            } else {
                None
            },
            harmonics: 64,
            t60: 1.2,
            wet_level: 8.0,
            input_gain: 2.0,
            ..Default::default()
        };
        engine.process_frame(frame.iter_mut(), controls);
    }
    let peak = output
        .iter()
        .flatten()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    // This gain affects the audition fixture only, never the plugin algorithm.
    let audition_gain = if peak > 0.8 { 0.8 / peak } else { 1.0 };
    for sample in output.iter_mut().flatten() {
        *sample *= audition_gain;
    }
    wav(
        &Path::new(&destination).join("input-stereo.wav"),
        &input,
        rate as u32,
    )?;
    wav(
        &Path::new(&destination).join("resonated-stereo.wav"),
        &output,
        rate as u32,
    )?;
    println!(
        "prepare_ms={prepare_ms:.3}, fixture_peak_before_gain={peak:.6}, fixture_gain={audition_gain:.6}"
    );

    // Time only the preallocated engine and prepared audio, with all 64 low
    // note partials enabled. Block includes FFT work every fourth callback.
    for changing in [false, true] {
        engine.reset();
        let mut timings = Vec::with_capacity(frames / 128);
        let mut work = input.clone();
        let mut controls = ResonatorControls {
            note: Some(33),
            harmonics: 64,
            ..Default::default()
        };
        for (block_index, block) in work.chunks_mut(128).enumerate() {
            if changing {
                controls.note = Some(33 + (block_index / 4 % 12) as i32);
            }
            let start = Instant::now();
            for frame in block {
                engine.process_frame(frame.iter_mut(), controls);
            }
            timings.push(start.elapsed().as_secs_f64() * 1e6);
        }
        timings.sort_by(f64::total_cmp);
        let mean = timings.iter().sum::<f64>() / timings.len() as f64;
        println!(
            "mode={}, block=128, mean_us={mean:.3}, p50_us={:.3}, p99_us={:.3}, max_us={:.3}, audio_budget_us={:.3}",
            if changing {
                "note-change-every-hop"
            } else {
                "steady-64-partials"
            },
            timings[timings.len() / 2],
            timings[timings.len() * 99 / 100],
            timings.last().unwrap(),
            128e6 / rate as f64
        );
    }
    Ok(())
}

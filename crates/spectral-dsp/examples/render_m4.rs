//! Reproducible chord audition fixture and configurable polyphony benchmark.
//! Event dispatch is timed with processing; setup, input generation and file
//! writes are excluded. These measurements are not a real-time host test.
use spectral_dsp::{
    DecayCurve, MAX_PARTIALS, MAX_UNISON, MAX_VOICES, ModulationControls, ModulationMode,
    ResonatorControls, SpectralResonator, UnisonMode,
};
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
    for sample in audio.iter().flatten() {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}

fn events(engine: &mut SpectralResonator, sample: usize, mode: usize, voices: usize, root: i32) {
    if mode == 2 {
        // Much faster than musical playing: overlapping notes and replacements
        // arriving while all slots are fading, plus short pending note-offs.
        if sample.is_multiple_of(64) {
            let token = sample as u64 / 64 + 1;
            engine.note_on(root + (token % 12) as i32, token, 1.0);
            if token > 12 {
                engine.note_off(token - 12);
            }
        }
    } else if sample == 0 || (mode == 1 && sample.is_multiple_of(12_000)) {
        let chord = sample as u64 / 12_000;
        // Held 8/16-voice comparisons use the same low-pitch distribution;
        // repeated keys retain independent note identities. This avoids a
        // higher chord clipping more upper harmonics and hiding its CPU cost.
        let pitch_classes = if mode == 1 { 12 } else { 8 };
        for voice in 0..voices as u64 {
            if chord > 0 {
                engine.note_off((chord - 1) * voices as u64 + voice + 1);
            }
            engine.note_on(
                root + ((chord + voice) % pitch_classes) as i32,
                chord * voices as u64 + voice + 1,
                1.0,
            );
        }
    }
}

fn main() -> std::io::Result<()> {
    // Optional arguments: destination, harmonic limit, sample rate, effect,
    // unison count, decay mode, block size, held/chord note count, unison mode,
    // benchmark root MIDI note. Keep the
    // original 64/48k configuration reproducible alongside expanded-load runs.
    let mut args = std::env::args().skip(1);
    let destination = args.next().unwrap_or_else(|| "target/fixtures/m4".into());
    let harmonics = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(64);
    let rate = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(48_000);
    let effect = args.next().unwrap_or_else(|| "off".into());
    let mode = match effect.as_str() {
        "off" => ModulationMode::Off,
        "chorus" => ModulationMode::Chorus,
        "wander" => ModulationMode::Wander,
        "granular" => ModulationMode::Granular,
        _ => return Err(std::io::Error::other("unknown effect")),
    };
    let unison = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(1);
    let decay_mode = args.next().unwrap_or_else(|| "damping".into());
    let decay_curve = match decay_mode.as_str() {
        "damping" => None,
        "curve" => {
            let mut curve = DecayCurve::default();
            for (point, seconds) in curve.points.iter_mut().zip([3.0, 2.5, 2.0, 1.0, 0.4, 0.2]) {
                point.seconds = seconds;
            }
            Some(curve)
        }
        _ => return Err(std::io::Error::other("unknown decay mode")),
    };
    let block_size = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(128);
    if !(1..=8192).contains(&block_size) {
        return Err(std::io::Error::other("block size out of range"));
    }
    let voices = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(8);
    if !(1..=MAX_VOICES).contains(&voices) {
        return Err(std::io::Error::other("voice count out of range"));
    }
    let unison_algorithm = args.next().unwrap_or_else(|| "spectral".into());
    let unison_mode = match unison_algorithm.as_str() {
        "spectral" => UnisonMode::Spectral,
        "post" => UnisonMode::Post,
        _ => return Err(std::io::Error::other("unknown Unison mode")),
    };
    let root = args
        .next()
        .map(|value| value.parse::<i32>())
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or(33);
    if !(33..=82).contains(&root) {
        return Err(std::io::Error::other("benchmark root note out of range"));
    }
    if !(1..=MAX_PARTIALS).contains(&harmonics) || !(1000..=768_000).contains(&rate) {
        return Err(std::io::Error::other(
            "harmonic limit or sample rate out of range",
        ));
    }
    if !(1..=MAX_UNISON).contains(&unison) {
        return Err(std::io::Error::other("unison count out of range"));
    }
    std::fs::create_dir_all(&destination)?;
    let frames = rate * 8;
    let mut input = vec![[0.0; 2]; frames];
    let mut seed = 0x1234_5678_u32;
    for (i, frame) in input.iter_mut().enumerate() {
        for (channel, sample) in frame.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32;
            if i < rate * 5 {
                *sample = noise * (-(((i + channel * 6000) % 12_000) as f32) / 1200.0).exp() * 0.45;
            }
        }
    }
    let controls = ResonatorControls {
        note: None,
        harmonics,
        t60: 2.0,
        decay_curve,
        unison_mode,
        modulation: ModulationControls {
            mode,
            rate_hz: 3.0,
            amount: 0.7,
            pitch_semitones: 0.2,
            unison_voices: unison,
            unison_detune_cents: 12.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let start = Instant::now();
    let mut engine = SpectralResonator::new(2, 4096, 512, rate as f32).unwrap();
    println!(
        "rate={rate}, harmonic_limit={harmonics}, effect={effect}, unison={unison}, unison_mode={unison_algorithm}, root={root}, decay={decay_mode}, notes_per_chord={voices}, pool_capacity={MAX_VOICES}, prepare_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let mut output = input.clone();
    // Two four-note chords overlap their natural tails without exhausting the
    // pool. Both are released before the final silent-input decay section.
    for (i, frame) in output.iter_mut().enumerate() {
        if i == 0 {
            for (token, note) in [57, 60, 64, 67].into_iter().enumerate() {
                engine.note_on(note, token as u64 + 1, 1.0);
            }
        }
        if i == rate * 2 {
            for token in 1..=4 {
                engine.note_off(token);
            }
            for (token, note) in [59, 62, 65, 69].into_iter().enumerate() {
                engine.note_on(note, token as u64 + 5, 1.0);
            }
        }
        if i == rate * 4 {
            for token in 5..=8 {
                engine.note_off(token);
            }
        }
        engine.process_poly_frame(frame.iter_mut(), controls);
    }
    let peak = output
        .iter()
        .flatten()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let gain = (0.8 / peak).min(1.0);
    // Audition headroom affects the file only; the plugin has no hidden limiter.
    for sample in output.iter_mut().flatten() {
        *sample *= gain;
    }
    wav(
        &Path::new(&destination).join("input-stereo.wav"),
        &input,
        rate as u32,
    )?;
    wav(
        &Path::new(&destination).join("polyphonic-stereo.wav"),
        &output,
        rate as u32,
    )?;
    println!("fixture_peak_before_gain={peak:.6}, fixture_gain={gain:.6}");

    for (mode, name) in [
        "held-low-notes",
        "chord-replacement",
        "dense-stealing-every-64-samples",
        "held-curve-automation",
    ]
    .into_iter()
    .enumerate()
    {
        engine.reset();
        let mut work = input.clone();
        let mut timings = Vec::with_capacity(frames.div_ceil(block_size));
        let mut peak_active_voices = 0;
        for (block_index, block) in work.chunks_mut(block_size).enumerate() {
            let start = Instant::now();
            for (offset, frame) in block.iter_mut().enumerate() {
                events(
                    &mut engine,
                    block_index * block_size + offset,
                    mode,
                    voices,
                    root,
                );
                let mut current = controls;
                if mode == 3 {
                    // Exercise coefficient cache invalidation throughout a
                    // moving curve, including the cost of each hop's updates.
                    let curve = current.decay_curve.get_or_insert_with(DecayCurve::default);
                    let cycle = (block_index * block_size + offset) as f32 / rate as f32;
                    curve.points[2].seconds = 1.5 + (cycle * 2.0).sin();
                }
                engine.process_poly_frame(frame.iter_mut(), current);
            }
            timings.push(start.elapsed().as_secs_f64() * 1e6);
            // Observe outside the timed callback. Released tails also consume
            // slots, so replacement/dense scenes can exceed notes_per_chord.
            peak_active_voices = peak_active_voices.max(engine.active_voice_count());
        }
        if mode == 0 || mode == 3 {
            assert_eq!(peak_active_voices, voices);
        }
        assert!(work.iter().flatten().all(|sample| sample.is_finite()));
        timings.sort_by(f64::total_cmp);
        let mean = timings.iter().sum::<f64>() / timings.len() as f64;
        println!(
            "mode={name}, peak_active_voices={peak_active_voices}, block={block_size}, blocks={}, mean_us={mean:.3}, p50_us={:.3}, p99_us={:.3}, max_us={:.3}, audio_budget_us={:.3}",
            timings.len(),
            timings[timings.len() / 2],
            timings[timings.len() * 99 / 100],
            timings.last().unwrap(),
            block_size as f64 * 1e6 / rate as f64
        );
    }
    Ok(())
}

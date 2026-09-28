//! Measure actual single-partial excitation curves for neighboring low keys.
//! Diagnostic only: production FFT settings and installed artifacts are untouched.
use spectral_dsp::{ResonatorControls, SpectralResonator};
use std::{fs::File, io::BufWriter, io::Write, path::Path};

const RATE: usize = 48_000;

fn hz(note: i32) -> f64 {
    440.0 * 2.0_f64.powf((note - 69) as f64 / 12.0)
}

fn measure(engine: &mut SpectralResonator, note: i32, input: f64, fast: bool) -> f64 {
    engine.reset();
    let settings = ResonatorControls {
        note: Some(note),
        harmonics: 1,
        t60: 2.0,
        attack_ms: fast.then_some(0.0),
        hf_damp: 0.0,
        lf_damp: 0.0,
        low_hz: 1.0,
        high_hz: 12_000.0,
        wet_level: 1.0,
        mid_mix: 1.0,
        align_wet: 0.0,
        ..Default::default()
    };
    let (step_sin, step_cos) = (std::f64::consts::TAU * input / RATE as f64).sin_cos();
    let (mut sine, mut cosine) = (0.0, 1.0);
    let mut energy = 0.0;
    // As in evaluate_windows: drive for two seconds and measure the last 0.5 s.
    // Use a double-precision oscillator to avoid calling sin for every sample.
    for i in 0..RATE * 2 {
        let mut frame = (0.1 * sine) as f32;
        (sine, cosine) = (
            sine * step_cos + cosine * step_sin,
            cosine * step_cos - sine * step_sin,
        );
        engine.process_frame([&mut frame], settings);
        let wet = engine.output_parts().1[0];
        assert!(wet.is_finite());
        if i >= RATE * 3 / 2 {
            energy += f64::from(wet).powi(2);
        }
    }
    (energy / (RATE / 2) as f64).sqrt()
}

fn main() -> std::io::Result<()> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/low-key-overlap".into());
    let root = Path::new(&output);
    std::fs::create_dir_all(root)?;
    let mut csv = BufWriter::new(File::create(root.join("sweep.csv"))?);
    writeln!(csv, "size,hop,mode,note,target_hz,input_hz,wet_rms")?;
    let two_octaves = std::env::args().any(|arg| arg == "--two-octaves");
    let last_note = if two_octaves { 72 } else { 54 };
    let last_hz = if two_octaves { 560 } else { 220 };
    let step = if two_octaves { 2 } else { 1 };
    // Include exact equal-tempered key centers as well as the uniform grid.
    // These exact samples support the neighbor-response bars without interpolation.
    let mut inputs: Vec<_> = (100..=last_hz).step_by(step).map(f64::from).collect();
    inputs.extend((48..=last_note).map(hz));
    inputs.sort_by(f64::total_cmp);
    inputs.dedup();
    for size in [2048, 4096] {
        let mut engine = SpectralResonator::new(1, size, 512, RATE as f32).unwrap();
        for note in 48..=last_note {
            for &input in &inputs {
                let rms = measure(&mut engine, note, input, true);
                writeln!(csv, "{size},512,fast,{note},{},{input},{rms}", hz(note))?;
            }
            csv.flush()?;
            println!("N={size} H=512 fast note={note} complete");
        }
    }
    // One high-resolution Natural curve documents the difference from the fast
    // excitation mode instead of suggesting all attack/decay settings are broad.
    let mut engine = SpectralResonator::new(1, 2048, 512, RATE as f32).unwrap();
    let mut inputs: Vec<_> = (-100..=100).map(|i| hz(50) + f64::from(i) * 0.25).collect();
    inputs.extend((48..=54).map(hz));
    inputs.sort_by(f64::total_cmp);
    inputs.dedup();
    for input in inputs {
        let rms = measure(&mut engine, 50, input, false);
        writeln!(csv, "2048,512,natural,50,{},{input},{rms}", hz(50))?;
    }
    csv.flush()?;
    println!("N=2048 H=512 Natural D3 complete");
    Ok(())
}

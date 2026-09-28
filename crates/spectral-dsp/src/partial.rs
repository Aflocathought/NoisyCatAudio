use rustfft::num_complex::{Complex32, Complex64};

pub const MAX_PARTIALS: usize = 512;
pub const DEFAULT_PARTIALS: usize = 256;
const KERNEL_RADIUS: i32 = 32;

pub(crate) struct Tap {
    pub bin: usize,
    /// DFT of a periodic Hann-windowed complex sinusoid, divided by N.
    pub weight: Complex32,
}

pub(crate) struct Partial {
    pub taps: Vec<Tap>,
    pub rotation: Complex32,
    pub level: f32,
}

pub(crate) struct NoteTemplate {
    pub partials: Vec<Partial>,
}

pub(crate) fn frequency(note: i32) -> f64 {
    440.0 * 2.0_f64.powf((note - 69) as f64 / 12.0)
}

// Closed-form finite geometric sum, including its removable singularity.
// Reducing the argument also keeps the negative-frequency wrap well behaved.
fn rectangular_dtft(bin: f64, size: usize) -> Complex64 {
    let n = size as f64;
    let bin = (bin + n * 0.5).rem_euclid(n) - n * 0.5;
    if bin.abs() < 1e-10 {
        return Complex64::new(n, 0.0);
    }
    let angle = std::f64::consts::PI * bin;
    Complex64::from_polar(angle.sin() / (angle / n).sin(), -angle * (n - 1.0) / n)
}

pub(crate) fn hann_dtft(bin: f64, size: usize) -> Complex64 {
    rectangular_dtft(bin, size) * 0.5
        - (rectangular_dtft(bin - 1.0, size) + rectangular_dtft(bin + 1.0, size)) * 0.25
}

impl Partial {
    pub fn new(hz: f64, rate: f64, size: usize, hop: usize, harmonic: usize) -> Self {
        let center = hz * size as f64 / rate;
        let nearest = center.round() as i32;
        let taps = (-KERNEL_RADIUS..=KERNEL_RADIUS)
            .map(|offset| {
                let bin = nearest + offset;
                let weight = hann_dtft(bin as f64 - center, size) / size as f64;
                Tap {
                    bin: bin.rem_euclid(size as i32) as usize,
                    weight: Complex32::new(weight.re as f32, weight.im as f32),
                }
            })
            .collect();
        let phase =
            (std::f64::consts::TAU * hz * hop as f64 / rate).rem_euclid(std::f64::consts::TAU);
        // Fade the last partials out before Nyquist; never fold them back.
        let nyquist_fade = ((rate * 0.49 - hz) / (rate * 0.04)).clamp(0.0, 1.0);
        Self {
            taps,
            rotation: Complex32::new(phase.cos() as f32, phase.sin() as f32),
            level: (nyquist_fade / (harmonic as f64).sqrt()) as f32,
        }
    }

    pub fn project(&self, input: &[Complex32]) -> Complex32 {
        // Parseval gives the exact fractional-frequency Hann projection from
        // the unwindowed FFT. Complex weights preserve phase, unlike magnitude
        // interpolation. The finite support error is tested independently.
        let sum = self.taps.iter().fold(Complex32::new(0.0, 0.0), |sum, tap| {
            sum + input[tap.bin] * tap.weight.conj()
        });
        sum * (4.0 / input.len() as f32)
    }

    pub fn synthesize_positive(&self, state: Complex32, spectrum: &mut [Complex32]) {
        let amplitude = state * (spectrum.len() as f32 * 0.5);
        for tap in &self.taps {
            spectrum[tap.bin] += amplitude * tap.weight;
        }
    }

    #[cfg(test)]
    pub fn synthesize(&self, state: Complex32, spectrum: &mut [Complex32]) {
        let amplitude = state * (spectrum.len() as f32 * 0.5);
        for tap in &self.taps {
            let value = amplitude * tap.weight;
            spectrum[tap.bin] += value;
            // Include both lobes even if they overlap around DC or Nyquist.
            let mirror = if tap.bin == 0 {
                0
            } else {
                spectrum.len() - tap.bin
            };
            spectrum[mirror] += value.conj();
        }
    }
}

/// Linearity permits adding every conjugate mirror once after all partials:
/// X[k] = P[k] + conj(P[-k]). Pairwise updates retain both original values,
/// including overlapping lobes near DC/Nyquist, with no extra buffer or loss
/// of kernel taps. Floating-point summation order is the only difference.
pub(crate) fn complete_conjugate_spectrum(spectrum: &mut [Complex32]) {
    let half = spectrum.len() / 2;
    let (lower, upper) = spectrum.split_at_mut(half);
    lower[0] = Complex32::new(2.0 * lower[0].re, 0.0);
    upper[0] = Complex32::new(2.0 * upper[0].re, 0.0);
    for (positive, negative) in lower[1..].iter_mut().zip(upper[1..].iter_mut().rev()) {
        let value = *positive + negative.conj();
        *positive = value;
        *negative = value.conj();
    }
}

impl NoteTemplate {
    pub fn new(note: i32, rate: f64, size: usize, hop: usize) -> Self {
        let fundamental = frequency(note);
        // The parameter is an upper bound, not permission to alias harmonics.
        // Only prepare partials below the existing pre-Nyquist safety cutoff.
        let partials = (1..=MAX_PARTIALS)
            .take_while(|&h| fundamental * (h as f64) < rate * 0.49)
            .map(|h| Partial::new(fundamental * h as f64, rate, size, hop, h))
            .collect();
        Self { partials }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::FftPlanner;

    #[test]
    fn expanded_templates_respect_nyquist_at_low_and_high_notes() {
        for rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for note in [33, 69, 93] {
                let template = NoteTemplate::new(note, rate, 4096, 512);
                let count = template.partials.len();
                assert!(count <= MAX_PARTIALS);
                assert!(frequency(note) * (count as f64) < rate * 0.49);
                assert!(
                    count == MAX_PARTIALS || frequency(note) * (count + 1) as f64 >= rate * 0.49
                );
                assert!(
                    template
                        .partials
                        .iter()
                        .all(|p| p.level > 0.0 && p.rotation.norm().is_finite())
                );
                if note == 33 {
                    assert_eq!(
                        count,
                        match rate as u32 {
                            44_100 => 392,
                            48_000 => 427,
                            _ => 512,
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn full_spectrum_matches_f64_direct_synthesis() {
        let n = 512;
        let inverse = FftPlanner::<f64>::new().plan_fft_inverse(n);
        for center in [0.3, 2.51, 17.17, 127.9, 254.3] {
            for phase in [0.0, 0.7, 2.3] {
                let state = Complex64::from_polar(0.7, phase);
                let mut bins: Vec<_> = (0..n)
                    .map(|k| {
                        (state * hann_dtft(k as f64 - center, n)
                            + state.conj() * hann_dtft(k as f64 + center, n))
                            * 0.5
                    })
                    .collect();
                inverse.process(&mut bins);
                let mut error = 0.0;
                let mut energy = 0.0;
                for (t, value) in bins.iter().enumerate() {
                    let theta = std::f64::consts::TAU * t as f64 / n as f64;
                    let expected = 0.5 * (1.0 - theta.cos()) * 0.7 * (theta * center + phase).cos();
                    error += (value.re / n as f64 - expected).powi(2);
                    energy += expected.powi(2);
                }
                assert!((error / energy).sqrt() < 1e-10);
            }
        }
    }

    #[test]
    fn sparse_synthesis_and_projection_match_direct_references() {
        let n = 4096;
        let mut planner = FftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(n);
        let inverse = planner.plan_fft_inverse(n);
        let mut worst_synthesis = 0.0_f64;
        let mut worst_projection = 0.0_f64;
        for rate in [44_100.0, 48_000.0, 96_000.0] {
            for base in [2.0, 17.0, 151.0, 1_950.0] {
                for offset in 0..=10 {
                    let center = base + offset as f64 / 10.0;
                    let partial = Partial::new(center * rate / n as f64, rate, n, 512, 1);
                    for phase in [0.0_f64, 0.7, 2.3] {
                        let mut spectrum = vec![Complex32::new(0.0, 0.0); n];
                        partial.synthesize(Complex32::from_polar(0.7, phase as f32), &mut spectrum);
                        inverse.process(&mut spectrum);
                        let mut error = 0.0;
                        let mut energy = 0.0;
                        let mut input = Vec::with_capacity(n);
                        let mut reference = Complex64::new(0.0, 0.0);
                        for (t, value) in spectrum.iter().enumerate() {
                            let theta = std::f64::consts::TAU * t as f64 / n as f64;
                            let window = 0.5 * (1.0 - theta.cos());
                            let sample =
                                0.7 * (theta * center + phase).cos() + 0.2 * (theta * 97.3).sin();
                            input.push(Complex32::new(sample as f32, 0.0));
                            reference += Complex64::from_polar(sample * window, -theta * center);
                            let expected = window * 0.7 * (theta * center + phase).cos();
                            error += (value.re as f64 / n as f64 - expected).powi(2);
                            energy += expected.powi(2);
                        }
                        worst_synthesis = worst_synthesis.max((error / energy).sqrt());
                        forward.process(&mut input);
                        let actual = partial.project(&input);
                        reference *= 4.0 / n as f64;
                        let error =
                            (Complex64::new(actual.re as f64, actual.im as f64) - reference).norm();
                        worst_projection = worst_projection.max(error);
                    }
                }
            }
        }
        eprintln!(
            "sparse synthesis relative RMS={worst_synthesis:e}, projection absolute error={worst_projection:e}"
        );
        assert!(worst_synthesis < 1e-3);
        assert!(worst_projection < 1e-4);
    }
}

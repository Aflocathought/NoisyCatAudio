use rustfft::num_complex::Complex32;

/// M2 research oscillator bank. Each channel owns its own complex history;
/// only the note and decay controls are shared across channels.
pub struct BinResonator {
    sample_rate: f32,
    fft_size: usize,
    hop_size: usize,
    state: Vec<Complex32>,
    rotation: Vec<Complex32>,
    excitation: Vec<f32>,
    radius: f32,
    note: i32,
    t60: f32,
}

impl BinResonator {
    pub fn new(sample_rate: f32, fft_size: usize, hop_size: usize) -> Option<Self> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.0
            || fft_size < 2
            || !fft_size.is_multiple_of(2)
            || hop_size == 0
            || hop_size > fft_size
        {
            return None;
        }

        let positive_bins = fft_size / 2 + 1;
        // Frame-local FFT phases advance by k * H/N turns between two hops.
        let rotation = (0..positive_bins)
            .map(|bin| {
                let phase = std::f32::consts::TAU * bin as f32 * hop_size as f32 / fft_size as f32;
                Complex32::from_polar(1.0, phase)
            })
            .collect();
        let mut resonator = Self {
            sample_rate,
            fft_size,
            hop_size,
            state: vec![Complex32::new(0.0, 0.0); positive_bins],
            rotation,
            excitation: vec![0.0; positive_bins],
            radius: 0.0,
            note: -1,
            t60: 0.0,
        };
        resonator.set_controls(57, 2.0);
        Some(resonator)
    }

    pub fn reset(&mut self) {
        self.state.fill(Complex32::new(0.0, 0.0));
    }

    pub fn set_controls(&mut self, note: i32, t60: f32) {
        let note = note.clamp(33, 93);
        let t60 = if t60.is_finite() {
            t60.clamp(0.05, 12.0)
        } else {
            2.0
        };
        if note != self.note {
            self.note = note;
            self.excitation.fill(0.0);
            let fundamental = 440.0 * 2.0_f32.powf((note - 69) as f32 / 12.0);
            for harmonic in 1..=32 {
                let frequency = fundamental * harmonic as f32;
                if frequency >= self.sample_rate * 0.45 {
                    break;
                }
                let bin = (frequency * self.fft_size as f32 / self.sample_rate).round() as usize;
                if bin > 0 && bin < self.fft_size / 2 {
                    // M2 deliberately snaps each harmonic to one FFT bin. The
                    // inverse-square-root weight limits bright, dense chords.
                    self.excitation[bin] += 1.0 / (harmonic as f32).sqrt();
                }
            }
        }
        if t60 != self.t60 {
            self.t60 = t60;
            // One T60 of hop-wise feedback must reduce amplitude by 60 dB.
            self.radius = 10.0_f32.powf(-3.0 * self.hop_size as f32 / (self.sample_rate * t60));
        }
    }

    pub fn process_spectrum(&mut self, spectrum: &mut [Complex32]) {
        if spectrum.len() != self.fft_size {
            spectrum.fill(Complex32::new(0.0, 0.0));
            return;
        }
        let half = self.fft_size / 2;
        spectrum[0] = Complex32::new(0.0, 0.0);
        spectrum[half] = Complex32::new(0.0, 0.0);
        for bin in 1..half {
            let input = spectrum[bin];
            let injection = self.excitation[bin] * (1.0 - self.radius);
            let next = self.state[bin] * self.rotation[bin] * self.radius + input * injection;
            // A corrupt host sample must not poison a resonator indefinitely.
            self.state[bin] =
                if next.re.is_finite() && next.im.is_finite() && next.norm_sqr() > 1e-40 {
                    next
                } else {
                    Complex32::new(0.0, 0.0)
                };
            spectrum[bin] = self.state[bin];
            // The negative bin mirrors the positive one for a real IFFT.
            spectrum[self.fft_size - bin] = self.state[bin].conj();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_bin_state_rotates_and_reaches_minus_sixty_db_at_t60() {
        let sample_rate = 48_000.0;
        let fft_size = 4096;
        let hop_size = 512;
        let mut resonator = BinResonator::new(sample_rate, fft_size, hop_size).unwrap();
        resonator.set_controls(69, 1.0);
        let bin = (440.0_f32 * fft_size as f32 / sample_rate).round() as usize;
        let mut spectrum = vec![Complex32::new(0.0, 0.0); fft_size];
        spectrum[bin] = Complex32::new(100.0, 0.0);
        resonator.process_spectrum(&mut spectrum);
        let initial = resonator.state[bin];
        assert!(initial.norm() > 0.0);

        spectrum.fill(Complex32::new(0.0, 0.0));
        resonator.process_spectrum(&mut spectrum);
        let expected = initial * resonator.rotation[bin] * resonator.radius;
        assert!((resonator.state[bin] - expected).norm() <= 1e-5);
        assert!((spectrum[fft_size - bin] - spectrum[bin].conj()).norm() <= 1e-6);

        for _ in 1..(sample_rate as usize / hop_size) {
            spectrum.fill(Complex32::new(0.0, 0.0));
            resonator.process_spectrum(&mut spectrum);
        }
        let ratio = resonator.state[bin].norm() / initial.norm();
        assert!((ratio - 0.001).abs() <= 0.000_08, "ratio={ratio}");
    }

    #[test]
    fn changing_note_keeps_previous_tail_but_reset_clears_it() {
        let mut resonator = BinResonator::new(44_100.0, 4096, 512).unwrap();
        let old_bin = (220.0_f32 * 4096.0 / 44_100.0).round() as usize;
        let mut spectrum = vec![Complex32::new(0.0, 0.0); 4096];
        spectrum[old_bin] = Complex32::new(20.0, 0.0);
        resonator.process_spectrum(&mut spectrum);
        let old_tail = resonator.state[old_bin].norm();
        resonator.set_controls(69, 2.0);
        spectrum.fill(Complex32::new(0.0, 0.0));
        resonator.process_spectrum(&mut spectrum);
        assert!(resonator.state[old_bin].norm() > 0.0);
        assert!(resonator.state[old_bin].norm() < old_tail);
        resonator.reset();
        assert!(resonator.state.iter().all(|value| value.norm_sqr() == 0.0));
    }
}

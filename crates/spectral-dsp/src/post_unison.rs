//! Small-detune audio Unison. One stereo delay line follows the summed wet
//! signal; its cost is independent of MIDI voice and partial counts.
use crate::MAX_UNISON;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnisonMode {
    #[default]
    Spectral,
    Post,
}

const TAPS: usize = 16;
const FRACTIONS: usize = 256;
const WINDOW_SIZE: usize = 1024;
const MIN_DELAY: f64 = TAPS as f64;
const SPAN_SECONDS: f64 = 0.04;

pub(crate) struct PostUnison {
    ring: Box<[[f32; 2]]>,
    kernel: Box<[[f32; TAPS]]>,
    window: Box<[f32]>,
    write: usize,
    span: f64,
    phase: [f64; MAX_UNISON],
    step: [f64; MAX_UNISON],
    target_step: [f64; MAX_UNISON],
    gains: [f32; MAX_UNISON],
    count: usize,
    blend: f32,
    enabled: bool,
    fade_step: f32,
    smoothing: f64,
    controls: Option<(UnisonMode, usize, f32)>,
}

impl PostUnison {
    pub fn new(rate: f32) -> Self {
        let span = (rate as f64 * SPAN_SECONDS).max(16.0);
        let ring_size = (span.ceil() as usize + TAPS * 2 + 2).next_power_of_two();
        // A normalized windowed-sinc interpolation table retains high-frequency
        // detail better than linear interpolation. Construct it off the audio
        // thread; the engine also fades the post input before Nyquist.
        let kernel = (0..=FRACTIONS)
            .map(|row| {
                let fraction = row as f64 / FRACTIONS as f64;
                let mut coefficients = std::array::from_fn(|tap| {
                    let x = tap as f64 - (TAPS / 2 - 1) as f64 - fraction;
                    let sinc = if x.abs() < 1e-12 {
                        0.94
                    } else {
                        (std::f64::consts::PI * 0.94 * x).sin() / (std::f64::consts::PI * x)
                    };
                    let window = if x.abs() >= TAPS as f64 / 2.0 {
                        0.0
                    } else {
                        0.42 + 0.5 * (std::f64::consts::TAU * x / TAPS as f64).cos()
                            + 0.08 * (2.0 * std::f64::consts::TAU * x / TAPS as f64).cos()
                    };
                    (sinc * window) as f32
                });
                let sum: f32 = coefficients.iter().sum();
                for coefficient in &mut coefficients {
                    *coefficient /= sum;
                }
                coefficients
            })
            .collect();
        Self {
            ring: vec![[0.0; 2]; ring_size].into_boxed_slice(),
            kernel,
            window: (0..=WINDOW_SIZE)
                .map(|i| {
                    (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / WINDOW_SIZE as f64).cos())
                        as f32
                })
                .collect(),
            write: 0,
            span,
            phase: std::array::from_fn(|i| i as f64 / MAX_UNISON as f64),
            step: [0.0; MAX_UNISON],
            target_step: [0.0; MAX_UNISON],
            gains: [0.0; MAX_UNISON],
            count: 1,
            blend: 0.0,
            enabled: false,
            fade_step: 1.0 / (rate * 0.02),
            smoothing: 1.0 - (-1.0 / (rate as f64 * 0.02)).exp(),
            controls: None,
        }
    }

    pub fn reset(&mut self) {
        self.ring.fill([0.0; 2]);
        self.write = 0;
        self.phase = std::array::from_fn(|i| i as f64 / MAX_UNISON as f64);
        self.step.fill(0.0);
        self.target_step.fill(0.0);
        self.gains.fill(0.0);
        self.blend = 0.0;
        self.enabled = false;
        self.controls = None;
    }

    pub fn max_delay_samples(&self) -> u32 {
        (self.span + MIN_DELAY + TAPS as f64).ceil() as u32
    }

    pub fn set_controls(&mut self, mode: UnisonMode, count: usize, cents: f32) {
        self.count = count.clamp(1, MAX_UNISON);
        let cents = if cents.is_finite() {
            cents.clamp(0.0, 50.0)
        } else {
            0.0
        };
        if self.controls == Some((mode, self.count, cents)) {
            return;
        }
        self.controls = Some((mode, self.count, cents));
        self.enabled = mode == UnisonMode::Post && self.count > 1 && cents > 0.0;
        for u in 0..MAX_UNISON {
            let position = if self.count > 1 && u < self.count {
                2.0 * u as f64 / (self.count - 1) as f64 - 1.0
            } else {
                0.0
            };
            let ratio = 2.0_f64.powf(position * cents as f64 / 1200.0);
            // y[n] = x[n-d[n]]: the read speed is 1-delta(d), hence
            // delta(d)=1-ratio. Hann weights hide the delay's wrap reset.
            self.target_step[u] = (1.0 - ratio) / self.span;
        }
    }

    fn read(&self, delay: f64) -> [f32; 2] {
        let position = self.write as f64 - delay;
        let base = position.floor();
        let fraction = (position - base) * FRACTIONS as f64;
        let row = (fraction as usize).min(FRACTIONS - 1);
        let mix = (fraction - row as f64) as f32;
        let first = base as isize - (TAPS / 2 - 1) as isize;
        let mask = self.ring.len() - 1;
        let mut output = [0.0; 2];
        for tap in 0..TAPS {
            let weight =
                self.kernel[row][tap] + mix * (self.kernel[row + 1][tap] - self.kernel[row][tap]);
            let sample = self.ring[(first + tap as isize) as usize & mask];
            output[0] += sample[0] * weight;
            output[1] += sample[1] * weight;
        }
        output
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        // Keep history warm even in Spectral/bypass mode, so a mode change can
        // crossfade immediately. No buffer creation or clearing on that change.
        self.ring[self.write] = input;
        let target = if self.enabled { 1.0 } else { 0.0 };
        self.blend += (target - self.blend).clamp(-self.fade_step, self.fade_step);
        let mut output = input;
        if self.blend > 0.0 {
            let mut shifted = [0.0; 2];
            let mut total_gain = 0.0;
            for u in 0..MAX_UNISON {
                let target = if u < self.count { 1.0 } else { 0.0 };
                self.gains[u] += (target - self.gains[u]).clamp(-self.fade_step, self.fade_step);
                let gain = self.gains[u];
                if gain == 0.0 {
                    continue;
                }
                self.step[u] += self.smoothing * (self.target_step[u] - self.step[u]);
                self.phase[u] += self.step[u];
                if self.phase[u] < 0.0 {
                    self.phase[u] += 1.0;
                }
                if self.phase[u] >= 1.0 {
                    self.phase[u] -= 1.0;
                }
                let phase = self.phase[u];
                let second_phase = if phase < 0.5 {
                    phase + 0.5
                } else {
                    phase - 0.5
                };
                let position = phase * WINDOW_SIZE as f64;
                let index = (position as usize).min(WINDOW_SIZE - 1);
                let fraction = (position - index as f64) as f32;
                let weight =
                    self.window[index] + fraction * (self.window[index + 1] - self.window[index]);
                let first = self.read(MIN_DELAY + phase * self.span);
                let second = self.read(MIN_DELAY + second_phase * self.span);
                for channel in 0..2 {
                    shifted[channel] +=
                        gain * (first[channel] * weight + second[channel] * (1.0 - weight));
                }
                total_gain += gain;
            }
            for channel in 0..2 {
                output[channel] +=
                    self.blend * (shifted[channel] / total_gain.max(1e-12) - input[channel]);
            }
        }
        self.write = (self.write + 1) & (self.ring.len() - 1);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_dc_stereo_isolation_and_reset_are_preserved() {
        let mut effect = PostUnison::new(48_000.0);
        for (mode, count, cents) in [
            (UnisonMode::Spectral, 8, 50.0),
            (UnisonMode::Post, 1, 50.0),
            (UnisonMode::Post, 8, 0.0),
        ] {
            effect.set_controls(mode, count, cents);
            for i in 0..4096 {
                let input = [(i as f32 * 0.13).sin(), 0.0];
                assert_eq!(effect.process(input), input);
            }
        }
        effect.set_controls(UnisonMode::Post, 8, 50.0);
        for i in 0..100_000 {
            let output = effect.process([0.3, 0.0]);
            assert_eq!(output[1], 0.0);
            if i > 4096 {
                assert!((output[0] - 0.3).abs() < 3e-6);
            }
        }
        effect.reset();
        for _ in 0..4096 {
            assert_eq!(effect.process([0.0; 2]), [0.0; 2]);
        }
    }

    #[test]
    fn detuned_tones_follow_playback_ratio_across_rates_and_wraps() {
        for rate in [44_100.0, 48_000.0, 96_000.0] {
            let mut effect = PostUnison::new(rate);
            effect.set_controls(UnisonMode::Post, 2, 50.0);
            let expected = [-50.0, 50.0].map(|cents| 1000.0 * 2.0_f64.powf(cents / 1200.0));
            let mut sums = [rustfft::num_complex::Complex64::default(); 3];
            let mut largest_step = 0.0_f32;
            let mut previous = 0.0;
            // Four seconds span several wraps in both delay directions.
            // Off-grid tones and their sidebands are tested separately below.
            for i in 0..rate as usize * 4 {
                let input = (std::f64::consts::TAU * 1000.0 * i as f64 / rate as f64).sin() as f32;
                let output = effect.process([input, 0.0]);
                assert_eq!(output[1], 0.0);
                assert!(output[0].is_finite());
                if i >= rate as usize {
                    largest_step = largest_step.max((output[0] - previous).abs());
                    for (sum, hz) in sums.iter_mut().zip([expected[0], expected[1], 1000.0]) {
                        *sum += rustfft::num_complex::Complex64::from_polar(
                            output[0] as f64,
                            -std::f64::consts::TAU * hz * i as f64 / rate as f64,
                        );
                    }
                }
                previous = output[0];
            }
            let amplitudes = sums.map(|sum| sum.norm() * 2.0 / (rate as f64 * 3.0));
            eprintln!("post rate={rate}, down/up/carrier={amplitudes:?}, max_step={largest_step}");
            assert!(amplitudes[0] > 0.35 && amplitudes[1] > 0.35);
            assert!(amplitudes[2] < 0.015);
            assert!(largest_step < 0.2);
        }
    }

    #[test]
    fn off_grid_tones_measure_pitch_and_wrap_sidebands() {
        use rustfft::{FftPlanner, num_complex::Complex32};
        let rate = 48_000.0;
        let count = 144_000;
        let size = 262_144;
        let fft = FftPlanner::<f32>::new().plan_fft_forward(size);
        for hz in [150.0, 440.0, 997.0] {
            let mut effect = PostUnison::new(rate);
            effect.set_controls(UnisonMode::Post, 2, 50.0);
            let mut spectrum = vec![Complex32::default(); size];
            for i in 0..count + 48_000 {
                let input = (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin() as f32;
                let output = effect.process([input, 0.0]);
                if i >= 48_000 {
                    let j = i - 48_000;
                    let window =
                        0.5 - 0.5 * (std::f64::consts::TAU * j as f64 / (count - 1) as f64).cos();
                    spectrum[j].re = output[0] * window as f32;
                }
            }
            fft.process(&mut spectrum);
            for cents in [-50.0, 50.0] {
                let expected = hz * 2.0_f64.powf(cents / 1200.0);
                let lo = ((expected - 2.0) * size as f64 / rate as f64) as usize;
                let hi = ((expected + 2.0) * size as f64 / rate as f64) as usize;
                let peak = (lo..=hi)
                    .max_by(|&a, &b| spectrum[a].norm_sqr().total_cmp(&spectrum[b].norm_sqr()))
                    .unwrap();
                let [a, b, c] = [peak - 1, peak, peak + 1]
                    .map(|bin| spectrum[bin].norm().max(1e-20).ln() as f64);
                let offset = 0.5 * (a - c) / (a - 2.0 * b + c);
                let measured = (peak as f64 + offset) * rate as f64 / size as f64;
                let error_cents = 1200.0 * (measured / expected).log2();
                let amplitude = spectrum[peak].norm() * 4.0 / count as f32;
                eprintln!(
                    "post off-grid input={hz}, detune={cents}, measured_hz={measured:.5}, error_cents={error_cents:.4}, peak_amplitude={amplitude:.4}"
                );
                // Finite-grain shifting has nearby sidebands: measure that
                // limitation rather than claiming sample-perfect resampling.
                assert!(error_cents.abs() < 12.0);
                assert!(amplitude > 0.15);
            }
        }
    }
}

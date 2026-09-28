//! Spectral wet-voice modulation. Input excitation stays shared, while each
//! unison copy has its own continuous phase and per-partial modulation source.
use crate::partial::{MAX_PARTIALS, hann_dtft};
use rustfft::num_complex::Complex32;

pub const MAX_UNISON: usize = 8;
const RADIUS: i32 = 32;
const TAPS: usize = 65;
const FRACTIONS: usize = 256;
const TAU: f64 = std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModulationMode {
    #[default]
    Off,
    Chorus,
    Wander,
    Granular,
}

#[derive(Clone, Copy, Debug)]
pub struct ModulationControls {
    pub mode: ModulationMode,
    pub rate_hz: f32,
    pub amount: f32,
    pub pitch_semitones: f32,
    pub grain_ms: f32,
    pub unison_voices: usize,
    pub unison_detune_cents: f32,
}

impl Default for ModulationControls {
    fn default() -> Self {
        Self {
            mode: ModulationMode::Off,
            rate_hz: 0.5,
            amount: 0.5,
            pitch_semitones: 0.1,
            grain_ms: 80.0,
            unison_voices: 1,
            unison_detune_cents: 7.0,
        }
    }
}

fn bound(value: f32, fallback: f32, low: f32, high: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

impl ModulationControls {
    pub(crate) fn sanitized(self) -> Self {
        Self {
            mode: self.mode,
            rate_hz: bound(self.rate_hz, 0.5, 0.0, 10.0),
            amount: bound(self.amount, 0.0, 0.0, 1.0),
            pitch_semitones: bound(self.pitch_semitones, 0.0, 0.0, 12.0),
            grain_ms: bound(self.grain_ms, 80.0, 5.0, 500.0),
            unison_voices: self.unison_voices.clamp(1, MAX_UNISON),
            unison_detune_cents: bound(self.unison_detune_cents, 0.0, 0.0, 50.0),
        }
    }

    fn enabled(self) -> bool {
        (self.mode != ModulationMode::Off && self.amount > 0.0)
            || (self.unison_voices > 1 && self.unison_detune_cents > 0.0)
    }
}

#[derive(Clone, Copy, Default)]
struct KernelPosition {
    bin: i32,
    row: usize,
    fraction: f32,
}

/// One lookup table per engine replaces per-note/per-hop trigonometric kernel
/// construction. All fractional centers share the same translated Hann shape.
pub(crate) struct ModulationKernel {
    rows: Box<[[Complex32; TAPS]]>,
    size: usize,
    rate: f32,
}

impl ModulationKernel {
    pub fn new(size: usize, rate: f32) -> Self {
        let rows = (0..=FRACTIONS)
            .map(|row| {
                std::array::from_fn(|tap| {
                    let offset = tap as f64 - RADIUS as f64 - row as f64 / FRACTIONS as f64;
                    let weight = hann_dtft(offset, size) / size as f64;
                    Complex32::new(weight.re as f32, weight.im as f32)
                })
            })
            .collect();
        Self { rows, size, rate }
    }

    fn position(&self, hz: f32) -> KernelPosition {
        let center = hz as f64 * self.size as f64 / self.rate as f64;
        let bin = center.floor() as i32;
        let lookup = ((center - bin as f64) * FRACTIONS as f64) as f32;
        let row = (lookup.floor() as usize).min(FRACTIONS - 1);
        KernelPosition {
            bin,
            row,
            fraction: (lookup - row as f32).clamp(0.0, 1.0),
        }
    }

    fn synthesize_positive(
        &self,
        position: KernelPosition,
        state: Complex32,
        spectrum: &mut [Complex32],
    ) {
        let amplitude = state * (self.size as f32 * 0.5);
        let row = &self.rows[position.row];
        let next_row = &self.rows[position.row + 1];
        let start = position.bin - RADIUS;
        if start >= 0 && start as usize + TAPS <= self.size {
            let start = start as usize;
            // Accumulate only the positive-frequency lobe. The engine adds
            // conjugate mirrors once after ALL partials, removing a scattered
            // write per tap and exposing one contiguous, vectorizable loop.
            for (destination, (a, b)) in spectrum[start..start + TAPS]
                .iter_mut()
                .zip(row.iter().zip(next_row))
            {
                let weight = *a + (*b - *a) * position.fraction;
                let value = amplitude * weight;
                *destination += value;
            }
            return;
        }
        // The positive lobe can wrap around DC. Preserve every wrapped tap;
        // the later mirror pass handles overlap at both DC and Nyquist.
        for tap in 0..TAPS {
            let a = row[tap];
            let b = next_row[tap];
            let weight = a + (b - a) * position.fraction;
            let raw_bin = position.bin + tap as i32 - RADIUS;
            // Keep the established fast wrap for powers of two. Mixed-radix
            // windows (3072) need Euclidean modulo, including negative DC taps.
            let bin = if self.size.is_power_of_two() {
                (raw_bin as usize) & (self.size - 1)
            } else {
                raw_bin.rem_euclid(self.size as i32) as usize
            };
            let value = amplitude * weight;
            spectrum[bin] += value;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Lane {
    seed: u32,
    cycle: f64,
    from: f32,
    to: f32,
    grain: f32,
    phase: f64,
    delta_hz: f32,
    gain: f32,
    amplitude: f32,
    position: KernelPosition,
    rotation: Complex32,
    rendered_gain: f32,
}

fn random(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 * (1.0 / 16_777_216.0)
}

impl Lane {
    fn seeded(order: u64, harmonic: usize, lane: usize) -> Self {
        let mut seed = (order as u32).wrapping_mul(0x9e37_79b9)
            ^ (harmonic as u32 + 1).wrapping_mul(0x85eb_ca6b)
            ^ (lane as u32 + 1).wrapping_mul(0xc2b2_ae35);
        seed |= 1;
        let cycle = random(&mut seed) as f64;
        let from = random(&mut seed) * 2.0 - 1.0;
        let to = random(&mut seed) * 2.0 - 1.0;
        Self {
            seed,
            cycle,
            from,
            to,
            amplitude: 1.0,
            ..Default::default()
        }
    }
}

pub(crate) struct ModulationBank {
    lanes: Box<[[Lane; MAX_UNISON]]>,
    order: Option<u64>,
    blend: f32,
}

impl ModulationBank {
    pub fn new() -> Self {
        Self {
            lanes: (0..MAX_PARTIALS)
                .map(|_| [Lane::default(); MAX_UNISON])
                .collect(),
            order: None,
            blend: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.order = None;
        self.blend = 0.0;
    }

    pub fn prepare(
        &mut self,
        order: u64,
        f0: f32,
        count: usize,
        controls: ModulationControls,
        kernel: &ModulationKernel,
        hop: usize,
    ) {
        let enabled = controls.enabled();
        if !enabled && self.blend == 0.0 {
            return;
        }
        if self.order != Some(order) {
            for (h, lanes) in self.lanes.iter_mut().enumerate() {
                for (u, lane) in lanes.iter_mut().enumerate() {
                    *lane = Lane::seeded(order, h, u);
                }
            }
            self.order = Some(order);
            self.blend = 0.0;
        }
        let dt = hop as f32 / kernel.rate;
        let step = (dt / 0.02).min(1.0);
        let target = if enabled { 1.0 } else { 0.0 };
        self.blend += (target - self.blend).clamp(-step, step);
        if self.blend == 0.0 {
            return;
        }
        let smoothing = 1.0 - (-dt / 0.02).exp();
        let grain_decay = (-dt * 1000.0 / controls.grain_ms).exp();
        let trigger_probability = 1.0 - (-controls.rate_hz * dt).exp();
        for (h, lanes) in self.lanes.iter_mut().take(count).enumerate() {
            let base_hz = f0 * (h + 1) as f32;
            let mut total_gain = 0.0;
            for (u, lane) in lanes.iter_mut().enumerate() {
                let target_gain = if u < controls.unison_voices { 1.0 } else { 0.0 };
                lane.gain += (target_gain - lane.gain).clamp(-step, step);
                total_gain += lane.gain;
                if lane.gain == 0.0 {
                    lane.rendered_gain = 0.0;
                    continue;
                }
                lane.cycle += controls.rate_hz as f64 * dt as f64;
                while lane.cycle >= 1.0 {
                    lane.cycle -= 1.0;
                    lane.from = lane.to;
                    lane.to = random(&mut lane.seed) * 2.0 - 1.0;
                }
                lane.grain *= grain_decay;
                if controls.mode == ModulationMode::Granular
                    && random(&mut lane.seed) < trigger_probability
                {
                    lane.grain = 1.0;
                }
                let signal = match controls.mode {
                    ModulationMode::Off => 0.0,
                    ModulationMode::Chorus => 1.0 - 4.0 * (lane.cycle as f32 - 0.5).abs(),
                    ModulationMode::Wander => lane.from + (lane.to - lane.from) * lane.cycle as f32,
                    ModulationMode::Granular => lane.grain,
                };
                let amplitude = match controls.mode {
                    ModulationMode::Off => 1.0,
                    ModulationMode::Granular => {
                        1.0 - controls.amount + controls.amount * lane.grain
                    }
                    _ => 1.0 - controls.amount * 0.5 * (signal + 1.0),
                };
                lane.amplitude += smoothing * (amplitude - lane.amplitude);
                let spread = if controls.unison_voices > 1 && u < controls.unison_voices {
                    u as f32 * 2.0 / (controls.unison_voices - 1) as f32 - 1.0
                } else {
                    0.0
                };
                let pitch_mod =
                    if controls.mode == ModulationMode::Chorus && controls.rate_hz == 0.0 {
                        0.0
                    } else {
                        signal * controls.pitch_semitones * controls.amount
                    };
                let semitones = spread * controls.unison_detune_cents * 0.01 + pitch_mod;
                let target_delta = base_hz * (2.0_f32.powf(semitones / 12.0) - 1.0);
                let previous_delta = lane.delta_hz;
                lane.delta_hz += smoothing * (target_delta - lane.delta_hz);
                // Integrate at the window center, then translate back to the
                // frame's phase origin. Updating only the rotation while using
                // the old frequency kernel would introduce hop-rate artifacts.
                lane.phase = (lane.phase
                    + TAU * 0.5 * (previous_delta + lane.delta_hz) as f64 * dt as f64)
                    .rem_euclid(TAU);
                let angle = lane.phase
                    - TAU * lane.delta_hz as f64 * kernel.size as f64 / (2.0 * kernel.rate as f64);
                lane.rotation = Complex32::new(angle.cos() as f32, angle.sin() as f32);
                let hz = base_hz + lane.delta_hz;
                lane.position = kernel.position(hz);
                let original_fade =
                    ((kernel.rate * 0.49 - base_hz) / (kernel.rate * 0.04)).clamp(0.0, 1.0);
                let shifted_fade =
                    ((kernel.rate * 0.49 - hz) / (kernel.rate * 0.04)).clamp(0.0, 1.0);
                // Upward modulation must fade out before Nyquist, not wrap.
                let protection = (shifted_fade / original_fade.max(1e-6)).min(1.0);
                lane.rendered_gain = lane.gain * lane.amplitude * protection;
            }
            for lane in lanes {
                lane.rendered_gain /= total_gain.max(1.0);
            }
        }
    }

    pub fn blend(&self) -> f32 {
        self.blend
    }

    pub fn synthesize(
        &self,
        h: usize,
        state: Complex32,
        kernel: &ModulationKernel,
        spectrum: &mut [Complex32],
    ) {
        for lane in &self.lanes[h] {
            if lane.rendered_gain > 0.0 {
                kernel.synthesize_positive(
                    lane.position,
                    state * lane.rotation * (self.blend * lane.rendered_gain),
                    spectrum,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partial::{Partial, complete_conjugate_spectrum};

    #[test]
    fn eight_unison_lanes_contribute_normalized_audio_and_fade_when_reduced() {
        let kernel = ModulationKernel::new(4096, 48_000.0);
        let mut bank = ModulationBank::new();
        let mut controls = ModulationControls {
            unison_voices: 8,
            unison_detune_cents: 50.0,
            ..Default::default()
        }
        .sanitized();
        for _ in 0..100 {
            bank.prepare(1, 880.0, 1, controls, &kernel, 512);
        }
        let mut actual = vec![Complex32::default(); 4096];
        let mut expected = actual.clone();
        let state = Complex32::new(0.8, -0.3);
        bank.synthesize(0, state, &kernel, &mut actual);
        assert_eq!(bank.lanes[0].len(), 8);
        for (u, lane) in bank.lanes[0].iter().enumerate() {
            let hz = 880.0 * 2.0_f32.powf((u as f32 * 2.0 / 7.0 - 1.0) * 50.0 / 1200.0);
            assert!((lane.delta_hz - (hz - 880.0)).abs() < 1e-3);
            assert_eq!(lane.rendered_gain, 0.125);
            // Independently sum every detuned copy. This catches synthesis
            // loops that still stop at four despite an expanded UI range.
            kernel.synthesize_positive(
                kernel.position(hz),
                state * lane.rotation / 8.0,
                &mut expected,
            );
        }
        let error: f32 = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (*a - *b).norm_sqr())
            .sum();
        let energy: f32 = expected.iter().map(|value| value.norm_sqr()).sum();
        assert!((error / energy).sqrt() < 1e-4);

        controls.unison_voices = 2;
        bank.prepare(1, 880.0, 1, controls, &kernel, 512);
        assert!(bank.lanes[0][7].gain > 0.0 && bank.lanes[0][7].gain < 1.0);
        assert!(
            (bank.lanes[0]
                .iter()
                .map(|lane| lane.rendered_gain)
                .sum::<f32>()
                - 1.0)
                .abs()
                < 1e-6
        );
        bank.prepare(1, 880.0, 1, controls, &kernel, 512);
        assert!(
            bank.lanes[0][2..]
                .iter()
                .all(|lane| lane.rendered_gain == 0.0)
        );
        assert_eq!(bank.lanes[0][0].rendered_gain, 0.5);
        eprintln!(
            "lane_bytes={}, sixteen_voice_unison_pool_bytes={}",
            std::mem::size_of::<Lane>(),
            crate::MAX_VOICES * MAX_PARTIALS * MAX_UNISON * std::mem::size_of::<Lane>()
        );
    }

    #[test]
    fn contiguous_synthesis_matches_scalar_with_overlaps_and_accumulated_spectrum() {
        for size in [256, 1024, 2048, 3072, 4096, 16_384] {
            let kernel = ModulationKernel::new(size, 48_000.0);
            let mut actual: Vec<_> = (0..size)
                .map(|i| Complex32::new((i as f32).sin() * 0.1, 0.03))
                .collect();
            let mut expected = actual.clone();
            complete_conjugate_spectrum(&mut expected);
            for index in 0..400 {
                // Cover both fast-path boundaries and many fractional centers,
                // while accumulating overlapping partials into nonzero bins.
                let center = if index < 8 {
                    [
                        0.3,
                        31.9,
                        32.0,
                        33.0,
                        size as f32 / 2.0 - 34.0,
                        size as f32 / 2.0 - 33.0,
                        size as f32 / 2.0 - 32.0,
                        size as f32 / 2.0 - 0.2,
                    ][index]
                } else {
                    (index * 997 % (size * 50)) as f32 * 0.01
                };
                let position = kernel.position(center * 48_000.0 / size as f32);
                let state = Complex32::new((index as f32).cos(), (index as f32 * 0.3).sin());
                kernel.synthesize_positive(position, state, &mut actual);
                let amplitude = state * (size as f32 * 0.5);
                for tap in 0..TAPS {
                    let a = kernel.rows[position.row][tap];
                    let b = kernel.rows[position.row + 1][tap];
                    let value = amplitude * (a + (b - a) * position.fraction);
                    let bin = (position.bin + tap as i32 - RADIUS).rem_euclid(size as i32) as usize;
                    expected[bin] += value;
                    expected[(size - bin) % size] += value.conj();
                }
            }
            complete_conjugate_spectrum(&mut actual);
            let energy: f64 = expected.iter().map(|x| x.norm_sqr() as f64).sum();
            let error: f64 = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| (*a - *b).norm_sqr() as f64)
                .sum();
            assert!((error / energy).sqrt() < 1e-6, "size={size}");
        }
    }

    #[test]
    fn interpolated_kernel_matches_direct_fractional_hann_synthesis() {
        let kernel = ModulationKernel::new(4096, 48_000.0);
        let mut worst = 0.0_f64;
        for center in [2.3, 17.123, 93.9999, 500.3, 1950.73] {
            let hz = center * 48_000.0 / 4096.0;
            let direct = Partial::new(hz, 48_000.0, 4096, 512, 1);
            let mut expected = vec![Complex32::default(); 4096];
            let mut actual = expected.clone();
            let state = Complex32::new(0.6, -0.2);
            direct.synthesize(state, &mut expected);
            kernel.synthesize_positive(kernel.position(hz as f32), state, &mut actual);
            complete_conjugate_spectrum(&mut actual);
            let energy: f64 = expected.iter().map(|x| x.norm_sqr() as f64).sum();
            let error: f64 = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| (*a - b).norm_sqr() as f64)
                .sum();
            worst = worst.max((error / energy).sqrt());
        }
        eprintln!("dynamic kernel worst relative spectral RMS={worst:e}");
        assert!(worst < 1e-3);
    }

    #[test]
    fn granular_is_irregular_independent_and_upward_pitch_is_nyquist_protected() {
        let kernel = ModulationKernel::new(4096, 48_000.0);
        let mut bank = ModulationBank::new();
        let controls = ModulationControls {
            mode: ModulationMode::Granular,
            rate_hz: 4.0,
            amount: 1.0,
            pitch_semitones: 12.0,
            unison_voices: 2,
            grain_ms: 100.0,
            ..Default::default()
        };
        let mut triggers = 0;
        let mut previous = 0.0;
        let mut independent = false;
        for _ in 0..1000 {
            bank.prepare(1, 440.0, 2, controls, &kernel, 512);
            let a = bank.lanes[0][0];
            let b = bank.lanes[0][1];
            if a.grain > previous {
                triggers += 1;
            }
            previous = a.grain;
            independent |= a.grain != b.grain;
            assert!((0.0..=1.0).contains(&a.grain));
            assert!(a.rendered_gain.is_finite() && a.rendered_gain >= 0.0);
        }
        assert!(
            (20..70).contains(&triggers),
            "unexpected granular density: {triggers}"
        );
        assert!(independent);
        let controls = ModulationControls {
            mode: ModulationMode::Off,
            unison_voices: 2,
            unison_detune_cents: 50.0,
            ..Default::default()
        };
        for _ in 0..100 {
            bank.prepare(2, 23_000.0, 1, controls, &kernel, 512);
        }
        assert_eq!(bank.lanes[0][1].rendered_gain, 0.0);
        assert!(bank.lanes[0][0].rendered_gain > 0.0);
    }
}

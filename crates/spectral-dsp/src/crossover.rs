/// Shared frequency weights for the three-band routing. The low and high
/// regions use the dry spectrum; the middle region uses the resonator output.
pub struct CrossoverMask {
    sample_rate: f32,
    weights: Vec<f32>,
    last_low_hz: f32,
    last_high_hz: f32,
}

impl CrossoverMask {
    pub fn new(sample_rate: f32, fft_size: usize) -> Option<Self> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.0
            || fft_size < 2
            || !fft_size.is_multiple_of(2)
        {
            return None;
        }
        let mut mask = Self {
            sample_rate,
            weights: vec![0.0; fft_size / 2 + 1],
            last_low_hz: f32::NAN,
            last_high_hz: f32::NAN,
        };
        mask.set_points(250.0, 4_000.0);
        Some(mask)
    }

    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    pub fn set_points(&mut self, low_hz: f32, high_hz: f32) {
        // Normal audio rates keep the documented 80–800 / 1.6–12 kHz ranges.
        // Smaller host rates are folded into the available spectrum safely.
        let upper = self.sample_rate * 0.45;
        let low = if low_hz.is_finite() { low_hz } else { 250.0 }
            .clamp(1.0, 800.0)
            .min(upper / 2.0);
        let high = if high_hz.is_finite() {
            high_hz
        } else {
            4_000.0
        }
        .clamp(1.0, 12_000.0)
        .min(upper)
        .max(low * 2.0);
        if low == self.last_low_hz && high == self.last_high_hz {
            return;
        }
        self.last_low_hz = low;
        self.last_high_hz = high;

        let bin_width = self.sample_rate / ((self.weights.len() - 1) * 2) as f32;
        for (bin, weight) in self.weights.iter_mut().enumerate() {
            let frequency = bin as f32 * bin_width;
            let low_ratio = frequency / low;
            let low_power = low_ratio * low_ratio * low_ratio;
            let low_power = low_power * low_power;
            let high_ratio = frequency / high;
            let high_power = high_ratio * high_ratio * high_ratio;
            let high_power = high_power * high_power;
            // Each flank tends to 36 dB/oct outside its transition region.
            // Applying the same real weight to dry and wet FFT frames keeps
            // their phase reference identical and their weights summing to 1.
            *weight = low_power / (1.0 + low_power) / (1.0 + high_power);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_has_three_regions_and_sixth_order_asymptotes() {
        let mask = CrossoverMask::new(48_000.0, 4_096).unwrap();
        let at = |hz: f32| -> f32 {
            let bin = (hz * 4_096.0 / 48_000.0).round() as usize;
            mask.weights()[bin]
        };
        assert_eq!(mask.weights()[0], 0.0);
        assert!(at(1_000.0) > 0.99);
        assert!(at(250.0) > 0.4 && at(250.0) < 0.6);
        assert!(at(4_000.0) > 0.4 && at(4_000.0) < 0.6);
        assert!(
            mask.weights()
                .iter()
                .all(|&value| (0.0..=1.0).contains(&value))
        );
        let low_slope_db = 20.0 * (mask.weights()[8] / mask.weights()[4]).log10();
        let high_slope_db = 20.0 * (at(8_000.0) / at(16_000.0)).log10();
        assert!((low_slope_db - 36.0).abs() < 4.0, "{low_slope_db}");
        assert!((high_slope_db - 36.0).abs() < 4.0, "{high_slope_db}");
    }
}

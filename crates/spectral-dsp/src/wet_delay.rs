//! Wet-only pre-delay, after Unison. Steady settings select an exact sample
//! offset; changing the offset crossfades fixed taps instead of bending pitch.
pub(crate) struct WetDelay {
    history: Box<[[f32; 2]]>,
    position: usize,
    initialized: bool,
    from: usize,
    to: usize,
    remaining: usize,
    fade_samples: usize,
}

impl WetDelay {
    pub fn new(max_delay: usize, rate: f32) -> Self {
        Self {
            // The extra slot allows writing the current sample before reading
            // either endpoint, including zero and the full-window delay.
            history: vec![[0.0; 2]; max_delay + 1].into_boxed_slice(),
            position: 0,
            initialized: false,
            from: 0,
            to: 0,
            remaining: 0,
            fade_samples: (rate * 0.02).round().max(1.0) as usize,
        }
    }

    pub fn reset(&mut self) {
        self.history.fill([0.0; 2]);
        self.position = 0;
        self.initialized = false;
        self.remaining = 0;
    }

    pub fn process(&mut self, input: [f32; 2], windows: f32) -> [f32; 2] {
        let windows = if windows.is_finite() {
            windows.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let target = (windows * (self.history.len() - 1) as f32).round() as usize;
        self.history[self.position] = input;
        if !self.initialized {
            // Never fade from zero delay on startup/reset: doing so could leak
            // the early wet onset even when the selected delay is a full window.
            self.from = target;
            self.to = target;
            self.initialized = true;
        } else if self.remaining == 0 && target != self.to {
            self.to = target;
            self.remaining = self.fade_samples;
        }

        let read =
            |delay| self.history[(self.position + self.history.len() - delay) % self.history.len()];
        let output = if self.remaining > 0 {
            self.remaining -= 1;
            let next = read(self.to);
            if self.remaining == 0 {
                self.from = self.to;
                next
            } else {
                let old = read(self.from);
                let mix = 1.0 - self.remaining as f32 / self.fade_samples as f32;
                std::array::from_fn(|channel| old[channel] * (1.0 - mix) + next[channel] * mix)
            }
        } else {
            read(self.to)
        };
        // Keep both taps stationary for the whole fade. If automation changed
        // meanwhile, pick up its latest value on the next sample after this
        // fade finishes. No reset, allocation, or moving-readhead pitch sweep.
        self.position = (self.position + 1) % self.history.len();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_offsets_preserve_both_channels_exactly_across_ring_wraps() {
        for windows in [0.0, 0.125, 0.37, 0.5, 1.0] {
            let mut delay = WetDelay::new(4096, 48000.0);
            let offset = (windows * 4096.0_f32).round() as usize;
            let signal = |i: usize| [i as f32 + 1.0, -(i as f32) - 17.0];
            for i in 0_usize..14000 {
                assert_eq!(
                    delay.process(signal(i), windows),
                    i.checked_sub(offset).map_or([0.0; 2], signal)
                );
            }
            delay.reset();
            for _ in 0..8192 {
                assert_eq!(delay.process([0.0; 2], windows), [0.0; 2]);
            }
        }
    }

    #[test]
    fn rapid_changes_finish_fixed_tap_fades_then_reach_latest_delay() {
        let mut delay = WetDelay::new(100, 1000.0);
        let mut previous = 0.0;
        for i in 0_usize..300 {
            let windows = if i < 120 {
                0.0
            } else if i < 200 {
                if i % 2 == 0 { 0.5 } else { 1.0 }
            } else {
                0.37
            };
            let actual = delay.process([i as f32, -(i as f32)], windows);
            // A raw tap switch jumps by up to 100 samples on this ramp. With
            // 20-sample fades, the bound is 1 ramp step + 100/20 fade steps.
            assert!((actual[0] - previous).abs() <= 6.001);
            assert_eq!(actual[1], -actual[0]);
            if i == 139 {
                assert_eq!(
                    actual[0], 89.0,
                    "rapid changes interrupted the first fixed-tap fade"
                );
            }
            if i >= 240 {
                assert_eq!(actual[0], (i - 37) as f32);
            }
            previous = actual[0];
        }
    }
}

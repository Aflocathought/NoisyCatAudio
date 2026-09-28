//! Input-timed shaping of the reconstructed wet signal. The reference is the
//! input delayed by the existing FFT latency, never mixed into the audio path.
#[derive(Clone, Copy, Debug)]
pub struct TransientControls {
    /// Linear rise to 90% in milliseconds; zero uses a 0.25 ms safety ramp.
    pub attack_ms: f32,
    /// Peak wet amplitude multiplier, 1..4. No change to the steady tail gain.
    pub emphasis: f32,
}

impl Default for TransientControls {
    fn default() -> Self {
        Self {
            attack_ms: 10.0,
            emphasis: 2.0,
        }
    }
}

#[derive(Default)]
struct Channel {
    peak: f32,
    refractory: usize,
    quiet: usize,
    armed: bool,
    opening: f32,
    pulse: f32,
    hold: usize,
}

pub(crate) struct TransientShaper {
    channels: [Channel; 2],
    rate: f32,
    detector_release: f32,
    pulse_release: f32,
    controls: TransientControls,
    initialized: bool,
    mix: f32,
}

impl TransientShaper {
    pub fn new(rate: f32) -> Self {
        Self {
            channels: Default::default(),
            rate,
            detector_release: (-1.0 / (0.02 * rate)).exp(),
            pulse_release: 10.0_f32.powf(-3.0 / (0.06 * rate)),
            controls: TransientControls::default(),
            initialized: false,
            mix: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.channels = Default::default();
        self.initialized = false;
        self.mix = 0.0;
    }

    pub fn process(
        &mut self,
        wet: [f32; 2],
        reference: [f32; 2],
        controls: Option<TransientControls>,
    ) -> [f32; 2] {
        let target = if controls.is_some() { 1.0 } else { 0.0 };
        if !self.initialized {
            self.mix = target;
            self.initialized = true;
        } else {
            let step = 1.0 / (0.02 * self.rate);
            self.mix += (target - self.mix).clamp(-step, step);
        }
        if let Some(controls) = controls {
            self.controls = TransientControls {
                attack_ms: if controls.attack_ms.is_finite() {
                    controls.attack_ms.clamp(0.0, 2000.0)
                } else {
                    10.0
                },
                emphasis: if controls.emphasis.is_finite() {
                    controls.emphasis.clamp(1.0, 4.0)
                } else {
                    1.0
                },
            };
        }
        let attack = self.controls.attack_ms.max(0.25) * 0.001 * self.rate;
        let rise = 0.9 / attack;
        std::array::from_fn(|i| {
            let channel = &mut self.channels[i];
            let input = reference[i].abs();
            channel.refractory = channel.refractory.saturating_sub(1);
            // Relative peak increase detects fresh strikes inside a held MIDI
            // note. A refractory interval avoids retriggering every sine cycle.
            let onset = input > 1e-5 && input > channel.peak * 1.8 && channel.refractory == 0;
            channel.peak = input.max(channel.peak * self.detector_release);
            if channel.peak < 1e-20 {
                channel.peak = 0.0;
            }
            if onset {
                channel.hold = (attack / 0.9 + self.rate * 0.012).ceil() as usize;
                channel.refractory = (self.rate * 0.03).ceil() as usize;
            }
            // The opening gate is latched while a tail is sounding. A repeated
            // onset never closes it or resets the pulse, so it cannot chop an
            // existing tail. Re-arm only after both paths have been negligible
            // for 120 ms; this also lets isolated later notes lose their pre-echo.
            if input > 1e-7 {
                channel.armed = true;
            }
            if input <= 1e-7 && wet[i].abs() <= 1e-8 {
                channel.quiet += 1;
                if channel.quiet >= (self.rate * 0.12).ceil() as usize {
                    channel.armed = false;
                    channel.opening = 0.0;
                    channel.quiet = (self.rate * 0.12).ceil() as usize;
                }
            } else {
                channel.quiet = 0;
            }
            if channel.armed {
                channel.opening = (channel.opening + rise).min(1.0);
            }
            if channel.hold > 0 {
                channel.hold -= 1;
                channel.pulse = (channel.pulse + rise).min(1.0);
            } else {
                channel.pulse *= self.pulse_release;
                if channel.pulse < 1e-6 {
                    channel.pulse = 0.0;
                }
            }
            let gain = channel.opening * (1.0 + (self.controls.emphasis - 1.0) * channel.pulse);
            // Exact bypass preserves old modes and avoids changing their audio
            // through floating-point cancellation. The detector keeps running.
            if self.mix == 0.0 {
                wet[i]
            } else {
                wet[i] * (1.0 + self.mix * (gain - 1.0))
            }
        })
    }
}

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex32};

use crate::bin_resonator::BinResonator;

/// Performs a windowed FFT/IFFT round trip on frames supplied by the streaming
/// overlap-add scheduler. The scheduler owns the per-channel audio history.
pub struct TransparentStft {
    analysis_window: Vec<f32>,
    synthesis_scale: Vec<f32>,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

struct ChannelState {
    input: Vec<f32>,
    output: Vec<f32>,
    frame: Vec<f32>,
    resonator: Option<BinResonator>,
}

/// Fixed-latency streaming scheduler with independent input and output rings
/// for every channel. All storage is allocated before the audio callback.
pub struct StreamingStft {
    fft_size: usize,
    hop_size: usize,
    position: usize,
    channels: Vec<ChannelState>,
    transform: TransparentStft,
}

impl StreamingStft {
    pub fn new(channel_count: usize, fft_size: usize, hop_size: usize) -> Option<Self> {
        if !(1..=2).contains(&channel_count) || fft_size > u32::MAX as usize {
            return None;
        }
        let transform = TransparentStft::new(fft_size, hop_size)?;
        let channels = (0..channel_count)
            .map(|_| ChannelState {
                input: vec![0.0; fft_size],
                output: vec![0.0; fft_size],
                frame: vec![0.0; fft_size],
                resonator: None,
            })
            .collect();
        Some(Self {
            fft_size,
            hop_size,
            position: 0,
            channels,
            transform,
        })
    }

    pub fn new_resonator(
        channel_count: usize,
        fft_size: usize,
        hop_size: usize,
        sample_rate: f32,
    ) -> Option<Self> {
        let mut engine = Self::new(channel_count, fft_size, hop_size)?;
        for channel in &mut engine.channels {
            channel.resonator = Some(BinResonator::new(sample_rate, fft_size, hop_size)?);
        }
        Some(engine)
    }

    pub fn latency_samples(&self) -> u32 {
        self.fft_size as u32
    }

    pub fn reset(&mut self) {
        self.position = 0;
        for channel in &mut self.channels {
            channel.input.fill(0.0);
            channel.output.fill(0.0);
            channel.frame.fill(0.0);
            if let Some(resonator) = &mut channel.resonator {
                resonator.reset();
            }
        }
    }

    pub fn process_frame<'a, I>(&mut self, samples: I)
    where
        I: IntoIterator<Item = &'a mut f32>,
        I::IntoIter: ExactSizeIterator,
    {
        self.process_frame_inner(samples, None);
    }

    /// M2 adds the integer-bin wet output to the N-sample delayed dry input.
    /// The level is temporary research control, before M3's crossover routing.
    pub fn process_resonator_frame<'a, I>(
        &mut self,
        samples: I,
        note: i32,
        t60: f32,
        wet_level: f32,
    ) where
        I: IntoIterator<Item = &'a mut f32>,
        I::IntoIter: ExactSizeIterator,
    {
        self.process_frame_inner(samples, Some((note, t60, wet_level)));
    }

    fn process_frame_inner<'a, I>(&mut self, samples: I, controls: Option<(i32, f32, f32)>)
    where
        I: IntoIterator<Item = &'a mut f32>,
        I::IntoIter: ExactSizeIterator,
    {
        let samples = samples.into_iter();
        if samples.len() != self.channels.len() {
            for sample in samples {
                *sample = 0.0;
            }
            return;
        }

        for (channel, sample) in self.channels.iter_mut().zip(samples) {
            let dry_delayed = channel.input[self.position];
            let wet = channel.output[self.position];
            channel.input[self.position] = *sample;
            *sample = if let Some((_, _, wet_level)) = controls {
                dry_delayed + wet * wet_level.clamp(0.0, 4.0)
            } else {
                wet
            };
            channel.output[self.position] = 0.0;
        }

        self.position = (self.position + 1) % self.fft_size;
        if !self.position.is_multiple_of(self.hop_size) {
            return;
        }

        for channel in &mut self.channels {
            for (index, sample) in channel.frame.iter_mut().enumerate() {
                *sample = channel.input[(self.position + index) % self.fft_size];
            }
            if let Some((note, t60, _)) = controls
                && let Some(resonator) = &mut channel.resonator
            {
                resonator.set_controls(note, t60);
            }
            self.transform
                .process_frame_with_resonator(&mut channel.frame, channel.resonator.as_mut());
            for (index, &sample) in channel.frame.iter().enumerate() {
                let output_index = (self.position + index) % self.fft_size;
                channel.output[output_index] += sample;
            }
        }
    }
}

impl TransparentStft {
    pub fn new(fft_size: usize, hop_size: usize) -> Option<Self> {
        if fft_size < 2
            || !fft_size.is_power_of_two()
            || hop_size == 0
            || !fft_size.is_multiple_of(hop_size)
        {
            return None;
        }

        // Periodic sqrt-Hann analysis and synthesis windows multiply to Hann.
        // The overlap denominator is evaluated for each hop phase so it remains
        // correct if a later quality preset changes N or H.
        let analysis_window: Vec<f32> = (0..fft_size)
            .map(|n| {
                (0.5 - 0.5 * (std::f64::consts::TAU * n as f64 / fft_size as f64).cos())
                    .max(0.0)
                    .sqrt() as f32
            })
            .collect();
        let mut denominator = vec![0.0_f64; hop_size];
        for (index, &window) in analysis_window.iter().enumerate() {
            denominator[index % hop_size] += f64::from(window) * f64::from(window);
        }
        if denominator
            .iter()
            .any(|&value| !value.is_finite() || value <= 0.0)
        {
            return None;
        }
        let synthesis_scale = analysis_window
            .iter()
            .enumerate()
            .map(|(index, &window)| {
                (f64::from(window) / (fft_size as f64 * denominator[index % hop_size])) as f32
            })
            .collect();

        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(fft_size);
        let inverse = planner.plan_fft_inverse(fft_size);
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());

        Some(Self {
            analysis_window,
            synthesis_scale,
            forward,
            inverse,
            spectrum: vec![Complex32::new(0.0, 0.0); fft_size],
            scratch: vec![Complex32::new(0.0, 0.0); scratch_len],
        })
    }

    pub fn process_frame(&mut self, frame: &mut [f32]) {
        self.process_frame_with_resonator(frame, None);
    }

    fn process_frame_with_resonator(
        &mut self,
        frame: &mut [f32],
        resonator: Option<&mut BinResonator>,
    ) {
        if frame.len() != self.spectrum.len() {
            frame.fill(0.0);
            return;
        }

        for ((bin, &sample), &window) in self
            .spectrum
            .iter_mut()
            .zip(frame.iter())
            .zip(self.analysis_window.iter())
        {
            *bin = Complex32::new(sample * window, 0.0);
        }
        self.forward
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);
        if let Some(resonator) = resonator {
            resonator.process_spectrum(&mut self.spectrum);
        }
        self.inverse
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);

        // RustFFT's inverse is unnormalized. The precomputed scale includes
        // both 1/N and the overlap-add denominator, each applied exactly once.
        for ((sample, bin), &scale) in frame
            .iter_mut()
            .zip(self.spectrum.iter())
            .zip(self.synthesis_scale.iter())
        {
            *sample = bin.re * scale;
        }
    }
}

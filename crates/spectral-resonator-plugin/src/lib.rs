use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;
use spectral_dsp::StreamingStft;

mod params;

use params::SpectralResonatorParams;

const FFT_SIZE: usize = 4096;
const HOP_SIZE: usize = 512;

struct SpectralResonator {
    params: Arc<SpectralResonatorParams>,
    stft: Option<StreamingStft>,
    sample_rate: f32,
}

impl Default for SpectralResonator {
    fn default() -> Self {
        Self {
            params: Arc::new(SpectralResonatorParams::default()),
            stft: None,
            sample_rate: 0.0,
        }
    }
}

impl Plugin for SpectralResonator {
    const NAME: &'static str = "Spectral Resonator";
    const VENDOR: &'static str = "Spectral Resonator Project";
    const URL: &'static str = "https://example.invalid/spectral-resonator";
    const EMAIL: &'static str = "audio@example.invalid";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Keep mono and stereo layouts explicit so Bitwig can negotiate either.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();
    type Editor = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        let Some(input_channels) = audio_io_layout.main_input_channels else {
            return false;
        };
        if audio_io_layout.main_output_channels != Some(input_channels)
            || !matches!(input_channels.get(), 1 | 2)
            || !buffer_config.sample_rate.is_finite()
            || buffer_config.sample_rate <= 0.0
        {
            return false;
        }

        let Some(stft) = StreamingStft::new_resonator(
            input_channels.get() as usize,
            FFT_SIZE,
            HOP_SIZE,
            buffer_config.sample_rate,
        ) else {
            return false;
        };
        // The streaming engine schedules each complete frame into its output
        // ring and emits a sample when that ring position cycles one N later.
        context.set_latency_samples(stft.latency_samples());
        self.stft = Some(stft);
        self.sample_rate = buffer_config.sample_rate;
        true
    }

    fn reset(&mut self) {
        if let Some(stft) = &mut self.stft {
            stft.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(stft) = &mut self.stft else {
            for channel in buffer.as_slice() {
                channel.fill(0.0);
            }
            return ProcessStatus::Error("STFT engine is not active");
        };

        for mut frame in buffer.iter_samples() {
            let note = self.params.root_note.value();
            let decay = self.params.decay_t60.smoothed.next();
            let wet_level = self.params.m2_wet_level.smoothed.next();
            // The FFT state is updated at hop boundaries; dry and wet still
            // leave through the same N-sample schedule on each channel.
            stft.process_resonator_frame(frame.iter_mut(), note, decay, wet_level);
            // Advance output automation once per sample frame for both channels.
            let gain = self.params.output_gain.smoothed.next();
            spectral_dsp::apply_frame_gain(frame, gain);
        }

        // Two T60 periods reach roughly -120 dB before the fixed STFT latency.
        let tail_samples = (self.params.decay_t60.value() * 2.0 * self.sample_rate).ceil() as u32;
        ProcessStatus::Tail(tail_samples.saturating_add(FFT_SIZE as u32))
    }
}

impl ClapPlugin for SpectralResonator {
    const CLAP_ID: &'static str = "org.spectral-resonator.dev";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("M2 integer-bin spectral resonator research prototype.");
    const CLAP_MANUAL_URL: Option<&'static str> =
        Some("https://example.invalid/spectral-resonator/manual");
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://example.invalid/spectral-resonator/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Utility];
}

nice_export_clap!(SpectralResonator);

#[cfg(test)]
mod tests {
    use super::*;

    fn render(
        input: &[Vec<f32>],
        fft_size: usize,
        hop_size: usize,
        block_sizes: &[usize],
    ) -> Vec<Vec<f32>> {
        let input_len = input[0].len();
        let total_len = input_len + fft_size + hop_size;
        let mut output = vec![vec![0.0; total_len]; input.len()];
        let mut stft = StreamingStft::new(input.len(), fft_size, hop_size).unwrap();
        let mut position = 0;
        let mut block_index = 0;

        while position < total_len {
            let end = (position + block_sizes[block_index % block_sizes.len()]).min(total_len);
            for (source, destination) in input.iter().zip(output.iter_mut()) {
                let copy_end = end.min(input_len);
                if position < copy_end {
                    destination[position..copy_end].copy_from_slice(&source[position..copy_end]);
                }
            }
            let (left, right) = output.split_at_mut(1);
            for sample_index in position..end {
                stft.process_frame([&mut left[0][sample_index], &mut right[0][sample_index]]);
            }
            position = end;
            block_index += 1;
        }

        output
    }

    #[test]
    fn transparent_stft_reconstructs_stereo_with_exact_latency_and_variable_blocks() {
        for (fft_size, hop_size) in [(2048, 256), (4096, 512), (8192, 1024)] {
            let input_len = fft_size * 2 + 37;
            let left: Vec<f32> = (0..input_len)
                .map(|index| {
                    let tone = (index as f32 * 0.031).sin() * 0.45;
                    let pulse = if index == 17 || index == input_len - 9 {
                        0.4
                    } else {
                        0.0
                    };
                    tone + pulse
                })
                .collect();
            let right: Vec<f32> = left
                .iter()
                .enumerate()
                .map(|(index, &sample)| {
                    if index < input_len / 2 {
                        -sample
                    } else if index == input_len - 5 {
                        0.7
                    } else {
                        0.0
                    }
                })
                .collect();
            let input = [left, right];
            let varied = render(
                &input,
                fft_size,
                hop_size,
                &[1, 7, 64, 127, 128, 257, 512, 1024],
            );
            let fixed = render(&input, fft_size, hop_size, &[128]);

            for (channel_input, (varied_output, fixed_output)) in
                input.iter().zip(varied.iter().zip(fixed.iter()))
            {
                let mut squared_error = 0.0_f64;
                let mut peak_error = 0.0_f32;
                for (index, (&actual, &other)) in
                    varied_output.iter().zip(fixed_output.iter()).enumerate()
                {
                    // No sample may leave the plugin before the declared N-sample delay.
                    let expected = index
                        .checked_sub(fft_size)
                        .and_then(|source_index| channel_input.get(source_index))
                        .copied()
                        .unwrap_or(0.0);
                    let error = (actual - expected).abs();
                    peak_error = peak_error.max(error);
                    squared_error += f64::from(error * error);
                    assert!((actual - other).abs() <= 1e-6);
                }
                let rms_error = (squared_error / varied_output.len() as f64).sqrt();
                assert!(peak_error <= 3e-5, "N={fft_size}, peak={peak_error}");
                assert!(rms_error <= 3e-6, "N={fft_size}, RMS={rms_error}");
            }
        }
    }

    #[test]
    fn mono_impulse_is_delayed_and_reset_clears_pending_output() {
        let fft_size = 2048;
        let hop_size = 256;
        let mut stft = StreamingStft::new(1, fft_size, hop_size).unwrap();
        assert_eq!(stft.latency_samples(), fft_size as u32);

        let mut output = vec![0.0_f32; fft_size * 2 + hop_size];
        output[23] = 0.75;
        for sample in &mut output {
            stft.process_frame([sample]);
        }
        for (index, &sample) in output.iter().enumerate() {
            let expected = if index == 23 + fft_size { 0.75 } else { 0.0 };
            assert!((sample - expected).abs() <= 3e-5, "sample {index}");
        }

        // A reset must discard both analysis history and previously queued OLA audio.
        stft.process_frame([&mut 1.0_f32]);
        stft.reset();
        for _ in 0..fft_size + hop_size {
            let mut sample = 0.0;
            stft.process_frame([&mut sample]);
            assert_eq!(sample, 0.0);
        }
    }

    #[test]
    fn m2_wet_path_is_audible_and_preserves_stereo_isolation() {
        let fft_size = 4096;
        let sample_rate = 44_100.0;
        let mut stft = StreamingStft::new_resonator(2, fft_size, 512, sample_rate).unwrap();
        let input_len = fft_size * 4;
        let mut wet_energy = 0.0_f64;
        for index in 0..input_len + fft_size * 2 {
            let dry = if index < input_len {
                (std::f32::consts::TAU * 220.0 * index as f32 / sample_rate).sin() * 0.4
            } else {
                0.0
            };
            let mut left = dry;
            let mut right = 0.0;
            stft.process_resonator_frame([&mut left, &mut right], 57, 2.0, 1.0);
            assert_eq!(right, 0.0, "right channel leaked at sample {index}");
            if index >= fft_size && index < input_len + fft_size {
                let expected_dry =
                    (std::f32::consts::TAU * 220.0 * (index - fft_size) as f32 / sample_rate).sin()
                        * 0.4;
                wet_energy += f64::from((left - expected_dry).powi(2));
            }
        }
        assert!(wet_energy > 1.0, "M2 wet path remained inaudible");
    }
}

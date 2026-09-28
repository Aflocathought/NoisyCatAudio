use std::num::NonZeroU32;
use std::sync::Arc;

use nice_plug::prelude::*;
use spectral_dsp::{
    DecayCurve, DecayPoint, ModulationControls, ResonatorControls,
    SpectralResonator as ResonatorEngine,
};

mod analyzer;
mod editor;
mod fft;
mod midi;
mod params;
mod state;

#[cfg(test)]
mod process_tests;

use midi::MidiNotes;
use params::{DecayMode, OutputMode, PitchSource, SpectralResonatorParams};

#[cfg(test)]
const FFT_SIZE: usize = 4096;
#[cfg(test)]
const HOP_SIZE: usize = 512;

struct SpectralResonator {
    params: Arc<SpectralResonatorParams>,
    stft: Option<ResonatorEngine>,
    sample_rate: f32,
    notes: MidiNotes,
    pitch_source: PitchSource,
    panic_held: bool,
    capture: analyzer::AudioCapture,
    editor_state: Arc<nice_plug_egui::EguiEditorState>,
    route: [f32; 2],
}

impl Default for SpectralResonator {
    fn default() -> Self {
        Self {
            params: Arc::new(SpectralResonatorParams::default()),
            stft: None,
            sample_rate: 0.0,
            notes: MidiNotes::default(),
            pitch_source: PitchSource::Internal,
            panic_held: false,
            capture: analyzer::AudioCapture::new(analyzer::SharedAnalysis::new()),
            editor_state: nice_plug_egui::EguiEditorState::from_size(
                nice_plug::editor::dpi::LogicalSize::new(1120.0, 780.0),
                1.0,
            ),
            route: [1.0; 2],
        }
    }
}

impl Plugin for SpectralResonator {
    const NAME: &'static str = "Spectral Resonator";
    const VENDOR: &'static str = "Spectral Resonator Project";
    const URL: &'static str = "https://example.invalid/spectral-resonator";
    const EMAIL: &'static str = "audio@example.invalid";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // nice-plug exposes the first layout as CLAP's default. Bitwig should
    // therefore receive stereo by default while mono remains available.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_output_ports: &[new_nonzero_u32(2)],
            names: PortNames {
                layout: Some("Stereo + Wet"),
                aux_outputs: &["Wet / Post Unison"],
                ..PortNames::const_default()
            },
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();
    type Editor = nice_plug_egui::EguiEditor<editor::ResonatorEditor>;

    fn editor(&mut self, _: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(
            self.params.clone(),
            self.capture.shared.clone(),
            self.editor_state.clone(),
        )
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn filter_state(state: &mut PluginState) {
        state::migrate(state);
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

        let fft = self.params.fft_size.value();
        let Some(stft) = ResonatorEngine::new(
            input_channels.get() as usize,
            fft.samples(),
            fft.hop(),
            buffer_config.sample_rate,
        ) else {
            return false;
        };
        // The streaming engine schedules each complete frame into its output
        // ring and emits a sample when that ring position cycles one N later.
        context.set_latency_samples(stft.latency_samples());
        self.stft = Some(stft);
        self.sample_rate = buffer_config.sample_rate;
        self.notes.reset();
        self.pitch_source = self.params.pitch_source.value();
        self.panic_held = false;
        self.capture.set_latency(fft.samples());
        self.capture.reset(self.sample_rate);
        self.capture.shared.fft.activate(fft, self.sample_rate);
        self.route = route_gains(self.params.output_mode.value());
        true
    }

    fn deactivate(&mut self) {
        self.capture.shared.fft.deactivate();
        self.stft = None;
    }

    fn reset(&mut self) {
        self.notes.reset();
        self.pitch_source = self.params.pitch_source.value();
        self.panic_held = false;
        self.capture.reset(self.sample_rate);
        self.route = route_gains(self.params.output_mode.value());
        if let Some(stft) = &mut self.stft {
            stft.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let Some(stft) = &mut self.stft else {
            for channel in buffer.as_slice() {
                channel.fill(0.0);
            }
            return ProcessStatus::Error("STFT engine is not active");
        };

        // Keep processing with the old engine/latency until the host honors
        // the request. FFT plans, buffers and tails are replaced in activate().
        if self
            .capture
            .shared
            .fft
            .request_restart(self.params.fft_size.value())
        {
            context.request_restart();
        }

        // A host can flush notes in a zero-length buffer after changing the
        // pitch source. Synchronize first so those notes survive the next block.
        let source = self.params.pitch_source.value();
        let panic = self.params.panic.value();
        if source != self.pitch_source || (panic && !self.panic_held) {
            self.notes.reset();
            stft.panic();
            self.pitch_source = source;
        }
        self.panic_held = panic;
        stft.set_voice_limit(self.params.max_polyphony.value() as usize);
        let capture_enabled = self.editor_state.is_open() && self.capture.enabled();
        let mut next_event = context.next_event();
        for (sample_index, mut frame) in buffer.iter_samples().enumerate() {
            stft.set_voice_limit(self.params.max_polyphony.value() as usize);
            let mut input = [0.0; 2];
            for (i, sample) in frame.iter_mut().enumerate() {
                input[i] = *sample;
            }
            if frame.len() == 1 {
                input[1] = input[0];
            }
            let source = self.params.pitch_source.value();
            let panic = self.params.panic.value();
            if source != self.pitch_source || (panic && !self.panic_held) {
                self.notes.reset();
                stft.panic();
                self.pitch_source = source;
            }
            self.panic_held = panic;
            // Consume every event at its input-buffer offset, including several
            // on the same sample. The spectral engine alone quantizes to hops.
            while let Some(event) = next_event {
                if event.timing() > sample_index as u32 {
                    break;
                }
                if source == PitchSource::Midi && !panic {
                    self.notes.handle(event, |action| action.apply(stft));
                }
                next_event = context.next_event();
            }
            let note = if panic {
                None
            } else if source == PitchSource::Internal {
                Some(self.params.root_note.value())
            } else {
                None
            };
            let decay = self.params.decay_t60.smoothed.next();
            let wet_level = self.params.m2_wet_level.smoothed.next();
            let low_hz = self.params.low_mid_hz.smoothed.next();
            let high_hz = self.params.mid_high_hz.smoothed.next();
            // Advance node smoothers even in legacy mode so switching modes
            // cannot revive stale parameter trajectories.
            let decay_curve = DecayCurve {
                points: std::array::from_fn(|i| DecayPoint {
                    hz: self.params.decay_points[i].hz.smoothed.next(),
                    seconds: self.params.decay_points[i].seconds.smoothed.next(),
                }),
            };
            // Advance hidden controls too: switching modes restores the current
            // automation value, never a stale smoothing ramp or a reset value.
            let attack_ms = self.params.attack_ms.smoothed.next();
            let emphasis = self.params.attack_emphasis_db.smoothed.next();
            let controls = ResonatorControls {
                note,
                note_token: 0,
                velocity: 1.0,
                harmonics: self.params.harmonics.value() as usize,
                t60: decay,
                attack_ms: (self.params.attack_mode.value() == params::AttackMode::Independent)
                    .then_some(attack_ms),
                transient: (self.params.attack_mode.value() == params::AttackMode::Reshape).then(
                    || spectral_dsp::TransientControls {
                        attack_ms,
                        emphasis: util::db_to_gain(emphasis),
                    },
                ),
                hf_damp: self.params.hf_damp.smoothed.next(),
                lf_damp: self.params.lf_damp.smoothed.next(),
                decay_curve: (self.params.decay_mode.value() == DecayMode::Curve)
                    .then_some(decay_curve),
                modulation: ModulationControls {
                    mode: self.params.mod_mode.value().into(),
                    rate_hz: self.params.mod_rate_hz.smoothed.next(),
                    amount: self.params.mod_amount.smoothed.next() * 0.01,
                    pitch_semitones: self.params.mod_pitch_semitones.smoothed.next(),
                    grain_ms: self.params.grain_ms.smoothed.next(),
                    unison_voices: self.params.unison_voices.value() as usize,
                    unison_detune_cents: self.params.unison_detune_cents.smoothed.next(),
                },
                unison_mode: self.params.unison_mode.value().into(),
                voice_spread: self.params.voice_spread.smoothed.next() * 0.01,
                input_gain: util::db_to_gain(self.params.input_send_db.smoothed.next()),
                wet_level,
                align_wet: self.params.align_wet.value(),
                mid_mix: self.params.mid_mix.smoothed.next() * 0.01,
                low_hz,
                high_hz,
                mute_low: self.params.mute_low.value(),
                mute_high: self.params.mute_high.value(),
            };
            if source == PitchSource::Midi {
                stft.process_poly_frame(frame.iter_mut(), controls);
            } else {
                stft.process_frame(frame.iter_mut(), controls);
            }
            // Advance output automation once per sample frame for both channels.
            let gain = self.params.output_gain.smoothed.next();
            let (dry, wet) = stft.output_parts();
            let target = route_gains(self.params.output_mode.value());
            for (value, target) in self.route.iter_mut().zip(target) {
                *value += (target - *value).clamp(
                    -1.0 / (self.sample_rate * 0.02),
                    1.0 / (self.sample_rate * 0.02),
                );
            }
            for (i, sample) in frame.iter_mut().enumerate() {
                *sample = dry[i] * self.route[0] + wet[i] * self.route[1];
            }
            // This output is always wet-only, independent of the main route.
            if let Some(output) = aux.outputs.first_mut() {
                for (i, channel) in output.as_slice().iter_mut().enumerate() {
                    channel[sample_index] = wet[i] * gain;
                }
            }
            let mut display_wet = wet.map(|v| v * gain);
            if frame.len() == 1 {
                display_wet[1] = display_wet[0];
            }
            self.capture.push(input, display_wet, capture_enabled);
            spectral_dsp::apply_frame_gain(frame, gain);
        }

        // Frameworks may deliver boundary events at buffer.len(), including a
        // zero-length flush. Preserve them for the next audio sample.
        while let Some(event) = next_event {
            if self.params.pitch_source.value() == PitchSource::Midi && !self.params.panic.value() {
                self.notes.handle(event, |action| action.apply(stft));
            }
            next_event = context.next_event();
        }

        // Two T60 periods reach roughly -120 dB before the fixed STFT latency.
        // Include curve times even just after a mode change, conservatively
        // retaining tails while the old/new decay laws crossfade.
        let longest_decay = self
            .params
            .decay_points
            .iter()
            .fold(self.params.decay_t60.value(), |seconds, point| {
                seconds.max(point.seconds.value())
            });
        let tail_samples = (longest_decay * 2.0 * self.sample_rate).ceil() as u32;
        ProcessStatus::Tail(
            tail_samples
                .saturating_add(stft.latency_samples())
                .saturating_add(stft.effect_tail_samples()),
        )
    }
}

impl ClapPlugin for SpectralResonator {
    fn remote_controls(
        &self,
        context: &mut impl nice_plug::context::remote_controls::RemoteControlsContext,
    ) {
        use nice_plug::context::remote_controls::{RemoteControlsPage, RemoteControlsSection};
        let p = &self.params;
        context.add_section("Spectral Resonator", |section| {
            section.add_page("Perform", |page| {
                page.add_param(&p.low_mid_hz);
                page.add_param(&p.mid_high_hz);
                page.add_param(&p.mid_mix);
                page.add_param(&p.m2_wet_level);
                page.add_param(&p.decay_t60);
                page.add_param(&p.root_note);
                page.add_param(&p.voice_spread);
                page.add_param(&p.output_gain);
            });
            section.add_page("Motion", |page| {
                page.add_param(&p.mod_mode);
                page.add_param(&p.mod_rate_hz);
                page.add_param(&p.mod_amount);
                page.add_param(&p.mod_pitch_semitones);
                page.add_param(&p.grain_ms);
                page.add_param(&p.unison_mode);
                page.add_param(&p.unison_voices);
                page.add_param(&p.unison_detune_cents);
            });
            section.add_page("Resonance", |page| {
                page.add_param(&p.pitch_source);
                page.add_param(&p.harmonics);
                page.add_param(&p.decay_mode);
                page.add_param(&p.lf_damp);
                page.add_param(&p.hf_damp);
                page.add_param(&p.input_send_db);
                page.add_param(&p.output_mode);
                page.add_param(&p.panic);
            });
            section.add_page("Envelope", |page| {
                page.add_param(&p.attack_mode);
                page.add_param(&p.attack_ms);
                page.add_param(&p.decay_t60);
                page.add_param(&p.decay_mode);
                page.add_param(&p.lf_damp);
                page.add_param(&p.hf_damp);
                page.add_param(&p.max_polyphony);
                page.add_param(&p.align_wet);
            });
            section.add_page("Transient", |page| {
                page.add_param(&p.attack_mode);
                page.add_param(&p.attack_ms);
                page.add_param(&p.attack_emphasis_db);
                page.add_param(&p.align_wet);
            });
            section.add_page("Routing", |page| {
                page.add_param(&p.output_mode);
                page.add_param(&p.output_gain);
                page.add_param(&p.mute_low);
                page.add_param(&p.mute_high);
            });
        });
    }
    // This engine deliberately uses identical DSP in realtime and offline mode.
    // Requesting a restart inside render.set() adds an unnecessary handshake
    // while Bitwig is already transitioning to Bounce. Keep mode changes local;
    // sample-rate/layout changes still use the regular activate/reset lifecycle.
    const CLAP_REACTIVATE_ON_RENDER_MODE_CHANGE: bool = false;

    const CLAP_ID: &'static str = "org.spectral-resonator.dev";
    const CLAP_DESCRIPTION: Option<&'static str> = Some(
        "Polyphonic spectral resonator with chorus, wander, granular, unison and frequency decay curve.",
    );
    const CLAP_MANUAL_URL: Option<&'static str> =
        Some("https://example.invalid/spectral-resonator/manual");
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://example.invalid/spectral-resonator/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Utility];
}

nice_export_clap!(SpectralResonator);

fn route_gains(mode: OutputMode) -> [f32; 2] {
    match mode {
        OutputMode::Mixed => [1.0, 1.0],
        OutputMode::Dry => [1.0, 0.0],
        OutputMode::Wet => [0.0, 1.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral_dsp::StreamingStft;

    #[test]
    fn stereo_is_the_default_clap_layout() {
        let default_layout = SpectralResonator::AUDIO_IO_LAYOUTS[0];
        assert_eq!(default_layout.main_input_channels.unwrap().get(), 2);
        assert_eq!(default_layout.main_output_channels.unwrap().get(), 2);
    }

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

    #[test]
    fn crossover_retains_low_and_high_dry_while_replacing_mid() {
        let sample_rate = 48_000.0_f32;
        let fft_size = 4096;
        let input_len = fft_size * 5;
        let mut measured = [0.0_f64; 3];
        for (slot, frequency) in measured.iter_mut().zip([100.0_f32, 1_000.0, 10_000.0]) {
            let mut stft = StreamingStft::new_resonator(1, fft_size, 512, sample_rate).unwrap();
            let mut output_energy = 0.0_f64;
            let mut input_energy = 0.0_f64;
            for index in 0..input_len + fft_size {
                let original = if index < input_len {
                    (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.4
                } else {
                    0.0
                };
                let mut sample = original;
                stft.process_crossover_frame([&mut sample], 57, 2.0, 0.0, 250.0, 4_000.0);
                if (fft_size * 2..input_len).contains(&index) {
                    output_energy += f64::from(sample * sample);
                    input_energy += f64::from(original * original);
                }
            }
            *slot = (output_energy / input_energy).sqrt();
        }
        assert!(measured[0] > 0.9, "low dry gain={}", measured[0]);
        assert!(measured[1] < 0.1, "mid dry leaked={}", measured[1]);
        assert!(measured[2] > 0.9, "high dry gain={}", measured[2]);
    }

    #[test]
    fn crossover_left_output_does_not_depend_on_right_input() {
        let render = |right_enabled: bool| {
            let sample_rate = 44_100.0_f32;
            let fft_size = 4096;
            let mut stft = StreamingStft::new_resonator(2, fft_size, 512, sample_rate).unwrap();
            let mut left_output = Vec::new();
            let mut right_output = Vec::new();
            for index in 0..fft_size * 5 {
                let mut left =
                    (std::f32::consts::TAU * 440.0 * index as f32 / sample_rate).sin() * 0.25;
                let mut right = if right_enabled {
                    (std::f32::consts::TAU * 660.0 * index as f32 / sample_rate).sin() * 0.25
                } else {
                    0.0
                };
                stft.process_crossover_frame([&mut left, &mut right], 69, 2.0, 4.0, 250.0, 4_000.0);
                left_output.push(left);
                right_output.push(right);
            }
            (left_output, right_output)
        };
        let (left_with_right, right_present) = render(true);
        let (left_alone, right_silent) = render(false);
        assert!(
            left_with_right
                .iter()
                .zip(left_alone.iter())
                .all(|(with_right, alone)| (with_right - alone).abs() <= 1e-6)
        );
        assert!(right_silent.iter().all(|&sample| sample == 0.0));
        assert!(right_present.iter().any(|&sample| sample.abs() > 0.01));
    }

    #[test]
    fn crossover_wet_gain_changes_mid_band_audibly() {
        let render = |wet_level: f32| {
            let sample_rate = 48_000.0_f32;
            let fft_size = 4096;
            let mut stft = StreamingStft::new_resonator(1, fft_size, 512, sample_rate).unwrap();
            let mut output = Vec::new();
            for index in 0..fft_size * 6 {
                let mut sample =
                    (std::f32::consts::TAU * 440.0 * index as f32 / sample_rate).sin() * 0.25;
                stft.process_crossover_frame([&mut sample], 69, 2.0, wet_level, 250.0, 4_000.0);
                output.push(sample);
            }
            output
        };
        let dry_middle = render(0.0);
        let wet_middle = render(4.0);
        let difference_energy: f64 = dry_middle
            .iter()
            .zip(wet_middle.iter())
            .skip(4096 * 3)
            .map(|(&dry, &wet)| f64::from((wet - dry).powi(2)))
            .sum();
        let difference_rms = (difference_energy / (4096 * 3) as f64).sqrt();
        eprintln!("440 Hz wet-minus-muted RMS at level 4: {difference_rms:.6}");
        assert!(difference_rms > 0.05, "wet RMS difference={difference_rms}");
    }
}

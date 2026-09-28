use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex32};

use crate::{
    crossover::CrossoverMask,
    decay::DecayCurve,
    modulation::{ModulationBank, ModulationControls, ModulationKernel},
    partial::{DEFAULT_PARTIALS, MAX_PARTIALS, NoteTemplate},
    post_unison::{PostUnison, UnisonMode},
    transient::{TransientControls, TransientShaper},
    wet_delay::WetDelay,
};

const ZERO: Complex32 = Complex32::new(0.0, 0.0);
const FIRST_NOTE: i32 = 33;
const LAST_NOTE: i32 = 93;
pub const MAX_VOICES: usize = 16;
// Alternate left/right before filling the inner positions. Assignment follows
// note lifetime/order, not the current active-voice count, so tails never move
// when another note is released or a physical voice slot is reused.
// Keep the established eight-position cycle independent of pool capacity, so
// increasing polyphony does not change the spatial pattern of saved projects.
const VOICE_PAN_POSITIONS: [f32; 8] = [
    -1.0,
    1.0,
    -5.0 / 7.0,
    5.0 / 7.0,
    -3.0 / 7.0,
    3.0 / 7.0,
    -1.0 / 7.0,
    1.0 / 7.0,
];

#[cfg(test)]
#[path = "resonator_tests.rs"]
mod poly_tests;

#[cfg(test)]
#[path = "resonator_envelope_tests.rs"]
mod envelope_tests;

#[cfg(test)]
#[path = "resonator_timing_tests.rs"]
mod timing_tests;

#[cfg(test)]
#[path = "resonator_transient_tests.rs"]
mod transient_tests;

#[cfg(test)]
#[path = "resonator_band_mute_tests.rs"]
mod band_mute_tests;

/// Shared input-time controls. `note` drives the internal/mono convenience
/// path; the polyphonic path receives identities through note_on/note_off.
#[derive(Clone, Copy, Debug)]
pub struct ResonatorControls {
    pub note: Option<i32>,
    /// Stable identity for one held note, separate from its musical pitch.
    pub note_token: u64,
    pub velocity: f32,
    pub harmonics: usize,
    pub t60: f32,
    /// None retains the natural resonator buildup. Some sets the independent
    /// rising response's time to 90%, in milliseconds (0 means one hop).
    pub attack_ms: Option<f32>,
    /// Shape reconstructed wet onsets against input delayed by FFT latency.
    /// This mode also uses the fastest spectral excitation response.
    pub transient: Option<TransientControls>,
    pub hf_damp: f32,
    pub lf_damp: f32,
    /// None selects legacy harmonic-index damping; Some selects absolute Hz.
    pub decay_curve: Option<DecayCurve>,
    pub modulation: ModulationControls,
    pub unison_mode: UnisonMode,
    /// Automatic wet-voice stereo balance width, normalized to 0..1.
    pub voice_spread: f32,
    pub input_gain: f32,
    pub wet_level: f32,
    /// Wet pre-delay in synthesis windows, 0..1 (0.5 is half a window).
    /// This shifts wet timing without changing host/dry latency.
    pub align_wet: f32,
    /// Linear dry/wet blend within the middle band only, normalized to 0..1.
    pub mid_mix: f32,
    pub low_hz: f32,
    pub high_hz: f32,
    /// Mute only the outer dry contributions, never resonant excitation/tails.
    pub mute_low: bool,
    pub mute_high: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partial::frequency;

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|&x| f64::from(x).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    #[test]
    fn output_tail_pitch_and_t60_at_three_sample_rates() {
        let mut worst_cents = 0.0_f64;
        let mut worst_decay_error = 0.0_f64;
        for rate in [44_100, 48_000, 96_000] {
            let mut engine = SpectralResonator::new(1, 4096, 512, rate as f32).unwrap();
            for note in [33, 45, 57, 69, 93] {
                engine.reset();
                engine.voices[0].note = Some(note);
                engine.voices[0].gain = 1.0;
                engine.voices[0].state[0][0] = Complex32::new(1.0, 0.0);
                let controls = ResonatorControls {
                    note: None,
                    harmonics: 1,
                    t60: 1.0,
                    hf_damp: 0.0,
                    low_hz: 1.0,
                    high_hz: 12_000.0,
                    wet_level: 1.0,
                    ..Default::default()
                };
                let mut output = vec![0.0; rate * 2 + 4096];
                for sample in &mut output {
                    engine.process_frame([sample], controls);
                }
                let start = 4096 + rate / 5;
                let end = start + rate / 5;
                let ratio = rms(&output[start + rate..end + rate]) / rms(&output[start..end]);
                let decay_error = (ratio / 0.001 - 1.0).abs();
                worst_decay_error = worst_decay_error.max(decay_error);
                assert!(
                    decay_error < 0.02,
                    "rate={rate} note={note} T60 ratio={ratio}"
                );
                // Sub-sample zero crossings measure frequency independently of
                // FFT bin spacing, using only the rendered audio tail.
                let mut crossings = Vec::new();
                for i in start..start + rate {
                    let (a, b) = (output[i] as f64, output[i + 1] as f64);
                    if a <= 0.0 && b > 0.0 {
                        crossings.push(i as f64 - a / (b - a));
                    }
                }
                let measured = rate as f64 * (crossings.len() - 1) as f64
                    / (crossings.last().unwrap() - crossings[0]);
                let cents = 1200.0 * (measured / frequency(note)).log2();
                worst_cents = worst_cents.max(cents.abs());
                assert!(
                    cents.abs() <= 1.0,
                    "rate={rate}, note={note}, cents={cents}"
                );
            }
        }
        eprintln!(
            "rendered tail worst pitch error={worst_cents:.6} cents; T60 relative error={worst_decay_error:.6}"
        );
    }

    #[test]
    fn new_engine_preserves_stereo_and_survives_controls_and_reset() {
        let rate = 48_000.0;
        let mut stereo = SpectralResonator::new(2, 4096, 512, rate).unwrap();
        let mut mono = SpectralResonator::new(1, 4096, 512, rate).unwrap();
        let mut energy = 0.0;
        for i in 0..24_000 {
            let input = (std::f32::consts::TAU * 440.0 * i as f32 / rate).sin() * 0.25;
            let mut left = input;
            let mut alone = input;
            let mut right = if i < 12_000 {
                0.0
            } else {
                (i as f32 * 0.07).sin() * 0.3
            };
            let controls = ResonatorControls {
                note: Some(if i < 20_000 { 69 } else { 72 }),
                harmonics: if i < 18_000 { 64 } else { 1 },
                t60: 0.1,
                ..Default::default()
            };
            stereo.process_frame([&mut left, &mut right], controls);
            mono.process_frame([&mut alone], controls);
            assert_eq!(left, alone);
            if i < 12_000 {
                assert_eq!(right, 0.0);
            }
            assert!(left.is_finite() && right.is_finite());
            if (8_000..12_000).contains(&i) {
                energy += f64::from(left).powi(2);
            }
        }
        assert!((energy / 4_000.0).sqrt() > 0.1, "wet output is too quiet");
        stereo.reset();
        for _ in 0..8192 {
            let mut left = f32::NAN;
            let mut right = f32::INFINITY;
            stereo.process_frame(
                [&mut left, &mut right],
                ResonatorControls {
                    note: None,
                    ..Default::default()
                },
            );
            assert_eq!((left, right), (0.0, 0.0));
        }
    }

    #[test]
    fn short_gate_excites_tail_then_panic_fades_it() {
        let mut engine = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
        let mut controls = ResonatorControls {
            note: None,
            t60: 2.0,
            harmonics: 1,
            ..Default::default()
        };
        for i in 0..8192 {
            // One 64-sample note entirely within a hop must not disappear.
            controls.note = if (8000..8064).contains(&i) {
                Some(69)
            } else {
                None
            };
            let mut sample = (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin();
            engine.process_frame([&mut sample], controls);
        }
        assert!(engine.voices[0].state[0][0].norm() > 1e-4);
        controls.note = None;
        let before = engine.voices[0].state[0][0].norm();
        for _ in 0..512 {
            engine.process_frame([&mut 0.0], controls);
        }
        let after = engine.voices[0].state[0][0].norm();
        assert!(after > 0.0 && after < before);
        engine.panic();
        for _ in 0..8192 {
            engine.process_frame([&mut 0.0], controls);
        }
        assert!(engine.voices.iter().all(|voice| voice.note.is_none()));
        for _ in 0..4096 {
            let mut sample = 0.0;
            engine.process_frame([&mut sample], controls);
            assert!(sample.abs() < 1e-10);
        }
    }

    #[test]
    fn damping_shortens_only_the_expected_partials() {
        let mut engine = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
        engine.render_hop(ResonatorControls {
            note: None,
            hf_damp: 0.0,
            lf_damp: 0.0,
            decay_curve: None,
            modulation: ModulationControls::default(),
            unison_mode: UnisonMode::Spectral,
            ..Default::default()
        });
        let baseline = engine.radii;
        engine.render_hop(ResonatorControls {
            note: None,
            hf_damp: 1.0,
            lf_damp: 0.0,
            ..Default::default()
        });
        assert_eq!(engine.radii[0], baseline[0]);
        assert_eq!(engine.radii[3], baseline[3]);
        assert!(engine.radii[31] < baseline[31]);
        engine.render_hop(ResonatorControls {
            note: None,
            hf_damp: 0.0,
            lf_damp: 1.0,
            ..Default::default()
        });
        assert!(engine.radii[0] < baseline[0]);
        assert_eq!(engine.radii[3], baseline[3]);
        assert_eq!(engine.radii[31], baseline[31]);
    }

    #[test]
    fn new_crossover_keeps_outer_dry_bands_and_mutes_middle() {
        let mut engine = SpectralResonator::new(1, 4096, 512, 48_000.0).unwrap();
        let mut gains = [0.0_f64; 5];
        for (gain, hz) in gains
            .iter_mut()
            .zip([100.0, 250.0, 1000.0, 4000.0, 10_000.0])
        {
            engine.reset();
            let controls = ResonatorControls {
                note: None,
                wet_level: 0.0,
                ..Default::default()
            };
            let mut output_energy = 0.0;
            let mut input_energy = 0.0;
            for i in 0..24_000 {
                let input = (std::f64::consts::TAU * hz * i as f64 / 48_000.0).sin() as f32 * 0.25;
                let mut sample = input;
                engine.process_frame([&mut sample], controls);
                if i >= 12_000 {
                    output_energy += f64::from(sample).powi(2);
                    input_energy += f64::from(input).powi(2);
                }
            }
            *gain = (output_energy / input_energy).sqrt();
        }
        eprintln!("M3 dry gains at 100/250/1000/4000/10000 Hz: {gains:?}");
        assert!(gains[0] > 0.98 && gains[4] > 0.98);
        assert!(gains[2] < 0.01);
        assert!((gains[1] - 0.5).abs() < 0.02 && (gains[3] - 0.5).abs() < 0.02);

        engine.reset();
        let mut impulse = vec![0.0; 16_384];
        impulse[4237] = 1.0;
        for sample in &mut impulse {
            engine.process_frame(
                [sample],
                ResonatorControls {
                    note: None,
                    wet_level: 0.0,
                    ..Default::default()
                },
            );
        }
        let peak_index = impulse
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
        assert_eq!(peak_index, 4237 + engine.latency_samples() as usize);
    }

    #[test]
    fn middle_mix_restores_dry_and_blends_linearly_in_stereo() {
        let mut worst_dry_error = 0.0_f32;
        let mut worst_blend_error = 0.0_f32;
        for (rate, low_hz, high_hz) in [
            (44_100.0, 80.0, 1600.0),
            (48_000.0, 250.0, 4000.0),
            (96_000.0, 800.0, 12000.0),
        ] {
            let mut engines =
                [0.0, 0.5, 1.0].map(|_| SpectralResonator::new(2, 4096, 512, rate).unwrap());
            let input: Vec<[f32; 2]> = (0..12_000)
                .map(|i| {
                    [
                        (i as f32 * 0.031).sin() * 0.3 + if i == 73 { 0.4 } else { 0.0 },
                        (i as f32 * 0.127).sin() * 0.2,
                    ]
                })
                .collect();
            let mut wet_difference = 0.0_f64;
            for i in 0..input.len() + 4096 {
                let dry_input = input.get(i).copied().unwrap_or([0.0; 2]);
                let mut outputs = [dry_input; 3];
                for ((engine, output), mix) in
                    engines.iter_mut().zip(&mut outputs).zip([0.0, 0.5, 1.0])
                {
                    engine.process_frame(
                        output.iter_mut(),
                        ResonatorControls {
                            note: Some(69),
                            mid_mix: mix,
                            low_hz,
                            high_hz,
                            ..Default::default()
                        },
                    );
                }
                let expected = i
                    .checked_sub(4096)
                    .and_then(|i| input.get(i))
                    .copied()
                    .unwrap_or([0.0; 2]);
                for (channel, expected) in expected.into_iter().enumerate() {
                    worst_dry_error = worst_dry_error.max((outputs[0][channel] - expected).abs());
                    worst_blend_error = worst_blend_error.max(
                        (outputs[1][channel] - (outputs[0][channel] + outputs[2][channel]) * 0.5)
                            .abs(),
                    );
                    wet_difference += f64::from(outputs[2][channel] - outputs[0][channel]).powi(2);
                }
            }
            assert!(
                wet_difference > 0.1,
                "wet endpoint was indistinguishable from dry"
            );
            // Mix is an output blend: the running resonant tail must survive at
            // 0%, instead of being reset or stopping its excitation/decay.
            assert_eq!(engines[0].voices[0].state, engines[2].voices[0].state);
        }
        eprintln!(
            "Mid Mix: worst dry error={worst_dry_error:e}, worst 50% blend error={worst_blend_error:e}"
        );
        assert!(worst_dry_error < 3e-6);
        assert!(worst_blend_error < 3e-6);
    }
}

impl Default for ResonatorControls {
    fn default() -> Self {
        Self {
            note: Some(57),
            note_token: 0,
            velocity: 1.0,
            harmonics: DEFAULT_PARTIALS,
            t60: 2.0,
            attack_ms: None,
            transient: None,
            hf_damp: 0.25,
            lf_damp: 0.0,
            decay_curve: None,
            modulation: ModulationControls::default(),
            unison_mode: UnisonMode::Spectral,
            voice_spread: 0.0,
            input_gain: 1.0,
            wet_level: 4.0,
            align_wet: 0.0,
            mid_mix: 1.0,
            low_hz: 250.0,
            high_hz: 4_000.0,
            mute_low: false,
            mute_high: false,
        }
    }
}

struct ChannelState {
    input: Vec<f32>,
    // Separate real dry/wet streams allow Unison to process only the wet sum.
    output: Vec<f32>,
    wet_output: Vec<f32>,
}

#[derive(Clone)]
struct Voice {
    note: Option<i32>,
    token: u64,
    state: [[Complex32; MAX_PARTIALS]; 2],
    curve_radii: [f32; MAX_PARTIALS],
    curve_revision: u64,
    rendered_partials: usize,
    pan: f32,
    pan_gains: [f32; 2],
    gain: f32,
    fading: bool,
    gate: bool,
    velocity: f32,
    gate_sum: f32,
    drive: f32,
    energy: f32,
    order: u64,
    pending: Option<PendingVoice>,
}

#[derive(Clone, Copy)]
struct PendingVoice {
    note: i32,
    token: u64,
    velocity: f32,
    gate: bool,
    gate_sum: f32,
    order: u64,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            note: None,
            token: 0,
            state: [[ZERO; MAX_PARTIALS]; 2],
            curve_radii: [0.0; MAX_PARTIALS],
            curve_revision: 0,
            rendered_partials: 0,
            pan: 0.0,
            pan_gains: [1.0; 2],
            gain: 0.0,
            fading: false,
            gate: false,
            velocity: 0.0,
            gate_sum: 0.0,
            drive: 0.0,
            energy: 0.0,
            order: 0,
            pending: None,
        }
    }
}

impl Voice {
    fn start(note: PendingVoice) -> Self {
        Self {
            note: Some(note.note),
            token: note.token,
            velocity: note.velocity,
            gate: note.gate,
            gate_sum: note.gate_sum,
            order: note.order,
            ..Self::default()
        }
    }
}

/// Fractional-frequency spectral resonator. All templates, FFT storage, rings
/// and sixteen independent voices are allocated before the audio callback.
pub struct SpectralResonator {
    #[cfg(feature = "profiling")]
    profile: [std::time::Duration; 6],
    rate: f32,
    size: usize,
    hop: usize,
    position: usize,
    channels: Vec<ChannelState>,
    templates: Vec<NoteTemplate>,
    // Keep the enlarged fixed pool on the heap so activation/moving the engine
    // does not put the entire voice pool on a host's small audio-thread stack.
    // Its length and allocation never change in process/reset/note handling.
    voices: Box<[Voice]>,
    voice_limit: usize,
    modulation_banks: Box<[ModulationBank]>,
    modulation_kernel: ModulationKernel,
    post_unison: PostUnison,
    // Exact time-domain contributions, retained for metering and host routing.
    last_dry: [f32; 2],
    last_wet: [f32; 2],
    wet_delay: WetDelay,
    transient: TransientShaper,
    post_guard: f32,
    note_order: u64,
    mono_note: Option<(i32, u64)>,
    radii: [f32; MAX_PARTIALS],
    damping_controls: [f32; 3],
    decay_curve: Option<DecayCurve>,
    curve_revision: u64,
    curve_mix: f32,
    attack_mix: f32,
    attack_radius: f32,
    crossover: CrossoverMask,
    outer_band_gains: Option<[f32; 2]>,
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    input_spectrum: Vec<Complex32>,
    wet_spectrum: Vec<Complex32>,
    output_spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

fn bounded(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

impl SpectralResonator {
    pub fn new(channels: usize, size: usize, hop: usize, rate: f32) -> Option<Self> {
        if !(1..=2).contains(&channels)
            || !rate.is_finite()
            || !(1_000.0..=768_000.0).contains(&rate)
            || !(256..=16_384).contains(&size)
            || !size.is_multiple_of(2)
            || hop == 0
            || hop > size / 2
            || !size.is_multiple_of(hop)
        {
            return None;
        }
        let mut planner = FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let scratch_len = forward
            .get_inplace_scratch_len()
            .max(inverse.get_inplace_scratch_len());
        Some(Self {
            #[cfg(feature = "profiling")]
            profile: [std::time::Duration::ZERO; 6],
            rate,
            size,
            hop,
            position: 0,
            channels: (0..channels)
                .map(|_| ChannelState {
                    input: vec![0.0; size],
                    output: vec![0.0; size],
                    wet_output: vec![0.0; size],
                })
                .collect(),
            templates: (FIRST_NOTE..=LAST_NOTE)
                .map(|note| NoteTemplate::new(note, rate as f64, size, hop))
                .collect(),
            voices: (0..MAX_VOICES).map(|_| Voice::default()).collect(),
            voice_limit: MAX_VOICES,
            modulation_banks: (0..MAX_VOICES).map(|_| ModulationBank::new()).collect(),
            modulation_kernel: ModulationKernel::new(size, rate),
            post_unison: PostUnison::new(rate),
            last_dry: [0.0; 2],
            last_wet: [0.0; 2],
            wet_delay: WetDelay::new(size, rate),
            transient: TransientShaper::new(rate),
            post_guard: 0.0,
            note_order: 0,
            mono_note: None,
            radii: [0.0; MAX_PARTIALS],
            damping_controls: [f32::NAN; 3],
            decay_curve: None,
            curve_revision: 1,
            curve_mix: 0.0,
            attack_mix: 0.0,
            attack_radius: 0.0,
            crossover: CrossoverMask::new(rate, size)?,
            outer_band_gains: None,
            forward,
            inverse,
            input_spectrum: vec![ZERO; size],
            wet_spectrum: vec![ZERO; size],
            output_spectrum: vec![ZERO; size],
            scratch: vec![ZERO; scratch_len],
        })
    }

    pub fn latency_samples(&self) -> u32 {
        self.size as u32
    }

    /// Contributions to the most recently emitted frame, before output gain.
    /// Wet is measured after Post Unison, including its variable delay.
    pub fn output_parts(&self) -> ([f32; 2], [f32; 2]) {
        (self.last_dry, self.last_wet)
    }

    /// Variable delay is part of the wet effect, not dry-path host latency.
    pub fn effect_tail_samples(&self) -> u32 {
        // Keep this conservative while switching back to the original timing:
        // the delayed branch and its crossfade can still emit pending audio.
        self.post_unison.max_delay_samples() + self.size as u32
    }

    /// Offline accumulated times: controls, modulation, input FFT,
    /// projection/state update, wet synthesis, mix/inverse FFT/output.
    #[cfg(feature = "profiling")]
    pub fn take_profile(&mut self) -> [std::time::Duration; 6] {
        std::mem::take(&mut self.profile)
    }

    pub fn reset(&mut self) {
        self.position = 0;
        for channel in &mut self.channels {
            channel.input.fill(0.0);
            channel.output.fill(0.0);
            channel.wet_output.fill(0.0);
        }
        for voice in &mut self.voices {
            *voice = Voice::default();
        }
        for bank in &mut self.modulation_banks {
            bank.reset();
        }
        self.note_order = 0;
        self.mono_note = None;
        self.curve_mix = 0.0;
        self.attack_mix = 0.0;
        self.post_unison.reset();
        self.last_dry = [0.0; 2];
        self.last_wet = [0.0; 2];
        self.wet_delay.reset();
        self.transient.reset();
        self.post_guard = 0.0;
        self.outer_band_gains = None;
    }

    /// Musical panic fades wet states; it deliberately preserves aligned dry
    /// audio. Host reset, in contrast, clears every pending output sample.
    pub fn panic(&mut self) {
        for voice in &mut self.voices {
            voice.fading = true;
            voice.gate = false;
            voice.gate_sum = 0.0;
            voice.pending = None;
        }
        self.mono_note = None;
    }

    pub fn choke(&mut self, token: u64) {
        for voice in &mut self.voices {
            if voice.token == token {
                voice.fading = true;
                voice.gate = false;
                voice.gate_sum = 0.0;
            }
            if voice.pending.is_some_and(|pending| pending.token == token) {
                voice.pending = None;
            }
        }
    }

    /// Note identities must be unique for simultaneous MIDI lifetimes. Ordinary
    /// note changes never clear another voice's phase or natural T60 tail.
    pub fn note_on(&mut self, note: i32, token: u64, velocity: f32) {
        if !(FIRST_NOTE..=LAST_NOTE).contains(&note) || !velocity.is_finite() {
            return;
        }
        if velocity <= 0.0 {
            self.note_off(token);
            return;
        }
        self.note_order = self.note_order.wrapping_add(1);
        let incoming = PendingVoice {
            note,
            token,
            velocity: velocity.min(1.0),
            gate: true,
            gate_sum: 0.0,
            order: self.note_order,
        };
        if let Some(voice) = self.voices[..self.voice_limit]
            .iter_mut()
            .find(|voice| voice.note.is_none())
        {
            *voice = Voice::start(incoming);
            return;
        }
        // Prefer the quietest released tail. Only steal a held voice if every
        // available one is held, in which case the oldest held note gives way.
        let released = self.voices[..self.voice_limit]
            .iter()
            .enumerate()
            .filter(|(_, voice)| !voice.fading && !voice.gate)
            .min_by(|a, b| a.1.energy.total_cmp(&b.1.energy))
            .map(|(index, _)| index);
        let oldest = || {
            self.voices[..self.voice_limit]
                .iter()
                .enumerate()
                .filter(|(_, voice)| !voice.fading)
                .max_by_key(|(_, voice)| self.note_order.wrapping_sub(voice.order))
                .map(|(index, _)| index)
        };
        let index = released.or_else(oldest).unwrap_or_else(|| {
            // All slots are already retiring: latest-note priority replaces the
            // oldest waiting note, without restarting or cutting a playing fade.
            self.voices[..self.voice_limit]
                .iter()
                .enumerate()
                .max_by_key(|(_, voice)| {
                    self.note_order
                        .wrapping_sub(voice.pending.map_or(voice.order, |pending| pending.order))
                })
                .unwrap()
                .0
        });
        let voice = &mut self.voices[index];
        voice.fading = true;
        voice.gate = false;
        voice.gate_sum = 0.0;
        voice.pending = Some(incoming);
    }

    /// The preallocated pool stays intact. Lowering the limit retires excess
    /// slots through the existing fade, and cancels their queued replacements.
    pub fn set_voice_limit(&mut self, limit: usize) {
        let limit = limit.clamp(1, MAX_VOICES);
        if limit == self.voice_limit {
            return;
        }
        self.voice_limit = limit;
        for voice in &mut self.voices[limit..] {
            voice.fading = true;
            voice.gate = false;
            voice.gate_sum = 0.0;
            voice.pending = None;
        }
    }

    pub fn note_off(&mut self, token: u64) {
        for voice in &mut self.voices {
            if voice.token == token {
                voice.gate = false;
            }
            if let Some(pending) = &mut voice.pending
                && pending.token == token
            {
                pending.gate = false;
            }
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|voice| voice.note.is_some())
            .count()
    }

    pub fn process_frame<'a, I>(&mut self, samples: I, controls: ResonatorControls)
    where
        I: IntoIterator<Item = &'a mut f32>,
        I::IntoIter: ExactSizeIterator,
    {
        let desired = controls
            .note
            .filter(|note| (FIRST_NOTE..=LAST_NOTE).contains(note))
            .map(|note| (note, controls.note_token));
        if desired != self.mono_note {
            if let Some((_, token)) = self.mono_note {
                self.note_off(token);
            }
            if let Some((note, token)) = desired {
                self.note_on(note, token, controls.velocity);
            }
            self.mono_note = desired;
        }
        self.process_poly_frame(samples, controls);
    }

    /// Sample-timed events update each voice's gate independently; several
    /// notes inside one hop each retain their own accumulated excitation.
    pub fn process_poly_frame<'a, I>(&mut self, samples: I, controls: ResonatorControls)
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
        let wet = std::array::from_fn(|index| {
            self.channels
                .get(index)
                .map_or(0.0, |channel| channel.wet_output[self.position])
        });
        // Before overwriting the input ring, this slot contains x[n - N]. Its
        // timing matches the dry reference without another buffer or latency.
        let reference = std::array::from_fn(|index| {
            self.channels
                .get(index)
                .map_or(0.0, |channel| channel.input[self.position])
        });
        let wet = self.transient.process(wet, reference, controls.transient);
        let original_wet = self.post_unison.process(wet);
        let wet = self.wet_delay.process(original_wet, controls.align_wet);
        self.last_wet = wet;
        for (index, (channel, sample)) in self.channels.iter_mut().zip(samples).enumerate() {
            channel.input[self.position] = if sample.is_finite() { *sample } else { 0.0 };
            self.last_dry[index] = channel.output[self.position];
            *sample = self.last_dry[index] + wet[index];
            channel.output[self.position] = 0.0;
            channel.wet_output[self.position] = 0.0;
        }
        for voice in &mut self.voices {
            if voice.gate {
                voice.gate_sum += voice.velocity;
            }
            if let Some(pending) = &mut voice.pending
                && pending.gate
            {
                pending.gate_sum += pending.velocity;
            }
        }
        self.position = (self.position + 1) % self.size;
        if self.position.is_multiple_of(self.hop) {
            self.render_hop(controls);
        }
    }

    fn render_hop(&mut self, controls: ResonatorControls) {
        #[cfg(feature = "profiling")]
        let mut stage = std::time::Instant::now();
        let input_gain = bounded(controls.input_gain, 1.0, 0.0, 4.0);
        let fade_step = (self.hop as f32 / (0.02 * self.rate)).min(1.0);
        let band_targets = [
            if controls.mute_low { 0.0 } else { 1.0 },
            if controls.mute_high { 0.0 } else { 1.0 },
        ];
        // Start at the saved state without a startup leak, then fade changes
        // over 20 ms of hop updates. Existing Hann overlap smooths synthesis.
        let mut band_gains = self.outer_band_gains.unwrap_or(band_targets);
        for (gain, target) in band_gains.iter_mut().zip(band_targets) {
            *gain += (target - *gain).clamp(-fade_step, fade_step);
        }
        self.outer_band_gains = Some(band_gains);
        let spread = if self.channels.len() == 2 {
            bounded(controls.voice_spread, 0.0, 0.0, 1.0)
        } else {
            0.0
        };
        for voice in &mut self.voices {
            if voice.fading {
                voice.gain = (voice.gain - fade_step).max(0.0);
                if voice.gain == 0.0 {
                    *voice = voice.pending.map_or_else(Voice::default, Voice::start);
                }
            }
            if !voice.fading {
                voice.gain = (voice.gain + fade_step).min(1.0);
            }
            if voice.note.is_some() {
                let position = voice.order.wrapping_sub(1) as usize % VOICE_PAN_POSITIONS.len();
                let target_pan = VOICE_PAN_POSITIONS[position] * spread;
                voice.pan += (target_pan - voice.pan).clamp(-fade_step, fade_step);
                // Stereo balance preserves the independent L/R resonances.
                // Center is exactly unity, the favored side is never boosted,
                // and only the opposite wet channel is attenuated. Mono ignores
                // Spread; dry audio never passes through these gains.
                let opposite = (voice.pan.abs() * std::f32::consts::FRAC_PI_2)
                    .cos()
                    .max(0.0);
                voice.pan_gains = if voice.pan < 0.0 {
                    [1.0, opposite]
                } else {
                    [opposite, 1.0]
                };
            }
            // A waiting note may span more than one hop while a stolen voice
            // fades; clamp its first injection instead of releasing a gain burst.
            voice.drive = (voice.gate_sum / self.hop as f32).min(1.0) * input_gain;
            voice.gate_sum = 0.0;
            voice.energy = 0.0;
        }
        let t60 = bounded(
            controls.t60,
            2.0,
            crate::MIN_DECAY_SECONDS,
            crate::MAX_DECAY_SECONDS,
        );
        let spectral_attack = if controls.transient.is_some() {
            Some(0.0)
        } else {
            controls.attack_ms
        };
        let attack_target = if spectral_attack.is_some() { 1.0 } else { 0.0 };
        self.attack_mix += (attack_target - self.attack_mix).clamp(-fade_step, fade_step);
        if let Some(attack_ms) = spectral_attack {
            let attack_ms = bounded(attack_ms, 10.0, 0.0, 2000.0);
            self.attack_radius = if attack_ms == 0.0 {
                0.0
            } else {
                10.0_f32.powf(-(self.hop as f32) * 1000.0 / (self.rate * attack_ms))
            };
        }
        // Retain the last pole during the short transition back to Natural.
        // Changing the hidden Attack value cannot interrupt that transition.
        let hf = bounded(controls.hf_damp, 0.25, 0.0, 1.0);
        let lf = bounded(controls.lf_damp, 0.0, 0.0, 1.0);
        if self.damping_controls != [t60, hf, lf] {
            self.damping_controls = [t60, hf, lf];
            for (h, radius) in self.radii.iter_mut().enumerate() {
                let decay = crate::damping_seconds((h + 1) as f32, t60, hf, lf);
                *radius = 10.0_f32.powf(-3.0 * self.hop as f32 / (self.rate * decay));
            }
        }
        self.crossover.set_points(controls.low_hz, controls.high_hz);
        if let Some(curve) = controls.decay_curve.map(DecayCurve::prepared)
            && self.decay_curve != Some(curve)
        {
            self.decay_curve = Some(curve);
            self.curve_revision = self.curve_revision.wrapping_add(1).max(1);
        }
        let target_mix = if controls.decay_curve.is_some() {
            1.0
        } else {
            0.0
        };
        self.curve_mix += (target_mix - self.curve_mix).clamp(-fade_step, fade_step);
        if self.curve_mix > 0.0
            && let Some(curve) = self.decay_curve
        {
            for voice in &mut self.voices {
                if let Some(note) = voice.note
                    && voice.curve_revision != self.curve_revision
                {
                    let f0 = crate::partial::frequency(note) as f32;
                    let count = self.templates[(note - FIRST_NOTE) as usize].partials.len();
                    for (h, radius) in voice.curve_radii.iter_mut().take(count).enumerate() {
                        let seconds = curve.seconds_at(f0 * (h + 1) as f32);
                        *radius = 10.0_f32.powf(-3.0 * self.hop as f32 / (self.rate * seconds));
                    }
                    voice.curve_revision = self.curve_revision;
                }
            }
        }
        let wet_level = bounded(controls.wet_level, 0.0, 0.0, 16.0);
        let mid_mix = bounded(controls.mid_mix, 1.0, 0.0, 1.0);
        let harmonics = controls.harmonics.clamp(1, MAX_PARTIALS);
        let mut modulation = controls.modulation.sanitized();
        self.post_unison.set_controls(
            controls.unison_mode,
            modulation.unison_voices,
            modulation.unison_detune_cents,
        );
        let guard_target = if controls.unison_mode == UnisonMode::Post
            && modulation.unison_voices > 1
            && modulation.unison_detune_cents > 0.0
        {
            1.0
        } else {
            0.0
        };
        self.post_guard += (guard_target - self.post_guard).clamp(-fade_step, fade_step);
        if controls.unison_mode == UnisonMode::Post {
            // Keep Chorus/Wander/Granular on the original partials, but synthesize
            // one copy. The summed wet audio receives the detuned copies later.
            modulation.unison_voices = 1;
            modulation.unison_detune_cents = 0.0;
        }
        #[cfg(feature = "profiling")]
        {
            self.profile[0] += stage.elapsed();
            stage = std::time::Instant::now();
        }
        for (voice, bank) in self.voices.iter_mut().zip(&mut self.modulation_banks) {
            if let Some(note) = voice.note {
                let count = self.templates[(note - FIRST_NOTE) as usize].partials.len();
                // Keep preparing retiring harmonics until this voice ends, so
                // reducing Harmonics cannot abruptly change their modulation.
                voice.rendered_partials = voice.rendered_partials.max(harmonics.min(count));
                bank.prepare(
                    voice.order,
                    crate::partial::frequency(note) as f32,
                    voice.rendered_partials,
                    modulation,
                    &self.modulation_kernel,
                    self.hop,
                );
            } else {
                bank.reset();
            }
        }
        let retiring_radius = 10.0_f32.powf(-3.0 * self.hop as f32 / (self.rate * 0.02));
        #[cfg(feature = "profiling")]
        {
            self.profile[1] += stage.elapsed();
        }
        for (channel_index, channel) in self.channels.iter_mut().enumerate() {
            #[cfg(feature = "profiling")]
            {
                stage = std::time::Instant::now();
            }
            for (i, sample) in self.input_spectrum.iter_mut().enumerate() {
                *sample = Complex32::new(channel.input[(self.position + i) % self.size], 0.0);
            }
            self.forward
                .process_with_scratch(&mut self.input_spectrum, &mut self.scratch);
            self.wet_spectrum.fill(ZERO);
            #[cfg(feature = "profiling")]
            {
                self.profile[2] += stage.elapsed();
            }
            for (voice, bank) in self.voices.iter_mut().zip(&self.modulation_banks) {
                let Some(note) = voice.note else {
                    continue;
                };
                let template = &self.templates[(note - FIRST_NOTE) as usize];
                #[cfg(feature = "profiling")]
                {
                    stage = std::time::Instant::now();
                }
                for (h, partial) in template.partials.iter().enumerate() {
                    let state = &mut voice.state[channel_index][h];
                    let mut radius = if h < harmonics {
                        if self.curve_mix == 1.0 {
                            // Avoid cancellation when a 5 ms curve radius is
                            // orders of magnitude below the damping radius.
                            voice.curve_radii[h]
                        } else {
                            self.radii[h] + self.curve_mix * (voice.curve_radii[h] - self.radii[h])
                        }
                    } else {
                        retiring_radius
                    };
                    let excitation = if !voice.fading && h < harmonics && voice.drive > 0.0 {
                        let projected = partial.project(&self.input_spectrum);
                        let drive = voice.drive * partial.level;
                        // A normalized resonator normally shares one pole for
                        // buildup and release. On a rising spectral envelope,
                        // select an independent attack pole; on falling input
                        // (including note-off), retain the original T60 pole.
                        // This responds to repeated audio transients as well as
                        // MIDI notes. Both terms use the same radius, keeping a
                        // bounded convex blend instead of boosting excitation
                        // into a long feedback tail. L/R decide independently.
                        // Natural skips this nonlinear shaping completely.
                        if self.attack_mix > 0.0
                            && projected.norm_sqr() * drive * drive > state.norm_sqr()
                        {
                            radius += self.attack_mix * (self.attack_radius - radius);
                        }
                        projected * (drive * (1.0 - radius))
                    } else {
                        ZERO
                    };
                    let next = *state * partial.rotation * radius + excitation;
                    *state =
                        if next.re.is_finite() && next.im.is_finite() && next.norm_sqr() > 1e-40 {
                            next
                        } else {
                            ZERO
                        };
                    if *state != ZERO {
                        voice.energy += state.norm_sqr();
                    }
                }
                #[cfg(feature = "profiling")]
                {
                    self.profile[3] += stage.elapsed();
                    stage = std::time::Instant::now();
                }
                // Keep accumulation in the original partial/voice order. The
                // separate pass permits coarse offline timing without a clock
                // call per partial, and reads the same updated complex states.
                for (h, partial) in template.partials.iter().enumerate() {
                    let state = voice.state[channel_index][h];
                    if state != ZERO {
                        let output = state * (voice.gain * voice.pan_gains[channel_index]);
                        if bank.blend() < 1.0 {
                            partial.synthesize_positive(
                                output * (1.0 - bank.blend()),
                                &mut self.wet_spectrum,
                            );
                        }
                        if bank.blend() > 0.0 {
                            bank.synthesize(
                                h,
                                output,
                                &self.modulation_kernel,
                                &mut self.wet_spectrum,
                            );
                        }
                    }
                }
                #[cfg(feature = "profiling")]
                {
                    self.profile[4] += stage.elapsed();
                }
            }
            #[cfg(feature = "profiling")]
            {
                stage = std::time::Instant::now();
            }
            crate::partial::complete_conjugate_spectrum(&mut self.wet_spectrum);
            #[cfg(feature = "profiling")]
            {
                self.profile[4] += stage.elapsed();
                stage = std::time::Instant::now();
            }
            // Hann multiplication in time is a three-bin circular convolution.
            // Both dry and wet now contain exactly one Hann window, so the same
            // real crossover mask and constant OLA normalization apply to both.
            let weights = self.crossover.weights();
            let outer_weights = self.crossover.outer_weights();
            for k in 0..self.size {
                let dry = self.input_spectrum[k] * 0.5
                    - (self.input_spectrum[(k + self.size - 1) % self.size]
                        + self.input_spectrum[(k + 1) % self.size])
                        * 0.25;
                // Crossfade only the middle-band replacement. At mix=0 the
                // complementary dry regions sum back to full-band dry; mix=1
                // preserves the previous routing. Keep evolving wet states even
                // at zero mix so bringing the effect back does not reset tails.
                let middle = weights[k.min(self.size - k)] * mid_mix;
                let hz = k.min(self.size - k) as f32 * self.rate / self.size as f32;
                // At maximum +50 cents, 0.47*fs shifts to <0.484*fs. Fade the
                // post input above 0.45*fs so the resampler cannot fold its
                // highest partials across Nyquist. Spectral mode keeps its
                // existing per-partial protection and original bandwidth.
                let protection = ((self.rate * 0.47 - hz) / (self.rate * 0.02)).clamp(0.0, 1.0);
                let wet = self.wet_spectrum[k]
                    * (middle * wet_level * (1.0 - self.post_guard * (1.0 - protection)));
                let dry_gain = if band_gains == [1.0; 2] {
                    // Exact legacy path when both switches are off.
                    1.0 - middle
                } else {
                    let [low, high] = outer_weights[k.min(self.size - k)];
                    // Sum retained regions instead of subtracting: muting
                    // both outer bands at 100% wet makes dry exactly zero.
                    weights[k.min(self.size - k)] * (1.0 - mid_mix)
                        + low * band_gains[0]
                        + high * band_gains[1]
                };
                let dry = dry * dry_gain;
                // Both spectra are Hermitian. Packing D+iW into one complex
                // IFFT gives dry in its real part and wet in its imaginary
                // part, keeping the same FFT count while separating routing.
                self.output_spectrum[k] = Complex32::new(dry.re - wet.im, dry.im + wet.re);
            }
            self.inverse
                .process_with_scratch(&mut self.output_spectrum, &mut self.scratch);
            // Sum of overlapping periodic Hann windows is N/(2H). RustFFT's
            // inverse adds another factor N; neither path gets a second window.
            let scale = 2.0 * self.hop as f32 / (self.size * self.size) as f32;
            for (i, value) in self.output_spectrum.iter().enumerate() {
                let sample = value.re * scale;
                if sample.is_finite() {
                    channel.output[(self.position + i) % self.size] += sample;
                }
                let wet = value.im * scale;
                if mid_mix > 0.0 && wet_level > 0.0 && wet.is_finite() {
                    channel.wet_output[(self.position + i) % self.size] += wet;
                }
            }
            #[cfg(feature = "profiling")]
            {
                self.profile[5] += stage.elapsed();
            }
        }
        // Released voices below -160 dB of state energy no longer occupy a
        // slot. Held silent notes remain available to receive later audio.
        for voice in &mut self.voices {
            if voice.note.is_some() && !voice.gate && !voice.fading && voice.energy < 1e-16 {
                *voice = Voice::default();
            }
        }
    }
}

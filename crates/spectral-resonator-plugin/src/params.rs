use nice_plug::prelude::*;
use spectral_dsp::{DECAY_POINTS, DEFAULT_DECAY_HZ, DEFAULT_PARTIALS, MAX_PARTIALS, MAX_UNISON};
use std::sync::atomic::AtomicBool;

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    #[id = "mixed"]
    Mixed,
    #[id = "dry"]
    #[name = "Dry contribution"]
    Dry,
    #[id = "wet"]
    #[name = "Wet only (post Unison)"]
    Wet,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnisonMode {
    #[id = "spectral"]
    Spectral,
    #[id = "post"]
    #[name = "Post (Audio)"]
    Post,
}

impl From<UnisonMode> for spectral_dsp::UnisonMode {
    fn from(mode: UnisonMode) -> Self {
        match mode {
            UnisonMode::Spectral => Self::Spectral,
            UnisonMode::Post => Self::Post,
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackMode {
    #[id = "natural"]
    Natural,
    #[id = "independent"]
    Independent,
    #[id = "reshape"]
    #[name = "Reshape wet"]
    Reshape,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecayMode {
    #[id = "damping"]
    Damping,
    #[id = "curve"]
    Curve,
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulationMode {
    #[id = "off"]
    Off,
    #[id = "chorus"]
    Chorus,
    #[id = "wander"]
    Wander,
    #[id = "granular"]
    Granular,
}

impl From<ModulationMode> for spectral_dsp::ModulationMode {
    fn from(mode: ModulationMode) -> Self {
        match mode {
            ModulationMode::Off => Self::Off,
            ModulationMode::Chorus => Self::Chorus,
            ModulationMode::Wander => Self::Wander,
            ModulationMode::Granular => Self::Granular,
        }
    }
}

#[derive(Params)]
pub struct DecayPointParams {
    #[id = "decay_point_hz"]
    pub hz: FloatParam,
    #[id = "decay_point_seconds"]
    pub seconds: FloatParam,
}

impl DecayPointParams {
    fn new(index: usize) -> Self {
        Self {
            hz: FloatParam::new(
                format!("Decay Point {} Frequency", index + 1),
                DEFAULT_DECAY_HZ[index],
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.5),
                },
            )
            .with_unit(" Hz")
            // Stable display precision makes host text/value round trips
            // insensitive to the last bit of the nonlinear range conversion.
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),
            seconds: FloatParam::new(
                format!("Decay Point {} T60", index + 1),
                2.0,
                FloatRange::Skewed {
                    min: spectral_dsp::MIN_DECAY_SECONDS,
                    max: spectral_dsp::MAX_DECAY_SECONDS,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(3))
            .with_smoother(SmoothingStyle::Logarithmic(50.0)),
        }
    }
}

#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PitchSource {
    #[id = "internal"]
    Internal,
    #[id = "midi"]
    Midi,
}

#[derive(Params)]
pub struct SpectralResonatorParams {
    #[id = "fft_size"]
    pub fft_size: EnumParam<crate::fft::FftSize>,
    // Editor preferences are preset fields, never automatable audio controls.
    #[persist = "ui_debug_fps"]
    pub ui_debug_fps: AtomicBool,
    #[persist = "ui_show_dry"]
    pub ui_show_dry: AtomicBool,
    #[persist = "ui_show_wet"]
    pub ui_show_wet: AtomicBool,
    #[id = "max_polyphony"]
    pub max_polyphony: IntParam,
    #[id = "output_mode"]
    pub output_mode: EnumParam<OutputMode>,
    /// Stable ID keeps host automation and saved projects attached to this control.
    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "root_note"]
    pub root_note: IntParam,

    #[id = "pitch_source"]
    pub pitch_source: EnumParam<PitchSource>,

    #[id = "harmonics"]
    pub harmonics: IntParam,

    #[id = "hf_damp"]
    pub hf_damp: FloatParam,

    #[id = "lf_damp"]
    pub lf_damp: FloatParam,

    #[id = "input_send_db"]
    pub input_send_db: FloatParam,

    #[id = "panic"]
    pub panic: BoolParam,

    #[id = "decay_t60"]
    pub decay_t60: FloatParam,

    #[id = "attack_mode"]
    pub attack_mode: EnumParam<AttackMode>,
    #[id = "attack_ms"]
    pub attack_ms: FloatParam,
    #[id = "attack_emphasis_db"]
    pub attack_emphasis_db: FloatParam,

    #[id = "decay_mode"]
    pub decay_mode: EnumParam<DecayMode>,
    #[nested(array, group = "Decay Curve")]
    pub decay_points: [DecayPointParams; DECAY_POINTS],

    #[id = "mod_mode"]
    pub mod_mode: EnumParam<ModulationMode>,
    #[id = "mod_rate_hz"]
    pub mod_rate_hz: FloatParam,
    #[id = "mod_amount"]
    pub mod_amount: FloatParam,
    #[id = "mod_pitch_semitones"]
    pub mod_pitch_semitones: FloatParam,
    #[id = "grain_ms"]
    pub grain_ms: FloatParam,
    #[id = "unison_voices"]
    pub unison_voices: IntParam,
    #[id = "unison_mode"]
    pub unison_mode: EnumParam<UnisonMode>,
    #[id = "unison_detune_cents"]
    pub unison_detune_cents: FloatParam,

    #[id = "voice_spread"]
    pub voice_spread: FloatParam,

    /// Wet gain is independent of the middle band's dry/wet blend.
    #[id = "m2_wet_level"]
    pub m2_wet_level: FloatParam,

    #[id = "align_wet"]
    pub align_wet: FloatParam,

    #[id = "mid_mix"]
    pub mid_mix: FloatParam,

    #[id = "low_mid_hz"]
    pub low_mid_hz: FloatParam,

    #[id = "mid_high_hz"]
    pub mid_high_hz: FloatParam,
    #[id = "mute_low"]
    pub mute_low: BoolParam,
    #[id = "mute_high"]
    pub mute_high: BoolParam,
}

impl Default for SpectralResonatorParams {
    fn default() -> Self {
        Self {
            fft_size: EnumParam::new("FFT Size", crate::fft::FftSize::N4096).non_automatable(),
            ui_debug_fps: AtomicBool::new(false),
            ui_show_dry: AtomicBool::new(true),
            ui_show_wet: AtomicBool::new(true),
            max_polyphony: IntParam::new(
                "Maximum Polyphony",
                16,
                IntRange::Linear { min: 1, max: 16 },
            ),
            output_mode: EnumParam::new("Main Output", OutputMode::Mixed),
            mute_low: BoolParam::new("Low Mute", false),
            mute_high: BoolParam::new("High Mute", false),
            output_gain: FloatParam::new(
                "Output Gain",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
            root_note: IntParam::new("Root Note", 57, IntRange::Linear { min: 33, max: 93 }),
            pitch_source: EnumParam::new("Pitch Source", PitchSource::Internal),
            harmonics: IntParam::new(
                "Harmonics",
                DEFAULT_PARTIALS as i32,
                IntRange::Linear {
                    min: 1,
                    max: MAX_PARTIALS as i32,
                },
            ),
            hf_damp: FloatParam::new(
                "High Damping",
                0.25,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            lf_damp: FloatParam::new(
                "Low Damping",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            input_send_db: FloatParam::new(
                "Excitation",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(10.0)),
            panic: BoolParam::new("Panic (hold to mute wet)", false),
            decay_t60: FloatParam::new(
                "Decay T60",
                2.0,
                FloatRange::Linear {
                    min: spectral_dsp::MIN_DECAY_SECONDS,
                    max: spectral_dsp::MAX_DECAY_SECONDS,
                },
            )
            .with_unit(" s")
            .with_value_to_string(formatters::v2s_f32_rounded(3))
            .with_smoother(SmoothingStyle::Linear(50.0)),
            attack_mode: EnumParam::new("Attack Response", AttackMode::Natural),
            attack_ms: FloatParam::new(
                "Attack (90%)",
                10.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 2000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            attack_emphasis_db: FloatParam::new(
                "Transient Emphasis",
                6.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_smoother(SmoothingStyle::Linear(20.0)),
            decay_mode: EnumParam::new("Decay Mode", DecayMode::Damping),
            decay_points: std::array::from_fn(DecayPointParams::new),
            mod_mode: EnumParam::new("Modulation Mode", ModulationMode::Off),
            mod_rate_hz: FloatParam::new(
                "Mod Rate",
                0.5,
                FloatRange::Linear {
                    min: 0.0,
                    max: 10.0,
                },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            mod_amount: FloatParam::new(
                "Mod Amount",
                50.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            mod_pitch_semitones: FloatParam::new(
                "Pitch Mod",
                0.1,
                FloatRange::Linear {
                    min: 0.0,
                    max: 12.0,
                },
            )
            .with_unit(" st")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            grain_ms: FloatParam::new(
                "Grain Decay",
                80.0,
                FloatRange::Linear {
                    min: 5.0,
                    max: 500.0,
                },
            )
            .with_unit(" ms")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            unison_voices: IntParam::new(
                "Unison",
                1,
                IntRange::Linear {
                    min: 1,
                    max: MAX_UNISON as i32,
                },
            ),
            unison_mode: EnumParam::new("Unison Mode", UnisonMode::Spectral),
            unison_detune_cents: FloatParam::new(
                "Unison Detune",
                7.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 50.0,
                },
            )
            .with_unit(" ct")
            .with_smoother(SmoothingStyle::Linear(30.0)),
            voice_spread: FloatParam::new(
                "Voice Spread",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_smoother(SmoothingStyle::Linear(30.0)),
            m2_wet_level: FloatParam::new(
                "Wet Level",
                4.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 16.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
            // DSP crossfades fixed delay taps; a parameter smoother would keep
            // retargeting the delay instead of selecting the requested sample.
            align_wet: FloatParam::new(
                "Wet Alignment",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" windows"),
            mid_mix: FloatParam::new(
                "Mid Mix",
                100.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_smoother(SmoothingStyle::Linear(20.0)),
            low_mid_hz: FloatParam::new(
                "Low / Mid",
                250.0,
                FloatRange::Linear {
                    min: 80.0,
                    max: 800.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            mid_high_hz: FloatParam::new(
                "Mid / High",
                4_000.0,
                FloatRange::Linear {
                    min: 1_600.0,
                    max: 12_000.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
        }
    }
}

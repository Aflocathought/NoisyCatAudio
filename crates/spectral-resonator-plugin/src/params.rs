use nice_plug::prelude::*;

#[derive(Params)]
pub struct SpectralResonatorParams {
    /// Stable ID keeps host automation and saved projects attached to this control.
    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "root_note"]
    pub root_note: IntParam,

    #[id = "decay_t60"]
    pub decay_t60: FloatParam,

    /// Temporary M2 research level; M3 will use the documented crossover.
    #[id = "m2_wet_level"]
    pub m2_wet_level: FloatParam,
}

impl Default for SpectralResonatorParams {
    fn default() -> Self {
        Self {
            output_gain: FloatParam::new(
                "Output Gain",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
            root_note: IntParam::new("Root Note", 57, IntRange::Linear { min: 33, max: 93 }),
            decay_t60: FloatParam::new(
                "Decay T60",
                2.0,
                FloatRange::Linear {
                    min: 0.05,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),
            m2_wet_level: FloatParam::new(
                "M2 Wet Level",
                1.0,
                FloatRange::Linear { min: 0.0, max: 4.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

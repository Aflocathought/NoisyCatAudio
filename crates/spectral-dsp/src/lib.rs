#![forbid(unsafe_code)]

mod bin_resonator;
mod crossover;
mod decay;
mod modulation;
mod partial;
mod resonator;
mod stft;
mod transient;
mod wet_delay;
pub use transient::TransientControls;

pub use decay::{
    DECAY_POINTS, DEFAULT_DECAY_HZ, DecayCurve, DecayPoint, MAX_DECAY_SECONDS, MIN_DECAY_SECONDS,
    damping_seconds,
};
pub use modulation::{MAX_UNISON, ModulationControls, ModulationMode};
pub use partial::{DEFAULT_PARTIALS, MAX_PARTIALS};
mod post_unison;
pub use post_unison::UnisonMode;
pub use resonator::{MAX_VOICES, ResonatorControls, SpectralResonator};
pub use stft::{StreamingStft, TransparentStft};

/// Applies one gain value to a sample frame while preserving channel alignment.
///
/// The plugin advances its smoothed automation value once per frame and passes
/// the same gain to every channel, so stereo balance remains unchanged.
#[inline]
pub fn apply_frame_gain<'a>(frame: impl IntoIterator<Item = &'a mut f32>, gain: f32) {
    for sample in frame {
        *sample *= gain;
    }
}

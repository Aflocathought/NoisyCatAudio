#![forbid(unsafe_code)]

mod bin_resonator;
mod stft;

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

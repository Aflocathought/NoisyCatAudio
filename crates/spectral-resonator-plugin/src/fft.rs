//! FFT selection is a saved, non-automatable parameter. Only host activation
//! builds the engine; the GUI/audio callback can request, never perform, a restart.
use nice_plug::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

#[derive(Enum, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FftSize {
    #[id = "1024"]
    #[name = "1024"]
    N1024,
    #[id = "2048"]
    #[name = "2048"]
    N2048,
    #[id = "3072"]
    #[name = "3072"]
    N3072,
    #[default]
    #[id = "4096"]
    #[name = "4096"]
    N4096,
}

impl FftSize {
    pub const ALL: [Self; 4] = [Self::N1024, Self::N2048, Self::N3072, Self::N4096];

    pub const fn samples(self) -> usize {
        match self {
            Self::N1024 => 1024,
            Self::N2048 => 2048,
            Self::N3072 => 3072,
            Self::N4096 => 4096,
        }
    }

    pub const fn hop(self) -> usize {
        // Four overlaps avoid the larger synthesis ripple of 1024/512.
        // Larger windows keep the existing update rate and partial CPU cost.
        match self {
            Self::N1024 => 256,
            _ => 512,
        }
    }
}

#[derive(Default)]
pub struct FftStatus {
    samples: AtomicU32,
    rate: AtomicU32,
    restart_pending: AtomicBool,
}

impl FftStatus {
    pub fn activate(&self, size: FftSize, rate: f32) {
        self.rate.store(rate.to_bits(), Ordering::Relaxed);
        self.samples.store(size.samples() as u32, Ordering::Release);
        self.restart_pending.store(false, Ordering::Release);
    }

    pub fn deactivate(&self) {
        self.samples.store(0, Ordering::Release);
        self.restart_pending.store(false, Ordering::Release);
    }

    pub fn samples(&self) -> u32 {
        self.samples.load(Ordering::Acquire)
    }

    pub fn rate(&self) -> f32 {
        f32::from_bits(self.rate.load(Ordering::Relaxed))
    }

    pub fn request_restart(&self, selected: FftSize) -> bool {
        let active = self.samples();
        // GUI parameter edits are queued by nice-plug. Call only after reading
        // the committed parameter value, never immediately after queuing it.
        // Deduplicate GUI and audio requests until the host actually reactivates.
        active != 0
            && active != selected.samples() as u32
            && !self.restart_pending.swap(true, Ordering::AcqRel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_changes_are_coalesced_until_actual_activation() {
        let status = FftStatus::default();
        assert!(!status.request_restart(FftSize::N1024));
        status.activate(FftSize::N4096, 48_000.0);
        assert!(!status.request_restart(FftSize::N4096));
        assert!(status.request_restart(FftSize::N2048));
        assert!(!status.request_restart(FftSize::N2048));
        assert!(!status.request_restart(FftSize::N3072));
        assert_eq!(status.samples(), 4096);
        status.activate(FftSize::N3072, 96_000.0);
        assert_eq!(status.rate(), 96_000.0);
        assert!(!status.request_restart(FftSize::N3072));
        assert!(status.request_restart(FftSize::N1024));
        status.deactivate();
        assert!(!status.request_restart(FftSize::N1024));
    }
}

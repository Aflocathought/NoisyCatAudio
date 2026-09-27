//! Bounded audio-to-editor transport. No analysis, locks, allocation or GPU
//! work occurs in the audio callback. A full queue drops visual data only.
use crossbeam_queue::ArrayQueue;
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const BLOCK: usize = 256;
pub const BINS: usize = 512;
pub const HISTORY: usize = 128;
pub const ROWS_PER_SECOND: f32 = 30.0;
pub const HISTORY_SECONDS: f32 = HISTORY as f32 / ROWS_PER_SECOND;
// Audio alignment is independent of display resolution. Increasing the bass
// analysis window must never delay the dry tap or change the audio engine.
const AUDIO_DELAY: usize = 4096;
const FAST_SIZE: usize = 4096;
const BASS_SIZE: usize = 16384;

#[derive(Clone, Copy)]
pub struct AudioPacket {
    pub samples: [[f32; 4]; BLOCK],
    pub start: u64,
    pub epoch: u64,
    pub rate: f32,
}

pub struct SharedAnalysis {
    pub enabled: AtomicBool,
    pub queue: ArrayQueue<AudioPacket>,
}

impl SharedAnalysis {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(false),
            queue: ArrayQueue::new(32),
        })
    }
}

pub struct AudioCapture {
    pub shared: Arc<SharedAnalysis>,
    delay: Box<[[f32; 2]]>,
    packet: AudioPacket,
    position: usize,
    count: usize,
    clock: u64,
}

impl AudioCapture {
    pub fn new(shared: Arc<SharedAnalysis>) -> Self {
        Self {
            shared,
            delay: vec![[0.0; 2]; AUDIO_DELAY].into_boxed_slice(),
            packet: AudioPacket {
                samples: [[0.0; 4]; BLOCK],
                start: 0,
                epoch: 0,
                rate: 48000.0,
            },
            position: 0,
            count: 0,
            clock: 0,
        }
    }

    pub fn reset(&mut self, rate: f32) {
        self.delay.fill([0.0; 2]);
        self.position = 0;
        self.count = 0;
        self.clock = 0;
        self.packet.epoch = self.packet.epoch.wrapping_add(1);
        self.packet.rate = rate;
    }

    pub fn push(&mut self, input: [f32; 2], wet: [f32; 2], enabled: bool) {
        let dry = self.delay[self.position];
        self.delay[self.position] = input.map(|v| if v.is_finite() { v } else { 0.0 });
        self.position = (self.position + 1) % AUDIO_DELAY;
        if enabled {
            if self.count == 0 {
                self.packet.start = self.clock;
            }
            self.packet.samples[self.count] = [dry[0], dry[1], wet[0], wet[1]];
            self.count += 1;
            if self.count == BLOCK {
                // Never wait for the editor. Timestamps let it discard windows
                // that span dropped samples or an audio-engine reset.
                let _ = self.shared.queue.push(self.packet);
                self.count = 0;
            }
        } else {
            self.count = 0;
        }
        self.clock = self.clock.wrapping_add(1);
    }

    pub fn enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
    }
}

/// UI-owned FFT buffers. Stereo powers are averaged, not waveforms, so an
/// antiphase input remains visible. Display analysis never feeds back into DSP.
pub struct Analyzer {
    fast: SpectrumWindow,
    bass: SpectrumWindow,
    bass_weight: [f32; BINS],
    ring: Vec<[f32; 4]>,
    position: usize,
    filled: usize,
    since_row: usize,
    expected: Option<(u64, u64, u32)>,
    pub rate: f32,
}

struct SpectrumWindow {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex32>,
    data: Vec<Complex32>,
    window: Vec<f32>,
    projection: [Projection; BINS],
}

#[derive(Clone, Copy)]
enum Projection {
    Interpolate { first: usize, fraction: f32 },
    Peak { first: usize, end: usize },
}

impl SpectrumWindow {
    fn new(size: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        Self {
            scratch: vec![Complex32::default(); fft.get_inplace_scratch_len()],
            fft,
            data: vec![Complex32::default(); size],
            window: (0..size)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
                .collect(),
            projection: [Projection::Peak { first: 1, end: 2 }; BINS],
        }
    }

    fn set_rate(&mut self, rate: f32) {
        let size = self.data.len();
        self.projection = std::array::from_fn(|x| {
            let lo = frequency(x as f32 / BINS as f32, rate) * size as f32 / rate;
            let hi = frequency((x + 1) as f32 / BINS as f32, rate) * size as f32 / rate;
            if hi - lo < 1.0 {
                // Narrow log-frequency pixels sample the spectrum at their
                // centre, instead of smearing a neighbouring peak over them.
                let centre = (lo * hi).sqrt().clamp(1.0, (size / 2) as f32);
                Projection::Interpolate {
                    first: (centre.floor() as usize).min(size / 2 - 1),
                    fraction: centre - centre.floor().min((size / 2 - 1) as f32),
                }
            } else {
                let first = (lo.ceil() as usize).clamp(1, size / 2);
                Projection::Peak {
                    first,
                    end: (hi.floor() as usize + 1).clamp(first + 1, size / 2 + 1),
                }
            }
        });
    }

    fn spectrum(&mut self, ring: &[[f32; 4]], position: usize) -> [[f32; BINS]; 2] {
        let size = self.data.len();
        let start = (position + ring.len() - size) % ring.len();
        let mut result = [[0.0; BINS]; 2];
        for channel in 0..4 {
            for i in 0..size {
                self.data[i] = Complex32::new(
                    ring[(start + i) % ring.len()][channel] * self.window[i],
                    0.0,
                );
            }
            self.fft
                .process_with_scratch(&mut self.data, &mut self.scratch);
            for (x, output) in result[channel / 2].iter_mut().enumerate() {
                let power = match self.projection[x] {
                    Projection::Interpolate { first, fraction } => {
                        let a = self.data[first].norm_sqr();
                        a + (self.data[first + 1].norm_sqr() - a) * fraction
                    }
                    Projection::Peak { first, end } => self.data[first..end]
                        .iter()
                        .map(|c| c.norm_sqr())
                        .fold(0.0_f32, f32::max),
                };
                *output += power * 8.0 / (size * size) as f32;
            }
        }
        result
    }
}

impl Analyzer {
    pub fn new() -> Self {
        let mut analyzer = Self {
            fast: SpectrumWindow::new(FAST_SIZE),
            bass: SpectrumWindow::new(BASS_SIZE),
            bass_weight: [0.0; BINS],
            ring: vec![[0.0; 4]; BASS_SIZE],
            position: 0,
            filled: 0,
            since_row: 0,
            expected: None,
            rate: 48000.0,
        };
        analyzer.set_rate(48000.0);
        analyzer
    }

    fn set_rate(&mut self, rate: f32) {
        self.rate = rate;
        self.fast.set_rate(rate);
        self.bass.set_rate(rate);
        self.bass_weight = std::array::from_fn(|x| {
            let hz = frequency((x as f32 + 0.5) / BINS as f32, rate);
            // Full bass detail through 400 Hz, smoothly joined to the faster
            // window by 1 kHz. Both windows end at the same audio sample.
            (1.0 - (hz / 400.0).ln() / (1000.0_f32 / 400.0).ln()).clamp(0.0, 1.0)
        });
    }

    pub fn clear(&mut self) {
        self.position = 0;
        self.filled = 0;
        self.since_row = 0;
        self.expected = None;
    }

    pub fn ingest(&mut self, packet: AudioPacket, mut row: impl FnMut([[f32; BINS]; 2], bool)) {
        let discontinuity =
            self.expected != Some((packet.start, packet.epoch, packet.rate.to_bits()));
        if discontinuity {
            self.clear();
        }
        if self.rate != packet.rate {
            self.set_rate(packet.rate);
        }
        self.expected = Some((
            packet.start + BLOCK as u64,
            packet.epoch,
            packet.rate.to_bits(),
        ));
        if discontinuity {
            row([[0.0; BINS]; 2], true);
        }
        let stride = (self.rate / ROWS_PER_SECOND).round().max(1.0) as usize;
        for sample in packet.samples {
            self.ring[self.position] = sample;
            self.position = (self.position + 1) % BASS_SIZE;
            self.filled = (self.filled + 1).min(BASS_SIZE);
            self.since_row += 1;
            if self.filled >= FAST_SIZE && self.since_row >= stride {
                self.since_row = 0;
                row(self.spectrum(), false);
            }
        }
    }

    fn spectrum(&mut self) -> [[f32; BINS]; 2] {
        let mut result = self.fast.spectrum(&self.ring, self.position);
        // Show audio promptly on opening/resuming the editor. Only use the
        // long window after it is fully filled with contiguous, current audio.
        if self.filled == BASS_SIZE {
            let bass = self.bass.spectrum(&self.ring, self.position);
            for channel in 0..2 {
                for x in 0..BINS {
                    result[channel][x] +=
                        (bass[channel][x] - result[channel][x]) * self.bass_weight[x];
                }
            }
        }
        result
    }
}

pub fn frequency(x: f32, rate: f32) -> f32 {
    20.0 * ((rate * 0.5).min(20000.0) / 20.0).powf(x.clamp(0.0, 1.0))
}
pub fn fraction(hz: f32, rate: f32) -> f32 {
    (hz / 20.0).ln() / ((rate * 0.5).min(20000.0) / 20.0).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bass_separates_nearby_tones_without_smearing_high_frequency_transients() {
        for rate in [44100.0, 48000.0, 96000.0] {
            let mut analyzer = Analyzer::new();
            let scale = rate / 48000.0;
            let mut last = [[0.0; BINS]; 2];
            // At 48 kHz, 80/92 Hz cannot be separated by the old 11.72 Hz
            // bins. Include a high tone to check the short window's release.
            for b in 0..(BASS_SIZE + 8192) / BLOCK {
                analyzer.ingest(
                    AudioPacket {
                        start: (b * BLOCK) as u64,
                        epoch: 0,
                        rate,
                        samples: std::array::from_fn(|i| {
                            let n = b * BLOCK + i;
                            let phase = std::f32::consts::TAU * n as f32 / rate;
                            let bass = (80.0 * scale * phase).sin() + (92.0 * scale * phase).sin();
                            let high = if n < BASS_SIZE {
                                (3000.0 * phase).sin()
                            } else {
                                0.0
                            };
                            [bass, -bass, high, high]
                        }),
                    },
                    |s, gap| {
                        if !gap {
                            last = s;
                        }
                    },
                );
            }
            let power_near = |hz: f32| {
                let x = (fraction(hz, rate) * BINS as f32) as usize;
                last[0][x.saturating_sub(1)..=x + 1]
                    .iter()
                    .copied()
                    .fold(0.0_f32, f32::max)
            };
            let a = power_near(80.0 * scale);
            let b = power_near(92.0 * scale);
            let valley = power_near(86.0 * scale);
            assert!(a > 0.6 && b > 0.6, "tone peaks missing at {rate}: {a}, {b}");
            assert!(
                valley < a.min(b) * 0.3,
                "bass tones merged at {rate}: {a}, {valley}, {b}"
            );
            let high = (fraction(3000.0, rate) * BINS as f32) as usize;
            assert_eq!(
                last[1][high], 0.0,
                "high band must not retain the long window's tail"
            );
        }
    }

    #[test]
    fn history_scrolls_twice_as_fast_without_doubling_analysis_cadence() {
        let mut analyzer = Analyzer::new();
        let mut rows = 0;
        let mut gaps = 0;
        let frames = 4096 + 48000;
        for start in (0..frames).step_by(BLOCK) {
            analyzer.ingest(
                AudioPacket {
                    start: start as u64,
                    epoch: 0,
                    rate: 48000.0,
                    samples: [[0.0; 4]; BLOCK],
                },
                |_, gap| {
                    if gap {
                        gaps += 1;
                    } else {
                        rows += 1;
                    }
                },
            );
        }
        assert_eq!(gaps, 1);
        assert_eq!(rows, 31); // First complete short window, then 30 rows/s.
        assert!((HISTORY_SECONDS - 4.266667).abs() < 0.00001);
    }

    #[test]
    fn bounded_capture_aligns_dry_and_drops_when_full() {
        let shared = SharedAnalysis::new();
        let mut capture = AudioCapture::new(shared.clone());
        for i in 0..4096 + BLOCK {
            capture.push([if i == 0 { 1.0 } else { 0.0 }, 0.0], [0.25, -0.25], true);
        }
        let packets: Vec<_> = std::iter::from_fn(|| shared.queue.pop()).collect();
        assert_eq!(packets[4096 / BLOCK].samples[0], [1.0, 0.0, 0.25, -0.25]);
        for _ in 0..BLOCK * 100 {
            capture.push([0.0; 2], [0.0; 2], true);
        }
        assert_eq!(shared.queue.len(), 32);
    }
    #[test]
    fn antiphase_stereo_is_visible_and_gaps_reset_analysis() {
        let mut analyzer = Analyzer::new();
        let mut rows = Vec::new();
        for b in 0..80 {
            let packet = AudioPacket {
                start: b * BLOCK as u64,
                epoch: 0,
                rate: 48000.0,
                samples: std::array::from_fn(|i| {
                    let s = (std::f32::consts::TAU * 1000.0 * (b as usize * BLOCK + i) as f32
                        / 48000.0)
                        .sin();
                    [s, -s, 0.0, 0.0]
                }),
            };
            analyzer.ingest(packet, |s, gap| {
                if !gap {
                    rows.push(s);
                }
            });
        }
        let row = rows.last().unwrap();
        assert!(row[0].iter().copied().fold(0.0_f32, f32::max) > 0.7);
        assert!(row[1].iter().all(|&x| x == 0.0));
        let mut gaps = 0;
        analyzer.ingest(
            AudioPacket {
                start: 99999,
                epoch: 0,
                rate: 48000.0,
                samples: [[0.0; 4]; BLOCK],
            },
            |_, gap| {
                assert!(gap);
                gaps += 1;
            },
        );
        assert_eq!(gaps, 1);
        assert_eq!(analyzer.filled, BLOCK);
    }
}

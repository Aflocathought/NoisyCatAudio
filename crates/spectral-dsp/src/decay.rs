//! Absolute-frequency decay controls. Seconds are the user-facing quantity;
//! converting those times to a stable feedback radius remains a DSP detail.
pub const DECAY_POINTS: usize = 6;
pub const DEFAULT_DECAY_HZ: [f32; DECAY_POINTS] = [80.0, 250.0, 1000.0, 4000.0, 10_000.0, 20_000.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecayPoint {
    pub hz: f32,
    pub seconds: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecayCurve {
    pub points: [DecayPoint; DECAY_POINTS],
}

impl Default for DecayCurve {
    fn default() -> Self {
        Self {
            points: DEFAULT_DECAY_HZ.map(|hz| DecayPoint { hz, seconds: 2.0 }),
        }
    }
}

impl DecayCurve {
    pub(crate) fn prepared(mut self) -> Self {
        for (point, fallback) in self.points.iter_mut().zip(DEFAULT_DECAY_HZ) {
            point.hz = if point.hz.is_finite() {
                point.hz.clamp(20.0, 20_000.0)
            } else {
                fallback
            };
            point.seconds = if point.seconds.is_finite() {
                point.seconds.clamp(0.05, 12.0)
            } else {
                2.0
            };
        }
        // Six-point stable insertion sort needs no scratch allocation. Crossing
        // nodes keep their parameter identities; at identical Hz the later
        // node wins, so duplicate automation positions cannot divide by zero.
        for i in 1..DECAY_POINTS {
            let mut j = i;
            while j > 0 && self.points[j].hz < self.points[j - 1].hz {
                self.points.swap(j, j - 1);
                j -= 1;
            }
        }
        self
    }

    pub(crate) fn seconds_at(&self, hz: f32) -> f32 {
        let right = self.points.partition_point(|point| point.hz <= hz);
        if right == DECAY_POINTS {
            return self.points[DECAY_POINTS - 1].seconds;
        }
        // Use the winning duplicate on both sides of a segment, including
        // below the lowest node. Otherwise approaching a duplicate from below
        // would interpolate toward a different time than the exact anchor.
        let upper = self
            .points
            .partition_point(|point| point.hz <= self.points[right].hz)
            - 1;
        if right == 0 {
            return self.points[upper].seconds;
        }
        let a = self.points[right - 1];
        let b = self.points[upper];
        let fraction = (hz / a.hz).log2() / (b.hz / a.hz).log2();
        // Log frequency and log seconds give an octave-consistent curve with
        // positive times and no overshoot between user-specified anchors.
        a.seconds * (b.seconds / a.seconds).powf(fraction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_interpolates_absolute_hz_with_safe_crossing_and_duplicate_nodes() {
        let mut curve = DecayCurve::default();
        curve.points[1] = DecayPoint {
            hz: 250.0,
            seconds: 4.0,
        };
        curve.points[2] = DecayPoint {
            hz: 1000.0,
            seconds: 1.0,
        };
        let curve = curve.prepared();
        assert_eq!(curve.seconds_at(250.0), 4.0);
        assert_eq!(curve.seconds_at(1000.0), 1.0);
        assert!((curve.seconds_at(500.0) - 2.0).abs() < 1e-6);
        let mut reversed = curve;
        reversed.points.reverse();
        assert_eq!(reversed.prepared(), curve);
        let duplicated = DecayCurve {
            points: std::array::from_fn(|i| DecayPoint {
                hz: 1000.0,
                seconds: i as f32 + 1.0,
            }),
        }
        .prepared();
        assert_eq!(duplicated.seconds_at(1000.0), 6.0);
        assert_eq!(duplicated.seconds_at(500.0), 6.0);
        assert_eq!(duplicated.seconds_at(2000.0), 6.0);
        let mut duplicate_upper = duplicated;
        duplicate_upper.points[0] = DecayPoint {
            hz: 250.0,
            seconds: 1.5,
        };
        assert!((duplicate_upper.seconds_at(500.0) - 3.0).abs() < 1e-6);
        let invalid = DecayCurve {
            points: [DecayPoint {
                hz: f32::NAN,
                seconds: f32::INFINITY,
            }; DECAY_POINTS],
        }
        .prepared();
        assert_eq!(invalid, DecayCurve::default());
    }
}

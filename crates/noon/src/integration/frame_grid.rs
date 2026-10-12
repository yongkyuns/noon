//! Output-frame coordinates, not another scene clock or animation scheduler.

use std::{error::Error, fmt};

/// A positive, reduced rational number of output frames per second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, FrameGridError> {
        if numerator == 0 || denominator == 0 {
            return Err(FrameGridError::InvalidRate);
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Seconds per integer output PTS tick, as `(numerator, denominator)`.
    pub const fn time_base(self) -> (u32, u32) {
        (self.denominator, self.numerator)
    }

    fn seconds_at(self, index: u64) -> f64 {
        // A u64 index times a u32 denominator fits in u128. Divide before
        // conversion so a large intermediate floating product cannot lose
        // the rational remainder unnecessarily.
        let ticks = u128::from(index) * u128::from(self.denominator);
        let divisor = u128::from(self.numerator);
        (ticks / divisor) as f64 + (ticks % divisor) as f64 / f64::from(self.numerator)
    }
}

/// A requested output sample. Its integer PTS is independent of render latency.
///
/// This is a request, not proof that the runtime has reached `authored_time`.
/// Compare it with the coherent observation returned by `ForwardSample`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSample {
    index: u64,
    authored_time: f64,
    rate: FrameRate,
}

impl FrameSample {
    pub const fn index(self) -> u64 {
        self.index
    }

    pub const fn authored_time(self) -> f64 {
        self.authored_time
    }

    pub const fn pts(self) -> u64 {
        self.index
    }

    pub const fn time_base(self) -> (u32, u32) {
        self.rate.time_base()
    }
}

/// An immutable output grid: `origin + index * rate.denominator / rate.numerator`.
///
/// Keep one grid across every play/wait/continuation in an export. No elapsed
/// wall time, mutable cursor, duration rounding, or accumulated `t += dt` is used.
/// A nonzero origin does not seek or replay a stateful scene for the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameGrid {
    rate: FrameRate,
    origin: f64,
}

impl FrameGrid {
    pub fn new(rate: FrameRate, origin: f64) -> Result<Self, FrameGridError> {
        if !origin.is_finite() || origin < 0.0 {
            return Err(FrameGridError::InvalidOrigin);
        }
        let grid = Self { rate, origin };
        if grid.time_at(1) <= origin {
            return Err(FrameGridError::UnrepresentableTime { index: 1 });
        }
        Ok(grid)
    }

    pub const fn rate(self) -> FrameRate {
        self.rate
    }

    pub const fn origin(self) -> f64 {
        self.origin
    }

    fn time_at(self, index: u64) -> f64 {
        self.origin + self.rate.seconds_at(index)
    }

    /// Convert a single exact frame coordinate to the existing runtime's f64 API.
    ///
    /// Reject collapsed adjacent timestamps rather than claiming that two
    /// different frame indices refer to distinct representable scene times.
    pub fn sample(self, index: u64) -> Result<FrameSample, FrameGridError> {
        let time = self.time_at(index);
        if !time.is_finite() || (index > 0 && time <= self.time_at(index - 1)) {
            return Err(FrameGridError::UnrepresentableTime { index });
        }
        Ok(FrameSample {
            index,
            authored_time: time,
            rate: self.rate,
        })
    }

    /// Number of samples in `[origin, end)`, or an explicit caller-supplied cap error.
    ///
    /// Endpoints are runtime f64 values. Compare them with the same converted
    /// sample times used for execution, without an arbitrary epsilon or a
    /// floating `ceil(duration * fps)`. At a converted grid point the endpoint
    /// is excluded; an immediately greater representable endpoint includes it.
    ///
    /// O(log(max_frames + 1)) work and O(1) storage; no frame list is allocated.
    /// Each subsequently requested sample still checks f64 representability.
    pub fn frame_count_before(self, end: f64, max_frames: u64) -> Result<u64, FrameGridError> {
        if !end.is_finite() || end < self.origin {
            return Err(FrameGridError::InvalidEnd);
        }
        if self.time_at(max_frames) < end {
            return Err(FrameGridError::FrameLimitExceeded { max_frames });
        }
        let (mut low, mut high) = (0, max_frames);
        while low < high {
            let middle = low + (high - low) / 2;
            if self.time_at(middle) < end {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        Ok(low)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameGridError {
    InvalidRate,
    InvalidOrigin,
    InvalidEnd,
    FrameLimitExceeded { max_frames: u64 },
    UnrepresentableTime { index: u64 },
}

impl fmt::Display for FrameGridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRate => write!(f, "frame-rate numerator and denominator must be positive"),
            Self::InvalidOrigin => write!(f, "frame-grid origin must be finite and nonnegative"),
            Self::InvalidEnd => write!(
                f,
                "frame-grid end must be finite and not precede its origin"
            ),
            Self::FrameLimitExceeded { max_frames } => {
                write!(f, "export interval exceeds the {max_frames}-frame limit")
            }
            Self::UnrepresentableTime { index } => {
                write!(
                    f,
                    "frame {index} cannot be represented as a distinct runtime timestamp"
                )
            }
        }
    }
}

impl Error for FrameGridError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(p: u32, q: u32) -> FrameGrid {
        FrameGrid::new(FrameRate::new(p, q).unwrap(), 0.0).unwrap()
    }

    #[test]
    fn rates_are_positive_and_reduced() {
        assert_eq!(FrameRate::new(0, 1), Err(FrameGridError::InvalidRate));
        assert_eq!(FrameRate::new(1, 0), Err(FrameGridError::InvalidRate));
        let rate = FrameRate::new(120_000, 2_002).unwrap();
        assert_eq!((rate.numerator(), rate.denominator()), (60_000, 1_001));
        assert_eq!(rate.time_base(), (1_001, 60_000));
    }

    #[test]
    fn ten_seconds_is_600_frames_not_601() {
        let grid = grid(60, 1);
        assert_eq!(grid.frame_count_before(10.0, 600), Ok(600));
        assert_eq!(grid.sample(0).unwrap().authored_time(), 0.0);
        assert_eq!(grid.sample(599).unwrap().authored_time(), 599.0 / 60.0);
        assert_eq!(grid.sample(600).unwrap().authored_time(), 10.0);
        assert_eq!(grid.sample(599).unwrap().pts(), 599);
    }

    #[test]
    fn fractional_rates_retain_exact_output_coordinates() {
        for p in [30_000, 60_000] {
            let grid = grid(p, 1_001);
            let sample = grid.sample(u64::from(p)).unwrap();
            assert_eq!(sample.authored_time(), 1_001.0);
            assert_eq!(sample.time_base(), (1_001, p));
            assert_eq!(
                grid.frame_count_before(1_001.0, u64::from(p)),
                Ok(u64::from(p))
            );
        }
    }

    #[test]
    fn exclusive_end_uses_the_runtime_grid_without_epsilon() {
        let grid = grid(10, 1);
        let boundary = grid.sample(3).unwrap().authored_time();
        assert_eq!(grid.frame_count_before(boundary, 100), Ok(3));
        assert_eq!(grid.frame_count_before(boundary.next_down(), 100), Ok(3));
        assert_eq!(grid.frame_count_before(boundary.next_up(), 100), Ok(4));
    }

    #[test]
    fn empty_short_and_fractional_intervals() {
        let grid = grid(30, 1);
        assert_eq!(grid.frame_count_before(0.0, 0), Ok(0));
        assert_eq!(grid.frame_count_before(0.001, 1), Ok(1));
        assert_eq!(grid.frame_count_before(0.312, 100), Ok(10));
    }

    #[test]
    fn frame_caps_are_errors_not_truncated_success() {
        let grid = grid(60, 1);
        assert_eq!(
            grid.frame_count_before(10.0, 599),
            Err(FrameGridError::FrameLimitExceeded { max_frames: 599 })
        );
        assert_eq!(
            grid.frame_count_before(1.0, 0),
            Err(FrameGridError::FrameLimitExceeded { max_frames: 0 })
        );
    }

    #[test]
    fn invalid_times_are_rejected() {
        let rate = FrameRate::new(60, 1).unwrap();
        for origin in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                FrameGrid::new(rate, origin),
                Err(FrameGridError::InvalidOrigin)
            );
        }
        let grid = FrameGrid::new(rate, 2.0).unwrap();
        for end in [1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                grid.frame_count_before(end, 100),
                Err(FrameGridError::InvalidEnd)
            );
        }
        assert_eq!(grid.sample(60).unwrap().authored_time(), 3.0);
    }

    #[test]
    fn large_indices_use_wide_integer_products_without_frame_allocation() {
        let grid = grid(1, u32::MAX);
        let index = (1_u64 << 32) + 1;
        // (2^32 + 1) * (2^32 - 1) = 2^64 - 1, rounded only at f64 boundary.
        assert_eq!(grid.sample(index).unwrap().authored_time(), u64::MAX as f64);
        assert_eq!(
            grid.frame_count_before(grid.sample(index).unwrap().authored_time(), index),
            Ok(index)
        );
    }

    #[test]
    fn collapsed_runtime_samples_are_rejected() {
        let rate = FrameRate::new(60, 1).unwrap();
        assert!(matches!(
            FrameGrid::new(rate, 1.0e30),
            Err(FrameGridError::UnrepresentableTime { .. })
        ));
        assert!(matches!(
            grid(60, 1).sample(u64::MAX),
            Err(FrameGridError::UnrepresentableTime { .. })
        ));
    }

    #[test]
    fn counting_matches_enumeration_across_origins_and_rates() {
        for (p, q) in [(24, 1), (60, 1), (30_000, 1_001)] {
            for origin in [0.0, 0.1, 17.125] {
                let grid = FrameGrid::new(FrameRate::new(p, q).unwrap(), origin).unwrap();
                for step in 0..200 {
                    let end = origin + f64::from(step) / 137.0;
                    let expected = (0..200)
                        .take_while(|&i| grid.sample(i).unwrap().authored_time() < end)
                        .count() as u64;
                    assert_eq!(grid.frame_count_before(end, 200), Ok(expected));
                }
            }
        }
    }
}

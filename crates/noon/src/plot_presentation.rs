//! Preparation-only tick labels and timestamp-aware plot data.
//!
//! These values capture one coordinate snapshot; they are not a scene, a clock,
//! or live coordinate handles. Render labels as ordinary Text, and feed interval
//! durations into ordinary animation compositions. No per-frame resampling is
//! required. Re-prepare after moving the axes to use their new coordinate frame.

use crate::{AxesFrame, CoordinateError, NumberLineFrame};

pub const MAX_PLOT_LABELS: usize = 1_000;
pub const MAX_TIMED_PLOT_SAMPLES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlotPresentationError {
    Coordinate(CoordinateError),
    InvalidInput(&'static str),
    AllocationFailed,
}

impl std::fmt::Display for PlotPresentationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinate(error) => error.fmt(f),
            Self::InvalidInput(reason) => f.write_str(reason),
            Self::AllocationFailed => f.write_str("plot presentation allocation failed"),
        }
    }
}

impl std::error::Error for PlotPresentationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Coordinate(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CoordinateError> for PlotPresentationError {
    fn from(error: CoordinateError) -> Self {
        Self::Coordinate(error)
    }
}

/// A numeric label's immutable text and world-space anchor. Font shaping and
/// bounding-box placement still use the normal shared Text/layout operations.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberLabel {
    pub number: f64,
    pub text: String,
    pub point: [f64; 2],
}

/// Prepare every label before any caller allocates Text or semantic identities.
/// Explicit values retain input order; None selects the shared tick planner.
/// Precision is deliberately explicit (0..=12), not inferred from float strings.
pub fn number_labels(
    frame: NumberLineFrame,
    numbers: Option<&[f64]>,
    decimal_places: u32,
    exclude_zero: bool,
) -> Result<Vec<NumberLabel>, PlotPresentationError> {
    if decimal_places > 12 {
        return Err(PlotPresentationError::InvalidInput(
            "number-label decimal_places must be between 0 and 12",
        ));
    }
    let generated;
    let numbers = match numbers {
        Some(numbers) => numbers,
        None => {
            generated = noon_geometry::number_line_tick_values(
                frame.range(), false, exclude_zero, MAX_PLOT_LABELS,
            )?;
            &generated
        }
    };
    if numbers.len() > MAX_PLOT_LABELS {
        return Err(PlotPresentationError::InvalidInput("number-label limit exceeded"));
    }
    let mut labels = Vec::new();
    labels.try_reserve_exact(numbers.len())
        .map_err(|_| PlotPresentationError::AllocationFailed)?;
    for &number in numbers {
        let point = frame.number_to_point(number)?;
        if exclude_zero && number == 0.0 {
            continue;
        }
        let mut text = format!("{:.*}", decimal_places as usize, number);
        // Round first, then remove a negative sign only when all digits are zero.
        if text.starts_with('-') && text[1..].chars().all(|c| c == '0' || c == '.') {
            text.remove(0);
        }
        labels.push(NumberLabel { number, text, point });
    }
    Ok(labels)
}

/// A finite (data timestamp, scalar measurement). Timestamps must increase
/// strictly: duplicate timestamps do not define a unique interpolating segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimedPlotSample {
    pub time: f64,
    pub value: f64,
}

/// Immutable preparation for an ordinary piecewise-linear plot animation.
/// Key times are measured from playback start; unequal data intervals retain
/// their relative duration. This is authoring intent, never runtime state.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeSeriesPlan {
    samples: Vec<TimedPlotSample>,
    points: Vec<[f64; 2]>,
    cursor_points: Vec<[f64; 2]>,
    key_times: Vec<f64>,
    durations: Vec<f64>,
}

impl TimeSeriesPlan {
    pub fn new(
        frame: AxesFrame,
        samples: &[TimedPlotSample],
        run_time: f64,
    ) -> Result<Self, PlotPresentationError> {
        if samples.len() < 2 || samples.len() > MAX_TIMED_PLOT_SAMPLES {
            return Err(PlotPresentationError::InvalidInput(
                "time-series plotting requires 2..=10000 samples",
            ));
        }
        if !run_time.is_finite() || run_time <= 0.0 {
            return Err(PlotPresentationError::InvalidInput(
                "time-series run_time must be finite and positive",
            ));
        }
        if samples.iter().any(|s| !s.time.is_finite() || !s.value.is_finite())
            || samples.windows(2).any(|s| s[1].time <= s[0].time)
        {
            return Err(PlotPresentationError::InvalidInput(
                "time-series samples must be finite with strictly increasing timestamps",
            ));
        }
        let first = samples[0].time;
        let span = samples[samples.len() - 1].time - first;
        if !span.is_finite() {
            return Err(PlotPresentationError::InvalidInput("time-series timestamp span overflow"));
        }
        let y_range = frame.y().range();
        let y_mid = y_range[0] + (y_range[1] - y_range[0]) * 0.5;
        let mut result = Self {
            samples: Vec::new(), points: Vec::new(), cursor_points: Vec::new(),
            key_times: Vec::new(), durations: Vec::new(),
        };
        result.samples.try_reserve_exact(samples.len()).map_err(allocation_error)?;
        result.points.try_reserve_exact(samples.len()).map_err(allocation_error)?;
        result.cursor_points.try_reserve_exact(samples.len()).map_err(allocation_error)?;
        result.key_times.try_reserve_exact(samples.len()).map_err(allocation_error)?;
        result.durations.try_reserve_exact(samples.len() - 1).map_err(allocation_error)?;
        for (index, sample) in samples.iter().enumerate() {
            let elapsed = if index + 1 == samples.len() {
                run_time
            } else {
                ((sample.time - first) / span) * run_time
            };
            if let Some(&previous) = result.key_times.last() {
                let duration = elapsed - previous;
                if !duration.is_finite() || duration <= 0.0 {
                    return Err(PlotPresentationError::InvalidInput(
                        "time-series interval is not representable at the requested run_time",
                    ));
                }
                result.durations.push(duration);
            }
            result.samples.push(*sample);
            result.points.push(frame.coords_to_point(sample.time, sample.value)?);
            result.cursor_points.push(frame.coords_to_point(sample.time, y_mid)?);
            result.key_times.push(elapsed);
        }
        Ok(result)
    }

    pub fn samples(&self) -> &[TimedPlotSample] { &self.samples }
    pub fn points(&self) -> &[[f64; 2]] { &self.points }
    pub fn cursor_points(&self) -> &[[f64; 2]] { &self.cursor_points }
    pub fn key_times(&self) -> &[f64] { &self.key_times }
    pub fn durations(&self) -> &[f64] { &self.durations }
    pub fn run_time(&self) -> f64 { self.key_times[self.key_times.len() - 1] }

    /// An O(log N) diagnostic/reference observation, with no retained cursor.
    /// Examples do not call this per frame: ordinary linear compositions drive
    /// each interval. Out-of-range times fail instead of silently clamping.
    pub fn point_at(&self, elapsed: f64) -> Result<[f64; 2], PlotPresentationError> {
        if !elapsed.is_finite() || elapsed < 0.0 || elapsed > self.run_time() {
            return Err(PlotPresentationError::InvalidInput("time-series sample time is out of range"));
        }
        let index = self.key_times.partition_point(|&t| t <= elapsed)
            .saturating_sub(1).min(self.durations.len() - 1);
        let alpha = (elapsed - self.key_times[index]) / self.durations[index];
        Ok(std::array::from_fn(|axis| {
            (1.0 - alpha) * self.points[index][axis] + alpha * self.points[index + 1][axis]
        }))
    }
}

fn allocation_error(_: std::collections::TryReserveError) -> PlotPresentationError {
    PlotPresentationError::AllocationFailed
}

#[cfg(test)]
mod tests;

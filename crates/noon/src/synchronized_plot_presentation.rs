//! Bounded, preparation-only synchronization of piecewise-linear recordings.
//!
//! Split at the union of supplied timestamps inside an explicit common window.
//! Every recording must cover that window: there is no extrapolation, missing-
//! data fill, smoothing, sample-index timing, or mutable playback clock here.
//! Ordinary animation compositions consume the resulting shared intervals.

use crate::plot_presentation::{
    PlotPresentationError as Error, TimeSeriesPlan, TimedPlotSample, MAX_TIMED_PLOT_SAMPLES,
};
use crate::AxesFrame;

pub const MAX_SYNCHRONIZED_SERIES: usize = 16;
/// Bound the expanded series-by-union-grid product, not just each input series.
pub const MAX_SYNCHRONIZED_POINTS: usize = 10_000;

#[derive(Clone, Debug, PartialEq)]
pub struct SynchronizedTimeSeriesPlan {
    series: Vec<TimeSeriesPlan>,
}

impl SynchronizedTimeSeriesPlan {
    /// Capture one coordinate frame and one data-time window for all recordings.
    /// Input and expanded output budgets are checked before their allocations.
    /// Preparation is O(N log N + S*K): N input points, S series, K union times.
    /// No semantic identity, resource, or runtime state is created.
    pub fn new(
        frame: AxesFrame,
        series: &[&[TimedPlotSample]],
        time_range: [f64; 2],
        run_time: f64,
    ) -> Result<Self, Error> {
        let [start, end] = time_range;
        if !start.is_finite()
            || !end.is_finite()
            || start >= end
            || !(end - start).is_finite()
        {
            return Err(Error::InvalidInput("invalid synchronized data-time window"));
        }
        if series.is_empty() || series.len() > MAX_SYNCHRONIZED_SERIES {
            return Err(Error::InvalidInput("synchronized plots require 1..=16 series"));
        }
        let count = series.iter().try_fold(0usize, |count, samples| {
            count.checked_add(samples.len()).ok_or(Error::InvalidInput(
                "synchronized input sample limit exceeded",
            ))
        })?;
        if count > MAX_TIMED_PLOT_SAMPLES {
            return Err(Error::InvalidInput("synchronized input sample limit exceeded"));
        }
        // Reuse the single-series admission contract rather than a second
        // validator. Temporary preparation is bounded by the input budget and
        // dropped before preparing the common-grid output. Entire source
        // recordings, including samples outside the window, must be valid.
        for samples in series {
            TimeSeriesPlan::new(frame, samples, run_time)?;
            if samples[0].time > start || samples[samples.len() - 1].time < end {
                return Err(Error::InvalidInput(
                    "every series must cover the complete data-time window",
                ));
            }
        }
        let mut times = Vec::new();
        times.try_reserve_exact(count + 2).map_err(allocation_error)?;
        times.extend([start, end]);
        for samples in series {
            times.extend(samples.iter().map(|s| s.time).filter(|&t| t > start && t < end));
        }
        times.sort_unstable_by(f64::total_cmp);
        // Exact equality only. Nearby but distinct timestamps are not silently
        // coalesced; unrepresentable playback intervals fail in TimeSeriesPlan.
        times.dedup_by(|a, b| *a == *b);
        if times.len() > MAX_SYNCHRONIZED_POINTS / series.len() {
            return Err(Error::InvalidInput("synchronized expanded point limit exceeded"));
        }
        let mut result = Self { series: Vec::new() };
        result.series.try_reserve_exact(series.len()).map_err(allocation_error)?;
        let mut aligned = Vec::new();
        aligned.try_reserve_exact(times.len()).map_err(allocation_error)?;
        for samples in series {
            aligned.clear();
            let mut left = 0;
            for &time in &times {
                while left + 1 < samples.len() && samples[left + 1].time <= time {
                    left += 1;
                }
                let a = samples[left];
                let value = if a.time == time {
                    a.value // Preserve authored values exactly at their knots.
                } else {
                    let b = samples[left + 1]; // Coverage was validated above.
                    let alpha = (time - a.time) / (b.time - a.time);
                    (1.0 - alpha) * a.value + alpha * b.value
                };
                aligned.push(TimedPlotSample { time, value });
            }
            result.series.push(TimeSeriesPlan::new(frame, &aligned, run_time)?);
        }
        Ok(result)
    }

    /// Each row has the same key times. Values are linearly interpolated only
    /// where another recording contributes a knot (or at a window boundary).
    pub fn series(&self) -> &[TimeSeriesPlan] {
        &self.series
    }
    pub fn key_times(&self) -> &[f64] {
        self.series[0].key_times()
    }
    pub fn durations(&self) -> &[f64] {
        self.series[0].durations()
    }
    pub fn cursor_points(&self) -> &[[f64; 2]] {
        self.series[0].cursor_points()
    }
    pub fn run_time(&self) -> f64 {
        self.series[0].run_time()
    }
}

fn allocation_error(_: std::collections::TryReserveError) -> Error {
    Error::AllocationFailed
}

#[cfg(test)]
mod tests;

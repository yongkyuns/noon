//! Explicit disconnections on the existing synchronized sampling grid.
//!
//! A break after source index i removes the open interval (t[i], t[i+1]).
//! Both measured endpoints remain known. Missing grid points and segments are
//! Option::None, never NaNs, held values, or publishable interpolated geometry.
//! These are immutable preparation values, not another playback state machine.

use super::{allocation_error, SynchronizedTimeSeriesPlan};
use crate::plot_presentation::{PlotPresentationError as Error, TimedPlotSample};
use crate::AxesFrame;

pub type PlotSegment = [[f64; 2]; 2];

#[derive(Clone, Debug, PartialEq)]
pub struct GappedSeries {
    points: Vec<Option<[f64; 2]>>,
    segments: Vec<Option<PlotSegment>>,
}

impl GappedSeries {
    /// Known samples at the common knots. An isolated measured endpoint remains
    /// Some even when both adjacent intervals are disconnected.
    pub fn points(&self) -> &[Option<[f64; 2]>] {
        &self.points
    }

    /// One optional drawable segment per shared playback interval. Do not infer
    /// connectivity from two known endpoints: a break can lie between them.
    pub fn segments(&self) -> &[Option<PlotSegment>] {
        &self.segments
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GappedTimeSeriesPlan {
    series: Vec<GappedSeries>,
    data_times: Vec<f64>,
    key_times: Vec<f64>,
    durations: Vec<f64>,
    cursor_points: Vec<[f64; 2]>,
}

impl GappedTimeSeriesPlan {
    /// One strictly increasing list of break-after indices per input recording.
    /// Empty lists mean fully connected recordings. Endpoints still must cover
    /// the selected window; explicit gaps do not authorize extrapolation.
    ///
    /// Reuses the bounded common-grid preparation. Its temporary dense values
    /// are masked before return and dropped, not retained as hidden measurements.
    /// O(N log N + S*K) work and O(S*K) storage, under the same aggregate and
    /// expanded-output budgets as SynchronizedTimeSeriesPlan.
    pub fn new(
        frame: AxesFrame,
        series: &[&[TimedPlotSample]],
        break_after: &[&[usize]],
        time_range: [f64; 2],
        run_time: f64,
    ) -> Result<Self, Error> {
        if break_after.len() != series.len() {
            return Err(Error::InvalidInput("one break list is required per series"));
        }
        for (samples, breaks) in series.iter().zip(break_after) {
            if breaks.len() > samples.len().saturating_sub(1)
                || breaks.iter().any(|&i| i >= samples.len().saturating_sub(1))
                || breaks.windows(2).any(|p| p[0] >= p[1])
            {
                return Err(Error::InvalidInput(
                    "break-after indices must be strictly increasing and identify source intervals",
                ));
            }
        }
        let dense = SynchronizedTimeSeriesPlan::new(frame, series, time_range, run_time)?;
        let mut result = Self {
            series: Vec::new(),
            data_times: Vec::new(),
            key_times: copy_slice(dense.key_times())?,
            durations: copy_slice(dense.durations())?,
            cursor_points: copy_slice(dense.cursor_points())?,
        };
        result.series.try_reserve_exact(series.len()).map_err(allocation_error)?;
        let grid = dense.series()[0].samples();
        result.data_times.try_reserve_exact(grid.len()).map_err(allocation_error)?;
        result.data_times.extend(grid.iter().map(|s| s.time));
        for ((source, breaks), aligned) in series.iter().zip(break_after).zip(dense.series()) {
            let mut row = GappedSeries { points: Vec::new(), segments: Vec::new() };
            row.points.try_reserve_exact(grid.len()).map_err(allocation_error)?;
            row.segments.try_reserve_exact(grid.len() - 1).map_err(allocation_error)?;
            let mut left = 0;
            let mut gap = 0;
            for (index, sample) in grid.iter().enumerate() {
                while left + 1 < source.len() && source[left + 1].time <= sample.time {
                    left += 1;
                }
                while gap < breaks.len() && breaks[gap] < left {
                    gap += 1;
                }
                let disconnected = gap < breaks.len() && breaks[gap] == left;
                row.points.push(if disconnected && sample.time != source[left].time {
                    None
                } else {
                    Some(aligned.points()[index])
                });
                if index + 1 < grid.len() {
                    row.segments.push(if disconnected {
                        None
                    } else {
                        Some([aligned.points()[index], aligned.points()[index + 1]])
                    });
                }
            }
            result.series.push(row);
        }
        Ok(result)
    }

    pub fn series(&self) -> &[GappedSeries] { &self.series }
    pub fn data_times(&self) -> &[f64] { &self.data_times }
    pub fn key_times(&self) -> &[f64] { &self.key_times }
    pub fn durations(&self) -> &[f64] { &self.durations }
    pub fn cursor_points(&self) -> &[[f64; 2]] { &self.cursor_points }
    pub fn run_time(&self) -> f64 { *self.key_times.last().expect("validated time grid") }
}

fn copy_slice<T: Clone>(source: &[T]) -> Result<Vec<T>, Error> {
    let mut result = Vec::new();
    result.try_reserve_exact(source.len()).map_err(allocation_error)?;
    result.extend_from_slice(source);
    Ok(result)
}

#[cfg(test)]
mod tests;

mod time_map;
pub use time_map::*;

use serde::{Deserialize, Serialize};

use crate::timeline::validate_track_definition;
use crate::{SceneDefinition, TimelineError, TrackId, TrackTiming};

/// One child's interval within a resolved animation composition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionInterval {
    pub start_time: f64,
    pub duration: f64,
}

impl CompositionInterval {
    pub const fn end_time(self) -> f64 {
        self.start_time + self.duration
    }
}

/// A deterministic composition schedule shared by every authoring frontend.
///
/// `intrinsic_run_time` is the virtual duration obtained from the child runtimes
/// and `lag_ratio`. `run_time` is the externally requested total duration after
/// optional rescaling. Child intervals are expressed in the final time scale.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionSchedule {
    pub intrinsic_run_time: f64,
    pub run_time: f64,
    pub intervals: Vec<CompositionInterval>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompositionError {
    Empty,
    InvalidLagRatio(f64),
    InvalidChildRunTime { index: usize, value: f64 },
    InvalidRunTime(f64),
}

impl std::fmt::Display for CompositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("animation composition requires at least one child"),
            Self::InvalidLagRatio(value) => write!(
                formatter,
                "lag_ratio must be finite and non-negative, got {value}"
            ),
            Self::InvalidChildRunTime { index, value } => write!(
                formatter,
                "child animation {index} run_time must be finite and non-negative, got {value}"
            ),
            Self::InvalidRunTime(value) => write!(
                formatter,
                "composition run_time must be finite and positive, got {value}"
            ),
        }
    }
}

impl std::error::Error for CompositionError {}

/// Resolve Manim-compatible animation composition timing.
///
/// Child starts follow Manim's timing geometry: every child after the first is
/// offset by `previous_child_run_time * lag_ratio`. The intrinsic composition
/// duration is the maximum child end, which is important when child runtimes are
/// unequal. If `run_time` is supplied, every interval is rescaled uniformly so
/// that the composition consumes that total duration while preserving relative
/// timing.
pub fn resolve_composition_schedule(
    child_run_times: &[f64],
    lag_ratio: f64,
    run_time: Option<f64>,
) -> Result<CompositionSchedule, CompositionError> {
    if child_run_times.is_empty() {
        return Err(CompositionError::Empty);
    }
    if !lag_ratio.is_finite() || lag_ratio < 0.0 {
        return Err(CompositionError::InvalidLagRatio(lag_ratio));
    }
    for (index, &child_run_time) in child_run_times.iter().enumerate() {
        if !child_run_time.is_finite() || child_run_time < 0.0 {
            return Err(CompositionError::InvalidChildRunTime {
                index,
                value: child_run_time,
            });
        }
    }
    if let Some(value) = run_time {
        if !value.is_finite() || value <= 0.0 {
            return Err(CompositionError::InvalidRunTime(value));
        }
    }

    let mut starts = Vec::with_capacity(child_run_times.len());
    let mut start = 0.0;
    for (index, &child_run_time) in child_run_times.iter().enumerate() {
        starts.push(start);
        if index + 1 < child_run_times.len() {
            start += child_run_time * lag_ratio;
        }
    }

    let intrinsic_run_time = starts
        .iter()
        .zip(child_run_times)
        .map(|(&child_start, &child_run_time)| child_start + child_run_time)
        .fold(0.0_f64, f64::max);
    let resolved_run_time = run_time.unwrap_or(intrinsic_run_time);
    let scale = if intrinsic_run_time > 0.0 {
        resolved_run_time / intrinsic_run_time
    } else {
        0.0
    };

    let intervals = starts
        .into_iter()
        .zip(child_run_times.iter().copied())
        .map(|(child_start, child_run_time)| CompositionInterval {
            start_time: child_start * scale,
            duration: child_run_time * scale,
        })
        .collect();

    Ok(CompositionSchedule {
        intrinsic_run_time,
        run_time: resolved_run_time,
        intervals,
    })
}

/// Convenience planner for family animations whose children have equal intrinsic
/// duration. This is the shared lowering used by grouped Create/fades and
/// `VGroup.animate(..., lag_ratio=...)`.
pub fn resolve_uniform_composition_schedule(
    child_count: usize,
    lag_ratio: f64,
    run_time: f64,
) -> Result<CompositionSchedule, CompositionError> {
    if child_count == 0 {
        return Err(CompositionError::Empty);
    }
    let child_run_times = vec![1.0; child_count];
    resolve_composition_schedule(&child_run_times, lag_ratio, Some(run_time))
}

impl SceneDefinition {
    /// Atomically retime one existing non-instant track and attach a validated
    /// root-to-leaf composition time map.
    ///
    /// This is an authoring-time lowering primitive. It preserves stable track
    /// identity and keeps the persisted scene as ordinary tracks rather than
    /// introducing a second composition graph into the runtime document.
    pub fn remap_track_for_composition(
        &mut self,
        id: TrackId,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
    ) -> Result<bool, TimelineError> {
        let Some(index) = self.tracks.iter().position(|track| track.id == id) else {
            return Ok(false);
        };
        let mut candidate = self.tracks[index].clone();
        candidate.timing = timing;
        candidate.time_map = time_map;
        validate_track_definition(&candidate)?;
        self.tracks[index] = candidate;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GeometryRef, Property, RateFunction, TrackTiming, TrackValues, Vec2};

    #[test]
    fn parallel_children_start_together_and_keep_longest_runtime() {
        let schedule = resolve_composition_schedule(&[2.0, 1.0, 3.0], 0.0, None).unwrap();
        assert_eq!(schedule.intrinsic_run_time, 3.0);
        assert_eq!(schedule.run_time, 3.0);
        assert_eq!(
            schedule.intervals,
            vec![
                CompositionInterval {
                    start_time: 0.0,
                    duration: 2.0,
                },
                CompositionInterval {
                    start_time: 0.0,
                    duration: 1.0,
                },
                CompositionInterval {
                    start_time: 0.0,
                    duration: 3.0,
                },
            ]
        );
    }

    #[test]
    fn lagged_schedule_uses_max_end_not_last_end() {
        let schedule = resolve_composition_schedule(&[10.0, 1.0], 0.1, None).unwrap();
        assert_eq!(schedule.intrinsic_run_time, 10.0);
        assert_eq!(schedule.intervals[0].end_time(), 10.0);
        assert_eq!(schedule.intervals[1].start_time, 1.0);
        assert_eq!(schedule.intervals[1].end_time(), 2.0);
    }

    #[test]
    fn explicit_runtime_uniformly_rescales_virtual_timing() {
        let schedule = resolve_composition_schedule(&[10.0, 1.0], 0.1, Some(5.0)).unwrap();
        assert_eq!(schedule.intrinsic_run_time, 10.0);
        assert_eq!(schedule.run_time, 5.0);
        assert_eq!(schedule.intervals[0].duration, 5.0);
        assert_eq!(schedule.intervals[1].start_time, 0.5);
        assert_eq!(schedule.intervals[1].duration, 0.5);
    }

    #[test]
    fn succession_is_lag_ratio_one() {
        let schedule = resolve_composition_schedule(&[2.0, 1.0, 3.0], 1.0, None).unwrap();
        let starts: Vec<_> = schedule
            .intervals
            .iter()
            .map(|interval| interval.start_time)
            .collect();
        assert_eq!(starts, vec![0.0, 2.0, 3.0]);
        assert_eq!(schedule.run_time, 6.0);
    }

    #[test]
    fn uniform_family_schedule_matches_equal_child_formula() {
        let schedule = resolve_uniform_composition_schedule(3, 0.5, 1.2).unwrap();
        assert!((schedule.intervals[0].duration - 0.6).abs() < 1e-12);
        assert!((schedule.intervals[1].start_time - 0.3).abs() < 1e-12);
        assert!((schedule.intervals[2].start_time - 0.6).abs() < 1e-12);
        assert!((schedule.run_time - 1.2).abs() < 1e-12);
    }

    #[test]
    fn invalid_inputs_fail_in_shared_semantics() {
        assert_eq!(
            resolve_composition_schedule(&[], 0.0, None),
            Err(CompositionError::Empty)
        );
        assert_eq!(
            resolve_composition_schedule(&[1.0], -0.1, None),
            Err(CompositionError::InvalidLagRatio(-0.1))
        );
        assert_eq!(
            resolve_composition_schedule(&[1.0, -1.0], 0.0, None),
            Err(CompositionError::InvalidChildRunTime {
                index: 1,
                value: -1.0,
            })
        );
        assert_eq!(
            resolve_composition_schedule(&[1.0], 0.0, Some(0.0)),
            Err(CompositionError::InvalidRunTime(0.0))
        );
    }

    #[test]
    fn remap_track_preserves_identity_and_validates_time_map() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let id = scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(0.5, 0.5, RateFunction::Linear),
            )
            .unwrap();
        let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Smooth,
        )]);

        assert!(scene
            .remap_track_for_composition(
                id,
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                map.clone(),
            )
            .unwrap());
        let track = &scene.tracks()[0];
        assert_eq!(track.id, id);
        assert_eq!(track.time_map, map);
        assert_eq!(track.property, Property::Position);
        assert_eq!(
            track.values,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            }
        );
    }
}

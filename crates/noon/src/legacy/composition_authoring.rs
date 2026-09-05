use super::{Animation, AuthoringError, IntoAnimations, Scene};
use noon_core::{
    resolve_composition_schedule, CompositionTimeMap, CompositionTimeMapStep, Property,
    RateFunction, TrackId, TrackTiming,
};

pub const DEFAULT_LAGGED_START_LAG_RATIO: f64 = 0.05;

#[derive(Clone, Copy, Debug)]
struct RootInterval {
    start: f64,
    duration: f64,
}

/// Transient Rust authoring wrapper for Manim-compatible animation composition.
///
/// The wrapper is not persisted. It lowers through Noon's shared composition
/// scheduler into ordinary tracks, adding root-to-leaf time maps only when a
/// nonlinear outer rate function makes flattening insufficient.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationGroup {
    pub(super) animations: Vec<Animation>,
    pub(super) run_time: Option<f64>,
    pub(super) rate_func: RateFunction,
    pub(super) lag_ratio: f64,
}

impl AnimationGroup {
    pub fn new<A: IntoAnimations>(animations: A) -> Self {
        Self {
            animations: animations.into_animations(),
            run_time: None,
            rate_func: RateFunction::Linear,
            lag_ratio: 0.0,
        }
    }

    pub fn run_time(mut self, run_time: f64) -> Self {
        self.run_time = Some(run_time);
        self
    }

    pub fn rate_func(mut self, rate_func: RateFunction) -> Self {
        self.rate_func = rate_func;
        self
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        self.lag_ratio = lag_ratio;
        self
    }
}

impl From<AnimationGroup> for Animation {
    fn from(value: AnimationGroup) -> Self {
        Self::Group(value)
    }
}

/// Manim-compatible composition with a default lag ratio of 0.05.
#[derive(Clone, Debug, PartialEq)]
pub struct LaggedStart(AnimationGroup);

impl LaggedStart {
    pub fn new<A: IntoAnimations>(animations: A) -> Self {
        Self(AnimationGroup::new(animations).lag_ratio(DEFAULT_LAGGED_START_LAG_RATIO))
    }

    pub fn run_time(mut self, run_time: f64) -> Self {
        self.0 = self.0.run_time(run_time);
        self
    }

    pub fn rate_func(mut self, rate_func: RateFunction) -> Self {
        self.0 = self.0.rate_func(rate_func);
        self
    }

    pub fn lag_ratio(mut self, lag_ratio: f64) -> Self {
        self.0 = self.0.lag_ratio(lag_ratio);
        self
    }
}

impl From<LaggedStart> for Animation {
    fn from(value: LaggedStart) -> Self {
        Self::Group(value.0)
    }
}

/// Manim-compatible sequential composition (`lag_ratio = 1`).
#[derive(Clone, Debug, PartialEq)]
pub struct Succession(AnimationGroup);

impl Succession {
    pub fn new<A: IntoAnimations>(animations: A) -> Self {
        Self(AnimationGroup::new(animations).lag_ratio(1.0))
    }

    pub fn run_time(mut self, run_time: f64) -> Self {
        self.0 = self.0.run_time(run_time);
        self
    }

    pub fn rate_func(mut self, rate_func: RateFunction) -> Self {
        self.0 = self.0.rate_func(rate_func);
        self
    }
}

impl From<Succession> for Animation {
    fn from(value: Succession) -> Self {
        Self::Group(value.0)
    }
}

pub(super) fn schedule_group(
    scene: &mut Scene,
    group: AnimationGroup,
    start: f64,
    duration: f64,
    rate_func_override: Option<RateFunction>,
) -> Result<(), AuthoringError> {
    let root = RootInterval { start, duration };
    let mut path = Vec::new();
    schedule_group_inner(
        scene,
        group,
        start,
        duration,
        rate_func_override,
        root,
        &mut path,
    )?;
    scene.cursor = start + duration;
    Ok(())
}

fn schedule_group_inner(
    scene: &mut Scene,
    group: AnimationGroup,
    start: f64,
    duration: f64,
    rate_func_override: Option<RateFunction>,
    root: RootInterval,
    path: &mut Vec<CompositionTimeMapStep>,
) -> Result<(), AuthoringError> {
    let child_run_times = group
        .animations
        .iter()
        .map(intrinsic_run_time)
        .collect::<Result<Vec<_>, _>>()?;
    let schedule = resolve_composition_schedule(&child_run_times, group.lag_ratio, Some(duration))?;
    let outer_rate_func = rate_func_override.unwrap_or(group.rate_func);

    for (child, interval) in group.animations.into_iter().zip(schedule.intervals) {
        let child_start = start + interval.start_time;
        let child_duration = interval.duration;
        path.push(CompositionTimeMapStep::new(
            interval.start_time / schedule.run_time,
            interval.duration / schedule.run_time,
            outer_rate_func,
        ));

        match child {
            Animation::Group(group) => {
                schedule_group_inner(scene, group, child_start, child_duration, None, root, path)?
            }
            leaf => schedule_leaf(scene, leaf, child_start, child_duration, root, path)?,
        }
        path.pop();
    }
    Ok(())
}

fn intrinsic_run_time(animation: &Animation) -> Result<f64, AuthoringError> {
    match animation {
        Animation::Group(group) => {
            let child_run_times = group
                .animations
                .iter()
                .map(intrinsic_run_time)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(
                resolve_composition_schedule(&child_run_times, group.lag_ratio, group.run_time)?
                    .run_time,
            )
        }
        _ => Ok(1.0),
    }
}

fn schedule_leaf(
    scene: &mut Scene,
    animation: Animation,
    start: f64,
    duration: f64,
    root: RootInterval,
    path: &[CompositionTimeMapStep],
) -> Result<(), AuthoringError> {
    let first_track = scene.definition.tracks().len();
    scene.cursor = start;
    scene.schedule(vec![animation], duration, None)?;

    if !path
        .iter()
        .any(|step| step.rate_func != RateFunction::Linear)
    {
        return Ok(());
    }

    let tracks = scene.definition.tracks()[first_track..]
        .iter()
        .filter(|track| track.property != Property::Presence)
        .map(|track| (track.id, track.timing.easing))
        .collect::<Vec<(TrackId, RateFunction)>>();
    let time_map = CompositionTimeMap::from_steps(path.to_vec());
    for (track_id, leaf_rate_func) in tracks {
        let found = scene.definition.remap_track_for_composition(
            track_id,
            TrackTiming::new(root.start, root.duration, leaf_rate_func),
            time_map.clone(),
        )?;
        debug_assert!(found, "newly authored track must remain addressable");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy::{Circle, Square, DOWN, RIGHT, UP};

    #[test]
    fn linear_animation_group_stays_flattened() {
        let mut scene = Scene::new();
        let left = scene.add(Circle::new(0.4));
        let right = scene.add(Square::new(0.8));

        scene
            .play(
                AnimationGroup::new((left.animate().shift(UP), right.animate().shift(DOWN)))
                    .lag_ratio(0.5),
            )
            .run_time(3.0)
            .unwrap();

        let tracks = scene.definition().tracks();
        assert_eq!(tracks.len(), 2);
        assert!(tracks.iter().all(|track| track.time_map.is_identity()));
        assert!((tracks[0].timing.start_time - 0.0).abs() < 1e-12);
        assert!((tracks[0].timing.duration - 2.0).abs() < 1e-12);
        assert!((tracks[1].timing.start_time - 1.0).abs() < 1e-12);
        assert!((tracks[1].timing.duration - 2.0).abs() < 1e-12);
    }

    #[test]
    fn nonlinear_succession_uses_shared_root_to_leaf_maps() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5));

        scene
            .play(
                Succession::new((circle.animate().shift(RIGHT), circle.animate().shift(UP)))
                    .rate_func(RateFunction::ThereAndBack),
            )
            .run_time(2.0)
            .unwrap();

        let tracks = scene.definition().tracks();
        assert_eq!(tracks.len(), 2);
        for track in tracks {
            assert_eq!(track.timing.start_time, 0.0);
            assert_eq!(track.timing.duration, 2.0);
            assert_eq!(track.timing.easing, RateFunction::Smooth);
            assert_eq!(track.time_map.steps.len(), 1);
            assert_eq!(
                track.time_map.steps[0].rate_func,
                RateFunction::ThereAndBack
            );
            assert!((track.time_map.steps[0].duration - 0.5).abs() < 1e-12);
        }
        assert!((tracks[0].time_map.steps[0].start - 0.0).abs() < 1e-12);
        assert!((tracks[1].time_map.steps[0].start - 0.5).abs() < 1e-12);
        assert_eq!(
            scene.snapshot(circle).unwrap().transform.translation,
            RIGHT + UP
        );
        assert_eq!(scene.time(), 2.0);
    }

    #[test]
    fn nested_group_carries_one_step_per_group_boundary() {
        let mut scene = Scene::new();
        let circle = scene.add(Circle::new(0.5));
        let square = scene.add(Square::new(0.8));
        let inner = Succession::new((circle.animate().shift(RIGHT), circle.animate().shift(UP)))
            .rate_func(RateFunction::Smooth);
        let outer = AnimationGroup::new((inner, square.animate().shift(UP)))
            .rate_func(RateFunction::Linear);

        scene.play(outer).run_time(4.0).unwrap();

        let circle_tracks = scene
            .definition()
            .tracks()
            .iter()
            .filter(|track| track.object == circle.id())
            .collect::<Vec<_>>();
        assert_eq!(circle_tracks.len(), 2);
        assert!(circle_tracks
            .iter()
            .all(|track| track.time_map.steps.len() == 2));
        assert!(circle_tracks.iter().all(|track| {
            track.time_map.steps[0].rate_func == RateFunction::Linear
                && track.time_map.steps[1].rate_func == RateFunction::Smooth
        }));
    }

    #[test]
    fn lagged_start_uses_manim_default_lag_ratio() {
        let mut scene = Scene::new();
        let first = scene.add(Circle::new(0.4));
        let second = scene.add(Square::new(0.8));
        scene
            .play(LaggedStart::new((
                first.animate().shift(UP),
                second.animate().shift(UP),
            )))
            .run_time(2.1)
            .unwrap();

        let tracks = scene.definition().tracks();
        assert!((tracks[0].timing.duration - 2.0).abs() < 1e-12);
        assert!((tracks[1].timing.start_time - 0.1).abs() < 1e-12);
        assert!((tracks[1].timing.duration - 2.0).abs() < 1e-12);
    }
}

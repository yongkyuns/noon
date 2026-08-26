use noon_core::{
    ReactiveError, ReactiveValue, SignalId, SignalSource, SignalTimelineDefinition,
    SignalTimelineError, TimedSemanticScene,
};

use crate::{
    EvaluationError, FrameChanges, FrameState, ReactiveRuntimeStats, SceneBuildError, SceneInstance,
};

#[derive(Clone, Debug, PartialEq)]
pub enum TimedSceneRuntimeError {
    Build(SceneBuildError),
    Evaluation(EvaluationError),
    Reactive(ReactiveError),
    SignalTimeline(SignalTimelineError),
}

impl std::fmt::Display for TimedSceneRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::Reactive(error) => error.fmt(formatter),
            Self::SignalTimeline(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TimedSceneRuntimeError {}

impl From<SceneBuildError> for TimedSceneRuntimeError {
    fn from(value: SceneBuildError) -> Self {
        Self::Build(value)
    }
}

impl From<EvaluationError> for TimedSceneRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl From<ReactiveError> for TimedSceneRuntimeError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl From<SignalTimelineError> for TimedSceneRuntimeError {
    fn from(value: SignalTimelineError) -> Self {
        Self::SignalTimeline(value)
    }
}

#[derive(Clone, Debug)]
struct SignalTrackGroup {
    signal: SignalId,
    initial: f32,
    track_indices: Vec<usize>,
}

/// Runtime for a semantic scene whose scalar inputs may also be driven by deterministic tracks.
///
/// Signal tracks are evaluated in Rust after ordinary object-timeline evaluation. A changed
/// tracker value is fed through `SceneInstance::set_reactive_input`, so only the affected native
/// dependency branch and dense property targets are updated. No frontend code executes per frame.
#[derive(Clone, Debug)]
pub struct TimedSceneInstance {
    inner: SceneInstance,
    timeline: SignalTimelineDefinition,
    groups: Vec<SignalTrackGroup>,
}

impl TimedSceneInstance {
    pub fn from_scene_instance(inner: SceneInstance) -> Self {
        Self {
            inner,
            timeline: SignalTimelineDefinition::new(),
            groups: Vec::new(),
        }
    }

    pub fn scene(&self) -> &SceneInstance {
        &self.inner
    }

    pub fn scene_mut(&mut self) -> &mut SceneInstance {
        &mut self.inner
    }

    pub fn from_timed(scene: &TimedSemanticScene) -> Result<Self, TimedSceneRuntimeError> {
        let inner = SceneInstance::from_semantic(scene.semantic())?;
        let timeline = SignalTimelineDefinition::from_parts(
            scene.semantic().reactive(),
            scene.signal_timeline().tracks().to_vec(),
        )?;
        let groups = build_signal_groups(scene, &timeline)?;
        let mut result = Self {
            inner,
            timeline,
            groups,
        };
        result.apply_signal_timeline(0.0)?;
        Ok(result)
    }

    pub fn frame(&self) -> &FrameState {
        self.inner.frame()
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.inner.take_frame_changes()
    }

    pub const fn last_reactive_stats(&self) -> ReactiveRuntimeStats {
        self.inner.last_reactive_stats()
    }

    pub fn reactive_value(&self, signal: SignalId) -> Option<&ReactiveValue> {
        self.inner.reactive_value(signal)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.inner.seek(time)?;
        self.apply_signal_timeline(time)?;
        Ok(self.inner.frame())
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.inner.advance_to(time)?;
        self.apply_signal_timeline(time)?;
        Ok(self.inner.frame())
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, TimedSceneRuntimeError> {
        self.inner.evaluate(time)?;
        self.apply_signal_timeline(time)?;
        Ok(self.inner.frame())
    }

    pub fn set_reactive_input(
        &mut self,
        signal: SignalId,
        value: impl Into<ReactiveValue>,
    ) -> Result<&FrameState, TimedSceneRuntimeError> {
        if self.timeline.drives(signal) {
            return Err(SignalTimelineError::ExternallyDrivenSignal(signal).into());
        }
        Ok(self.inner.set_reactive_input(signal, value)?)
    }

    fn apply_signal_timeline(&mut self, time: f64) -> Result<(), TimedSceneRuntimeError> {
        for group in &self.groups {
            let value = signal_value_at(&self.timeline, group, time);
            self.inner.set_reactive_input(group.signal, value)?;
        }
        Ok(())
    }
}

fn build_signal_groups(
    scene: &TimedSemanticScene,
    timeline: &SignalTimelineDefinition,
) -> Result<Vec<SignalTrackGroup>, SignalTimelineError> {
    let mut groups = Vec::<SignalTrackGroup>::new();
    for (index, track) in timeline.tracks().iter().enumerate() {
        if let Some(group) = groups.iter_mut().find(|group| group.signal == track.signal) {
            group.track_indices.push(index);
            continue;
        }
        let definition = scene
            .semantic()
            .reactive()
            .signals()
            .iter()
            .find(|signal| signal.id == track.signal)
            .ok_or(SignalTimelineError::UnknownSignal(track.signal))?;
        let SignalSource::Input(ReactiveValue::Scalar(initial)) = &definition.source else {
            return Err(SignalTimelineError::NonScalarSignal(track.signal));
        };
        groups.push(SignalTrackGroup {
            signal: track.signal,
            initial: *initial,
            track_indices: vec![index],
        });
    }
    Ok(groups)
}

fn signal_value_at(
    timeline: &SignalTimelineDefinition,
    group: &SignalTrackGroup,
    time: f64,
) -> f32 {
    let mut value = group.initial;
    for index in &group.track_indices {
        let track = &timeline.tracks()[*index];
        if time < track.timing.start_time {
            break;
        }
        let end = track.timing.start_time + track.timing.duration;
        if time >= end {
            value = track.to;
            continue;
        }
        let raw = ((time - track.timing.start_time) / track.timing.duration) as f32;
        let progress = track.timing.easing.evaluate(raw);
        return track.from + (track.to - track.from) * progress;
    }
    value
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, Property, RateFunction, ReactiveExpr, SignalTimelineDefinition,
        TimedSemanticScene, TrackTiming, Vec2,
    };

    use super::*;

    fn tracked_scene() -> (TimedSemanticScene, SignalId, SignalId) {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let tracker = semantic.add_input(0.0_f32);
        let position = semantic.add_derived(ReactiveExpr::Mul(
            Box::new(ReactiveExpr::signal(tracker)),
            Box::new(ReactiveExpr::vec2(Vec2::new(2.0, 0.0))),
        ));
        semantic.bind(position, object, Property::Position);
        let external = semantic.add_input(0.25_f32);
        semantic.bind(external, object, Property::Rotation);

        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                0.0,
                1.0,
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            )
            .unwrap();
        (
            TimedSemanticScene::from_parts(semantic, timeline).unwrap(),
            tracker,
            external,
        )
    }

    #[test]
    fn signal_tracks_drive_native_dependencies_during_seek_and_forward_playback() {
        let (scene, tracker, _) = tracked_scene();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();

        instance.seek(1.0).unwrap();
        assert_eq!(
            instance.reactive_value(tracker),
            Some(&ReactiveValue::Scalar(0.5))
        );
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(1.0, 0.0)
        );

        instance.advance_to(2.0).unwrap();
        assert_eq!(
            instance.reactive_value(tracker),
            Some(&ReactiveValue::Scalar(1.0))
        );
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(2.0, 0.0)
        );

        instance.seek(0.5).unwrap();
        assert_eq!(
            instance.reactive_value(tracker),
            Some(&ReactiveValue::Scalar(0.25))
        );
    }

    #[test]
    fn gaps_hold_previous_signal_value() {
        let mut semantic = noon_core::SemanticScene::new();
        let object = semantic.add(GeometryRef::circle(0.5));
        let tracker = semantic.add_input(0.0_f32);
        semantic.bind(tracker, object, Property::Rotation);
        let mut timeline = SignalTimelineDefinition::new();
        timeline
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap()
            .add_scalar_track(
                semantic.reactive(),
                tracker,
                1.0,
                2.0,
                TrackTiming::new(2.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let scene = TimedSemanticScene::from_parts(semantic, timeline).unwrap();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();
        instance.seek(1.5).unwrap();
        assert_eq!(
            instance.reactive_value(tracker),
            Some(&ReactiveValue::Scalar(1.0))
        );
    }

    #[test]
    fn externally_mutating_a_timeline_driven_signal_is_rejected() {
        let (scene, tracker, external) = tracked_scene();
        let mut instance = TimedSceneInstance::from_timed(&scene).unwrap();
        assert!(matches!(
            instance.set_reactive_input(tracker, 0.75_f32),
            Err(TimedSceneRuntimeError::SignalTimeline(
                SignalTimelineError::ExternallyDrivenSignal(_)
            ))
        ));
        instance.set_reactive_input(external, 0.75_f32).unwrap();
        assert_eq!(instance.frame().objects[0].transform.rotation, 0.75);
    }
}

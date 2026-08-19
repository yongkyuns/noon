//! Deterministic renderer-free evaluation of compiled Noon scenes.

#![forbid(unsafe_code)]

use noon_compile::{CompilePatchError, CompiledScene, CompiledTrack};
use noon_core::{
    Easing, GeometryRef, ObjectId, Property, ScenePatch, Style, TrackValues, Transform2D, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrameObjectState {
    pub id: ObjectId,
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameState {
    pub time: f64,
    pub objects: Vec<FrameObjectState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationStats {
    pub groups_evaluated: usize,
    pub tracks_advanced: usize,
    pub binary_search_steps: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationError {
    InvalidTime(f64),
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTime(time) => write!(formatter, "invalid scene time {time}"),
        }
    }
}

impl std::error::Error for EvaluationError {}

#[derive(Clone, Debug)]
struct TrackGroup {
    object_index: usize,
    property: Property,
    start: usize,
    end: usize,
    cursor: usize,
}

#[derive(Clone, Debug)]
pub struct SceneInstance {
    compiled: CompiledScene,
    frame: FrameState,
    groups: Vec<TrackGroup>,
    last_stats: EvaluationStats,
}

impl SceneInstance {
    pub fn new(compiled: CompiledScene) -> Self {
        let frame = base_frame(&compiled, 0.0);
        let groups = build_groups(compiled.tracks());
        let mut instance = Self {
            compiled,
            frame,
            groups,
            last_stats: EvaluationStats::default(),
        };
        instance.seek_unchecked(0.0);
        instance
    }

    pub fn frame(&self) -> &FrameState {
        &self.frame
    }

    pub const fn last_stats(&self) -> EvaluationStats {
        self.last_stats
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        if time >= self.frame.time {
            self.advance_unchecked(time);
        } else {
            self.seek_unchecked(time);
        }
        Ok(&self.frame)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        self.seek_unchecked(time);
        Ok(&self.frame)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        if time < self.frame.time {
            self.seek_unchecked(time);
        } else {
            self.advance_unchecked(time);
        }
        Ok(&self.frame)
    }

    pub fn apply_patch(&mut self, patch: &ScenePatch) -> Result<&FrameState, CompilePatchError> {
        let current_time = self.frame.time;
        self.compiled.apply_patch(patch)?;
        self.groups = build_groups(self.compiled.tracks());
        self.seek_unchecked(current_time);
        Ok(&self.frame)
    }

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();

        for group in &mut self.groups {
            let slice = &tracks[group.start..group.end];
            group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }

    fn advance_unchecked(&mut self, time: f64) {
        self.frame.time = time;
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();

        for group in &mut self.groups {
            let slice = &tracks[group.start..group.end];
            while group.cursor < slice.len() && slice[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            apply_group(&mut self.frame, slice, group, time);
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
}

fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    FrameState {
        time,
        objects: compiled
            .objects()
            .iter()
            .map(|object| FrameObjectState {
                id: object.id,
                geometry: object.geometry,
                transform: object.base_transform,
                style: object.base_style,
            })
            .collect(),
    }
}

fn build_groups(tracks: &[CompiledTrack]) -> Vec<TrackGroup> {
    let mut groups = Vec::new();
    let mut start = 0;

    while start < tracks.len() {
        let object_index = tracks[start].object_index as usize;
        let property = tracks[start].property;
        let mut end = start + 1;
        while end < tracks.len()
            && tracks[end].object_index as usize == object_index
            && tracks[end].property == property
        {
            end += 1;
        }
        groups.push(TrackGroup {
            object_index,
            property,
            start,
            end,
            cursor: 0,
        });
        start = end;
    }

    groups
}

fn upper_bound_start(tracks: &[CompiledTrack], time: f64, steps: &mut usize) -> usize {
    let mut low = 0;
    let mut high = tracks.len();
    while low < high {
        *steps += 1;
        let middle = low + (high - low) / 2;
        if tracks[middle].timing.start_time <= time {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

fn apply_group(frame: &mut FrameState, tracks: &[CompiledTrack], group: &TrackGroup, time: f64) {
    if group.cursor == 0 {
        return;
    }
    let track = &tracks[group.cursor - 1];
    let value = interpolate(track, time);
    let object = &mut frame.objects[group.object_index];

    match (group.property, value) {
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            object.transform.translation = value;
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            object.transform.rotation = value;
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            object.style.opacity = value;
        }
        _ => unreachable!("compiled track value type must match its property"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EvaluatedValue {
    Scalar(f32),
    Vec2(Vec2),
}

fn interpolate(track: &CompiledTrack, time: f64) -> EvaluatedValue {
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    let progress = apply_easing(track.timing.easing, raw);
    match track.values {
        TrackValues::Scalar { from, to } => EvaluatedValue::Scalar(lerp(from, to, progress)),
        TrackValues::Vec2 { from, to } => EvaluatedValue::Vec2(Vec2::new(
            lerp(from.x, to.x, progress),
            lerp(from.y, to.y, progress),
        )),
    }
}

const fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn apply_easing(easing: Easing, progress: f32) -> f32 {
    match easing {
        Easing::Linear => progress,
        Easing::EaseInOutCubic => {
            if progress < 0.5 {
                4.0 * progress * progress * progress
            } else {
                1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{
        Easing, GeometryRef, Property, SceneDefinition, Style, TrackDefinition, TrackTiming,
    };

    use super::*;

    fn compile_linear_scene() -> CompiledScene {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(1.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        CompiledScene::compile(&scene).expect("scene must compile")
    }

    #[test]
    fn timeline_endpoints_and_midpoint_are_exact() {
        let mut instance = SceneInstance::new(compile_linear_scene());

        assert_eq!(
            instance.seek(0.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            instance.seek(2.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::new(5.0, 0.0)
        );
        assert_eq!(
            instance.seek(3.0).expect("valid time").objects[0]
                .transform
                .translation,
            Vec2::new(10.0, 0.0)
        );
    }

    #[test]
    fn backward_and_forward_seeks_are_deterministic() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let first = instance.seek(2.25).expect("valid time").objects[0].clone();
        instance.seek(3.0).expect("valid time");
        instance.seek(0.5).expect("valid time");
        let second = instance.seek(2.25).expect("valid time").objects[0].clone();

        assert_eq!(first, second);
    }

    #[test]
    fn sequential_stepping_matches_direct_seek() {
        let compiled = compile_linear_scene();
        let mut sequential = SceneInstance::new(compiled.clone());
        let mut direct = SceneInstance::new(compiled);

        for step in 1..=25 {
            sequential
                .advance_to(f64::from(step) * 0.1)
                .expect("valid time");
        }
        direct.seek(2.5).expect("valid time");

        assert_eq!(sequential.frame(), direct.frame());
    }

    #[test]
    fn completed_history_is_not_rescanned_during_forward_steps() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for index in 0..1_000 {
            let start = f64::from(index);
            let from = index as f32;
            scene
                .animate_position(
                    object,
                    Vec2::new(from, 0.0),
                    Vec2::new(from + 1.0, 0.0),
                    TrackTiming::new(start, 0.5, Easing::Linear),
                )
                .expect("valid track");
        }

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.seek(999.25).expect("valid time");
        assert!(instance.last_stats().binary_search_steps < 20);

        instance.advance_to(999.30).expect("valid time");
        assert_eq!(instance.last_stats().tracks_advanced, 0);
        assert_eq!(instance.last_stats().binary_search_steps, 0);
        assert_eq!(instance.last_stats().groups_evaluated, 1);
    }

    #[test]
    fn scalar_properties_are_evaluated_without_renderer_state() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);

        let opacity = instance.seek(1.0).expect("valid time").objects[0]
            .style
            .opacity;
        assert_eq!(opacity, 0.5);
    }

    #[test]
    fn non_finite_times_are_rejected() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        assert!(matches!(
            instance.seek(f64::NAN),
            Err(EvaluationError::InvalidTime(_))
        ));
    }

    #[test]
    fn live_patch_matches_recompile_of_equivalent_definition() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let track_id = definition
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(4.0, 0.0),
                TrackTiming::new(0.0, 4.0, Easing::Linear),
            )
            .expect("valid track");

        let compiled = CompiledScene::compile(&definition).expect("scene must compile");
        let mut live = SceneInstance::new(compiled);
        live.seek(2.0).expect("valid time");

        let replacement = TrackDefinition {
            id: track_id,
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(8.0, 2.0),
            },
            timing: TrackTiming::new(0.0, 4.0, Easing::Linear),
        };
        let track_patch = ScenePatch::ReplaceTrack(replacement);
        let style_patch = ScenePatch::SetStyle {
            object,
            style: Style {
                opacity: 0.75,
                ..Style::default()
            },
        };

        live.apply_patch(&track_patch).expect("valid patch");
        live.apply_patch(&style_patch).expect("valid patch");

        definition
            .apply_patch(track_patch)
            .expect("valid definition patch");
        definition
            .apply_patch(style_patch)
            .expect("valid definition patch");
        let expected_compiled =
            CompiledScene::compile(&definition).expect("scene must compile after patches");
        let mut expected = SceneInstance::new(expected_compiled);
        expected.seek(2.0).expect("valid time");

        assert_eq!(live.frame(), expected.frame());
    }

    #[test]
    fn removing_track_restores_base_property_at_current_time() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let track_id = definition
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let compiled = CompiledScene::compile(&definition).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.seek(1.0).expect("valid time");
        assert_eq!(instance.frame().objects[0].style.opacity, 0.5);

        instance
            .apply_patch(&ScenePatch::RemoveTrack(track_id))
            .expect("valid patch");
        assert_eq!(instance.frame().objects[0].style.opacity, 1.0);
    }
}

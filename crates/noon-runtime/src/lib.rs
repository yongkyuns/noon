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
    /// Normalized per-object reveal state. Renderers may ignore it for
    /// geometry types that do not support reveal yet.
    pub reveals: Vec<f32>,
}

impl FrameState {
    pub fn reveal(&self, object_index: usize) -> f32 {
        self.reveals[object_index]
    }
}

/// Object-level changes accumulated since the renderer last consumed them.
///
/// A full invalidation is used after seeks and structural edits. Forward
/// evaluation and value-only patches retain a compact, deduplicated list of
/// changed object indices instead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameChanges {
    all: bool,
    object_indices: Vec<usize>,
}

impl FrameChanges {
    pub fn all() -> Self {
        Self {
            all: true,
            object_indices: Vec::new(),
        }
    }

    pub fn objects(mut object_indices: Vec<usize>) -> Self {
        object_indices.sort_unstable();
        object_indices.dedup();
        Self {
            all: false,
            object_indices,
        }
    }

    pub const fn is_all(&self) -> bool {
        self.all
    }

    pub fn object_indices(&self) -> &[usize] {
        &self.object_indices
    }

    pub const fn is_empty(&self) -> bool {
        !self.all && self.object_indices.is_empty()
    }

    fn invalidate_all(&mut self) {
        self.all = true;
        self.object_indices.clear();
    }

    fn insert(&mut self, object_index: usize) {
        if self.all || self.object_indices.last() == Some(&object_index) {
            return;
        }
        if self
            .object_indices
            .last()
            .is_none_or(|last| *last < object_index)
        {
            self.object_indices.push(object_index);
        } else if let Err(position) = self.object_indices.binary_search(&object_index) {
            self.object_indices.insert(position, object_index);
        }
    }
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
    changes: FrameChanges,
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
            changes: FrameChanges::all(),
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

    /// Drains changes accumulated by evaluation and live patches.
    pub fn take_frame_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.changes)
    }

    pub fn contains_object(&self, id: ObjectId) -> bool {
        self.compiled.object_index(id).is_some()
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
        if matches!(
            patch,
            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            return Ok(&self.frame);
        }
        let current_time = self.frame.time;
        self.compiled.apply_patch(patch)?;
        self.groups = build_groups(self.compiled.tracks());
        self.seek_unchecked(current_time);
        Ok(&self.frame)
    }

    fn apply_value_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        let object = match patch {
            ScenePatch::SetTransform { object, .. } | ScenePatch::SetStyle { object, .. } => {
                *object
            }
            _ => unreachable!("value patch helper only accepts transform or style patches"),
        };
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))? as usize;
        let before = self.frame.objects[index].clone();
        self.compiled.apply_patch(patch)?;

        match patch {
            ScenePatch::SetTransform { transform, .. } => {
                self.frame.objects[index].transform = *transform;
                self.reapply_properties(index, &[Property::Position, Property::Rotation]);
            }
            ScenePatch::SetStyle { style, .. } => {
                self.frame.objects[index].style = *style;
                self.reapply_properties(index, &[Property::Opacity]);
            }
            _ => unreachable!("value patch helper only accepts transform or style patches"),
        }
        if self.frame.objects[index] != before {
            self.changes.insert(index);
        }
        Ok(())
    }

    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let tracks = self.compiled.tracks();
        let mut stats = EvaluationStats::default();
        for group in &mut self.groups {
            if group.object_index == object_index && properties.contains(&group.property) {
                let slice = &tracks[group.start..group.end];
                group.cursor = upper_bound_start(slice, time, &mut stats.binary_search_steps);
                apply_group(&mut self.frame, slice, group, time);
                stats.groups_evaluated += 1;
            }
        }
        self.last_stats = stats;
    }

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.changes.invalidate_all();
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
        let changes = &mut self.changes;

        for group in &mut self.groups {
            let slice = &tracks[group.start..group.end];
            while group.cursor < slice.len() && slice[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if apply_group(&mut self.frame, slice, group, time) {
                changes.insert(group.object_index);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
}

fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    let objects: Vec<_> = compiled
        .objects()
        .iter()
        .map(|object| FrameObjectState {
            id: object.id,
            geometry: object.geometry.clone(),
            transform: object.base_transform,
            style: object.base_style,
        })
        .collect();
    FrameState {
        time,
        reveals: initial_reveals(compiled, objects.len()),
        objects,
    }
}

fn initial_reveals(compiled: &CompiledScene, object_count: usize) -> Vec<f32> {
    let mut reveals = vec![1.0; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks()
        .iter()
        .filter(|track| track.property == Property::Reveal)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Scalar { from, .. } = track.values else {
            unreachable!("compiled reveal track must contain scalar values");
        };
        reveals[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    reveals
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

fn apply_group(
    frame: &mut FrameState,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
) -> bool {
    if group.cursor == 0 {
        return false;
    }
    let track = &tracks[group.cursor - 1];
    let value = interpolate(track, time);

    match (group.property, value) {
        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.reveals[group.object_index] != value;
            frame.reveals[group.object_index] = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            let object = &mut frame.objects[group.object_index];
            let changed = object.transform.translation != value;
            object.transform.translation = value;
            changed
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            let object = &mut frame.objects[group.object_index];
            let changed = object.transform.rotation != value;
            object.transform.rotation = value;
            changed
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            let object = &mut frame.objects[group.object_index];
            let changed = object.style.opacity != value;
            object.style.opacity = value;
            changed
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
        Color, Easing, GeometryRef, Property, SceneDefinition, Style, TrackDefinition, TrackTiming,
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
    fn reveal_endpoints_midpoint_and_prestart_state_are_deterministic() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(
            noon_core::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(3.0, 4.0)),
        ));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(1.0, 2.0, Easing::Linear))
            .expect("valid reveal track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));

        assert_eq!(instance.seek(0.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(1.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(2.0).expect("valid time").reveal(0), 0.5);
        assert_eq!(instance.seek(3.0).expect("valid time").reveal(0), 1.0);
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

    #[test]
    fn value_patch_updates_base_fields_without_overwriting_animated_values() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        definition
            .animate_scalar(
                object,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&definition).expect("scene must compile"));
        instance.seek(1.0).expect("valid time");

        instance
            .apply_patch(&ScenePatch::SetStyle {
                object,
                style: Style {
                    fill: Some(Color::rgb(0.2, 0.4, 0.8)),
                    opacity: 0.9,
                    ..Style::default()
                },
            })
            .expect("style patch must apply");

        assert_eq!(
            instance.frame().objects[0].style.fill,
            Some(Color::rgb(0.2, 0.4, 0.8))
        );
        assert_eq!(instance.frame().objects[0].style.opacity, 0.5);
        assert_eq!(instance.frame().time, 1.0);
    }

    #[test]
    fn frame_changes_are_consumed_and_static_steps_stay_clean() {
        let mut scene = SceneDefinition::new();
        scene.add(GeometryRef::circle(1.0));
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));

        assert!(instance.take_frame_changes().is_all());
        instance.advance_to(0.5).expect("valid time");
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn frame_changes_accumulate_animation_and_patches_until_consumed() {
        let mut scene = SceneDefinition::new();
        let animated = scene.add(GeometryRef::circle(1.0));
        let patched = scene.add(GeometryRef::rectangle(2.0, 1.0));
        scene
            .animate_position(
                animated,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, Easing::Linear),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        instance.take_frame_changes();

        instance.advance_to(0.5).expect("valid time");
        instance
            .apply_patch(&ScenePatch::SetStyle {
                object: patched,
                style: Style {
                    opacity: 0.5,
                    ..Style::default()
                },
            })
            .expect("valid patch");
        instance.advance_to(0.75).expect("valid time");

        assert_eq!(instance.take_frame_changes().object_indices(), &[0, 1]);
        assert!(instance.take_frame_changes().is_empty());
    }
}

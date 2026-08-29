use std::collections::BTreeMap;

use noon_compile::{
    CompiledChannelKey, CompiledTrack, RetainedCompiledScene, TransformGeometryPlan,
};
use noon_core::{
    GeometryRef, ObjectContentRef, ObjectId, Property, StrokeWidthMode, Style, TextResourceHandle,
    TrackValues, Transform2D,
};

use crate::{
    apply_transform_geometry, interpolate, interpolate_pointwise_rotation_transform,
    interpolate_style, interpolate_transform, mapped_track_progress,
    screen_space_path_pair_relative_to_current, set_optional_geometry_if_changed, track_progress,
    upper_bound_start, EvaluatedValue, EvaluationError, EvaluationStats, FrameChanges,
    TimelineEventScheduler, TrackGroup,
};

/// Renderer-independent frame state for retained geometry/text objects.
///
/// Text stays as a stable `TextResourceHandle` throughout evaluation. Ordinary
/// object properties animate without expanding text into geometry or copying the
/// backing text resource.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedFrameObjectState {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
}

impl RetainedFrameObjectState {
    pub fn geometry(&self) -> Option<&GeometryRef> {
        self.content.geometry()
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        self.content.text()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedFrameState {
    pub time: f64,
    pub objects: Vec<RetainedFrameObjectState>,
    pub presences: Vec<bool>,
    pub reveals: Vec<f32>,
    pub morphs: Vec<f32>,
    /// Transient geometry prepared for path transforms. Text never uses this
    /// channel; its retained resource handle remains unchanged.
    pub render_geometries: Vec<Option<GeometryRef>>,
}

impl RetainedFrameState {
    pub fn is_present(&self, object_index: usize) -> bool {
        self.presences[object_index]
    }

    pub fn appearance(&self, object_index: usize) -> f32 {
        self.objects[object_index].appearance
    }

    pub fn reveal(&self, object_index: usize) -> f32 {
        self.reveals[object_index]
    }

    pub fn morph(&self, object_index: usize) -> f32 {
        self.morphs[object_index]
    }

    pub fn render_geometry(&self, object_index: usize) -> Option<&GeometryRef> {
        self.render_geometries[object_index]
            .as_ref()
            .or_else(|| self.objects[object_index].geometry())
    }

    pub fn text(&self, object_index: usize) -> Option<TextResourceHandle> {
        self.objects[object_index].text()
    }
}

/// Deterministic runtime for the retained-object compiler path.
///
/// This deliberately excludes live patching/reactive callbacks for the first
/// retained slice. The stable object ordering and timeline evaluator match the
/// legacy runtime, while retained text remains resource-backed from compile to
/// frame state.
#[derive(Clone, Debug)]
pub struct RetainedSceneInstance {
    compiled: RetainedCompiledScene,
    frame: RetainedFrameState,
    groups: BTreeMap<CompiledChannelKey, TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
    changes: FrameChanges,
}

impl RetainedSceneInstance {
    pub fn new(compiled: RetainedCompiledScene) -> Self {
        let frame = retained_base_frame(&compiled, 0.0);
        let groups = retained_build_groups(&compiled);
        let timeline_scheduler = retained_scheduler(&compiled);
        let mut instance = Self {
            compiled,
            frame,
            groups,
            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            changes: FrameChanges::all(),
        };
        instance.seek_unchecked(0.0);
        instance
    }

    pub fn frame(&self) -> &RetainedFrameState {
        &self.frame
    }

    pub const fn last_stats(&self) -> EvaluationStats {
        self.last_stats
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.changes)
    }

    pub fn contains_object(&self, id: ObjectId) -> bool {
        self.compiled.object_index(id).is_some()
    }

    pub fn evaluate(&mut self, time: f64) -> Result<&RetainedFrameState, EvaluationError> {
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

    pub fn seek(&mut self, time: f64) -> Result<&RetainedFrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        self.seek_unchecked(time);
        Ok(&self.frame)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&RetainedFrameState, EvaluationError> {
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

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = retained_base_frame(&self.compiled, time);
        self.changes.invalidate_all();
        let mut stats = EvaluationStats::default();

        for group in self.groups.values_mut() {
            let tracks = self.compiled.channel_tracks(group.channel);
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            retained_apply_group(&mut self.frame, tracks, group, time);
            stats.groups_evaluated += 1;
        }
        self.timeline_scheduler.seek(time);
        self.last_stats = stats;
    }

    fn advance_unchecked(&mut self, time: f64) {
        self.frame.time = time;
        let requested_count = self.timeline_scheduler.advance(time);
        let mut stats = EvaluationStats::default();

        for request_index in 0..requested_count {
            let channel = self.timeline_scheduler.requested()[request_index];
            let tracks = self.compiled.channel_tracks(channel);
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            while group.cursor < tracks.len() && tracks[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if retained_apply_group(&mut self.frame, tracks, group, time) {
                self.changes.insert(channel.object_index as usize);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
}

fn retained_base_frame(compiled: &RetainedCompiledScene, time: f64) -> RetainedFrameState {
    let object_count = compiled.objects().len();
    let objects = compiled
        .objects()
        .iter()
        .enumerate()
        .map(|(index, object)| RetainedFrameObjectState {
            id: object.id,
            content: object.content.clone(),
            transform: object.base_transform,
            style: object.base_style,
            appearance: retained_initial_scalar(compiled, index, Property::Appearance, 1.0),
        })
        .collect();
    let presences = (0..object_count)
        .map(|index| retained_initial_bool(compiled, index, Property::Presence, true))
        .collect();
    let reveals = (0..object_count)
        .map(|index| retained_initial_scalar(compiled, index, Property::Reveal, 1.0))
        .collect();
    let morphs = (0..object_count)
        .map(|index| retained_initial_scalar(compiled, index, Property::Morph, 0.0))
        .collect();

    RetainedFrameState {
        time,
        objects,
        presences,
        reveals,
        morphs,
        render_geometries: vec![None; object_count],
    }
}

fn retained_initial_bool(
    compiled: &RetainedCompiledScene,
    object_index: usize,
    property: Property,
    default: bool,
) -> bool {
    let channel = CompiledChannelKey::new(object_index as u32, property);
    let Some(track) = compiled.channel_tracks(channel).first() else {
        return default;
    };
    let TrackValues::Bool { from, .. } = &track.values else {
        unreachable!("compiled bool property must contain bool values");
    };
    *from
}

fn retained_initial_scalar(
    compiled: &RetainedCompiledScene,
    object_index: usize,
    property: Property,
    default: f32,
) -> f32 {
    let channel = CompiledChannelKey::new(object_index as u32, property);
    let Some(track) = compiled.channel_tracks(channel).first() else {
        return default;
    };
    let TrackValues::Scalar { from, .. } = &track.values else {
        unreachable!("compiled scalar property must contain scalar values");
    };
    from.clamp(0.0, 1.0)
}

fn retained_build_groups(
    compiled: &RetainedCompiledScene,
) -> BTreeMap<CompiledChannelKey, TrackGroup> {
    let mut groups = BTreeMap::new();
    for channel in compiled.channels() {
        let tracks = compiled.channel_tracks(channel);
        groups.insert(
            channel,
            TrackGroup {
                channel,
                cursor: 0,
                mapped: tracks.iter().any(|track| !track.time_map.is_identity()),
            },
        );
    }
    groups
}

fn retained_scheduler(compiled: &RetainedCompiledScene) -> TimelineEventScheduler {
    let mut scheduler = TimelineEventScheduler::new(&[]);
    for channel in compiled.channels() {
        scheduler.relower_channel(channel, compiled.channel_tracks(channel));
    }
    scheduler
}

fn retained_apply_group(
    frame: &mut RetainedFrameState,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
) -> bool {
    if group.cursor == 0 {
        return false;
    }
    if group.channel.property == Property::Presence {
        let track = &tracks[group.cursor - 1];
        let TrackValues::Bool { to, .. } = &track.values else {
            unreachable!("compiled Presence track must contain bool values");
        };
        let index = group.channel.object_index as usize;
        let changed = frame.presences[index] != *to;
        frame.presences[index] = *to;
        return changed;
    }

    let selected = if group.mapped {
        tracks[..group.cursor]
            .iter()
            .rev()
            .find_map(|track| mapped_track_progress(track, time).map(|progress| (track, progress)))
    } else {
        let track = &tracks[group.cursor - 1];
        Some((track, track_progress(track, time)))
    };
    let Some((track, progress)) = selected else {
        return false;
    };

    let index = group.channel.object_index as usize;
    if group.channel.property == Property::Transform {
        return retained_apply_transform_track(frame, index, track, progress);
    }
    retained_apply_evaluated_value(
        frame,
        index,
        group.channel.property,
        interpolate(track, progress),
    )
}

fn retained_apply_evaluated_value(
    frame: &mut RetainedFrameState,
    index: usize,
    property: Property,
    value: EvaluatedValue,
) -> bool {
    match (property, value) {
        (Property::Appearance, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.objects[index].appearance != value;
            frame.objects[index].appearance = value;
            changed
        }
        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.reveals[index] != value;
            frame.reveals[index] = value;
            changed
        }
        (Property::Morph, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.morphs[index] != value;
            frame.morphs[index] = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            let changed = frame.objects[index].transform.translation != value;
            frame.objects[index].transform.translation = value;
            changed
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            let changed = frame.objects[index].transform.rotation != value;
            frame.objects[index].transform.rotation = value;
            changed
        }
        (Property::Scale, EvaluatedValue::Vec2(value)) => {
            let changed = frame.objects[index].transform.scale != value;
            frame.objects[index].transform.scale = value;
            changed
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            let changed = frame.objects[index].style.opacity != value;
            frame.objects[index].style.opacity = value;
            changed
        }
        _ => unreachable!("compiled track value type must match its property"),
    }
}

fn retained_apply_transform_track(
    frame: &mut RetainedFrameState,
    index: usize,
    track: &CompiledTrack,
    progress: f32,
) -> bool {
    let TrackValues::Object { from, to } = &track.values else {
        unreachable!("compiled Transform track must contain object snapshots");
    };
    let plan = track
        .transform_geometry_plan
        .as_ref()
        .expect("compiled Transform track must carry a geometry plan");
    let next_transform = if matches!(plan, TransformGeometryPlan::PointwiseRotation) {
        interpolate_pointwise_rotation_transform(from.transform, to.transform, progress)
    } else {
        interpolate_transform(from.transform, to.transform, progress)
    };
    let next_style = interpolate_style(from.style, to.style, progress);
    let next_morph = if matches!(plan, TransformGeometryPlan::PathPair(_)) {
        progress
    } else {
        0.0
    };
    let owned_render_geometry = match plan {
        TransformGeometryPlan::PathPair(prepared)
            if from.style.stroke_width_mode == StrokeWidthMode::ScreenSpace
                && to.style.stroke_width_mode == StrokeWidthMode::ScreenSpace =>
        {
            screen_space_path_pair_relative_to_current(prepared, from, to, next_transform)
        }
        _ => None,
    };
    let next_render_geometry = owned_render_geometry.as_ref().or(match plan {
        TransformGeometryPlan::PathPair(prepared) => Some(prepared),
        _ => None,
    });

    let object = &mut frame.objects[index];
    let ObjectContentRef::Geometry(geometry) = &mut object.content else {
        unreachable!("retained compiler rejects geometry-snapshot Transform tracks on text");
    };
    let mut changed = apply_transform_geometry(geometry, plan, from, to, progress);
    if object.transform != next_transform {
        object.transform = next_transform;
        changed = true;
    }
    if object.style != next_style {
        object.style = next_style;
        changed = true;
    }
    if frame.morphs[index] != next_morph {
        frame.morphs[index] = next_morph;
        changed = true;
    }
    changed |=
        set_optional_geometry_if_changed(&mut frame.render_geometries[index], next_render_geometry);
    changed
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledScene, RetainedCompiledScene};
    use noon_core::{
        CompositionTimeMap, GeometryRef, ObjectId, ObjectSnapshot, Property, RateFunction,
        RetainedObjectDefinition, SceneDefinition, TextResourceHandle, TextResourceId,
        TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;
    use crate::SceneInstance;

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            id: TextResourceId::new(41),
            version: 7,
        }
    }

    fn text_track(
        id: u64,
        object: ObjectId,
        property: Property,
        values: TrackValues,
        start: f64,
        duration: f64,
    ) -> TrackDefinition {
        TrackDefinition {
            id: TrackId::new(id),
            object,
            property,
            values,
            timing: TrackTiming::new(start, duration, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        }
    }

    #[test]
    fn retained_text_handle_survives_seek_without_resource_copy_or_placeholder_geometry() {
        let text_id = ObjectId::new(10);
        let geometry_id = ObjectId::new(11);
        let objects = [
            RetainedObjectDefinition::text(text_id, text_handle()),
            RetainedObjectDefinition::geometry(geometry_id, GeometryRef::circle(1.0)),
        ];
        let compiled = RetainedCompiledScene::compile(&objects, &[]).unwrap();
        let mut runtime = RetainedSceneInstance::new(compiled);

        let frame = runtime.seek(12.5).unwrap();
        assert_eq!(frame.objects[0].id, text_id);
        assert_eq!(frame.text(0), Some(text_handle()));
        assert_eq!(frame.render_geometry(0), None);
        assert_eq!(frame.objects[1].id, geometry_id);
        assert!(frame.render_geometry(1).is_some());
    }

    #[test]
    fn ordinary_text_properties_evaluate_in_the_shared_object_domain() {
        let object = ObjectId::new(3);
        let objects = [RetainedObjectDefinition::text(object, text_handle())];
        let tracks = [
            text_track(
                0,
                object,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(8.0, 4.0),
                },
                0.0,
                2.0,
            ),
            text_track(
                1,
                object,
                Property::Opacity,
                TrackValues::Scalar { from: 1.0, to: 0.2 },
                0.0,
                2.0,
            ),
            text_track(
                2,
                object,
                Property::Appearance,
                TrackValues::Scalar { from: 0.0, to: 1.0 },
                0.0,
                2.0,
            ),
        ];
        let compiled = RetainedCompiledScene::compile(&objects, &tracks).unwrap();
        let mut runtime = RetainedSceneInstance::new(compiled);

        let frame = runtime.seek(1.0).unwrap();
        assert_eq!(frame.objects[0].transform.translation, Vec2::new(4.0, 2.0));
        assert!((frame.objects[0].style.opacity - 0.6).abs() < 1e-6);
        assert_eq!(frame.appearance(0), 0.5);
        assert_eq!(frame.text(0), Some(text_handle()));
    }

    #[test]
    fn retained_text_scale_track_preserves_resource_identity() {
        let object = ObjectId::new(3);
        let handle = text_handle();
        let objects = [RetainedObjectDefinition::text(object, handle)];
        let tracks = [text_track(
            0,
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(0.0, 0.0),
            },
            0.0,
            2.0,
        )];
        let compiled = RetainedCompiledScene::compile(&objects, &tracks).unwrap();
        let mut runtime = RetainedSceneInstance::new(compiled);

        let midpoint = runtime.seek(1.0).unwrap();
        assert_eq!(midpoint.objects[0].transform.scale, Vec2::new(0.5, 0.5));
        assert_eq!(midpoint.text(0), Some(handle));
        assert_eq!(midpoint.render_geometry(0), None);

        let end = runtime.seek(2.0).unwrap();
        assert_eq!(end.objects[0].transform.scale, Vec2::ZERO);
        assert_eq!(end.text(0), Some(handle));
        assert_eq!(end.render_geometry(0), None);
    }

    #[test]
    fn retained_text_direct_seek_matches_incremental_playback() {
        let object = ObjectId::new(3);
        let objects = [RetainedObjectDefinition::text(object, text_handle())];
        let tracks = [text_track(
            0,
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, -2.0),
            },
            1.0,
            4.0,
        )];
        let compiled = RetainedCompiledScene::compile(&objects, &tracks).unwrap();
        let mut sequential = RetainedSceneInstance::new(compiled.clone());
        let mut direct = RetainedSceneInstance::new(compiled);

        for step in 1..=35 {
            sequential.advance_to(f64::from(step) * 0.1).unwrap();
        }
        direct.seek(3.5).unwrap();
        assert_eq!(sequential.frame(), direct.frame());
    }

    #[test]
    fn retained_presence_is_discrete_and_seekable() {
        let object = ObjectId::new(3);
        let objects = [RetainedObjectDefinition::text(object, text_handle())];
        let tracks = [TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        }];
        let compiled = RetainedCompiledScene::compile(&objects, &tracks).unwrap();
        let mut runtime = RetainedSceneInstance::new(compiled);

        assert!(!runtime.frame().is_present(0));
        assert!(!runtime.seek(0.5).unwrap().is_present(0));
        assert!(runtime.seek(1.0).unwrap().is_present(0));
    }

    #[test]
    fn retained_geometry_transform_matches_legacy_runtime() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let mut to = ObjectSnapshot::new(GeometryRef::circle(3.0));
        to.transform.translation = Vec2::new(2.0, 4.0);
        scene
            .animate_transform(
                object,
                from,
                to,
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            )
            .unwrap();

        let retained = RetainedCompiledScene::compile_legacy(&scene).unwrap();
        let legacy = CompiledScene::compile(&scene).unwrap();
        let mut retained_runtime = RetainedSceneInstance::new(retained);
        let mut legacy_runtime = SceneInstance::new(legacy);
        let retained_frame = retained_runtime.seek(1.0).unwrap();
        let legacy_frame = legacy_runtime.seek(1.0).unwrap();

        assert_eq!(
            retained_frame.objects[0].geometry(),
            Some(&legacy_frame.objects[0].geometry)
        );
        assert_eq!(
            retained_frame.objects[0].transform,
            legacy_frame.objects[0].transform
        );
        assert_eq!(
            retained_frame.objects[0].style,
            legacy_frame.objects[0].style
        );
        assert_eq!(retained_frame.morph(0), legacy_frame.morph(0));
        assert_eq!(
            retained_frame.render_geometry(0),
            Some(legacy_frame.render_geometry(0))
        );
    }
}

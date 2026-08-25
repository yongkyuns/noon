//! Deterministic renderer-free evaluation of compiled Noon scenes.

#![forbid(unsafe_code)]

mod execution_slots;
mod reactive;

pub use execution_slots::*;
pub use reactive::*;

use std::collections::BTreeMap;

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledScene, CompiledTrack, TransformGeometryPlan,
};
use noon_core::{
    Color, GeometryRef, ObjectId, ObjectSnapshot, Property, ScenePatch, Style, TrackValues,
    Transform2D, Vec2,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrameObjectState {
    pub id: ObjectId,
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameState {
    pub time: f64,
    pub objects: Vec<FrameObjectState>,
    pub presences: Vec<bool>,
    pub reveals: Vec<f32>,
    pub morphs: Vec<f32>,
    pub render_geometries: Vec<Option<GeometryRef>>,
}

impl FrameState {
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

    pub fn render_geometry(&self, object_index: usize) -> &GeometryRef {
        self.render_geometries[object_index]
            .as_ref()
            .unwrap_or(&self.objects[object_index].geometry)
    }
}

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
    channel: CompiledChannelKey,
    cursor: usize,
    mapped: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimePatchStats {
    pub channels_relowered: usize,
    pub scheduler_events_removed: usize,
    pub scheduler_events_inserted: usize,
    pub objects_recomputed: usize,
    pub groups_evaluated: usize,
    pub object_slots_appended: usize,
    pub object_slots_retired: usize,
    pub track_locators_removed: usize,
    pub full_group_rebuilds: usize,
    pub full_seeks: usize,
}

#[derive(Clone, Debug)]
pub struct SceneInstance {
    compiled: CompiledScene,
    frame: FrameState,
    groups: BTreeMap<CompiledChannelKey, TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
    last_patch_stats: RuntimePatchStats,
    changes: FrameChanges,
    reactive: Option<ReactiveRuntime>,
    last_reactive_stats: ReactiveRuntimeStats,
}

impl SceneInstance {
    pub fn new(compiled: CompiledScene) -> Self {
        let frame = base_frame(&compiled, 0.0);
        let groups = build_groups(&compiled);
        let timeline_scheduler = TimelineEventScheduler::from_compiled(&compiled);
        let mut instance = Self {
            compiled,
            frame,
            groups,
            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            last_patch_stats: RuntimePatchStats::default(),
            changes: FrameChanges::all(),
            reactive: None,
            last_reactive_stats: ReactiveRuntimeStats::default(),
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

    pub const fn last_patch_stats(&self) -> RuntimePatchStats {
        self.last_patch_stats
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.changes)
    }

    pub fn contains_object(&self, id: ObjectId) -> bool {
        self.compiled.object_index(id).is_some()
    }

    pub fn object_slot_is_live(&self, object_index: usize) -> bool {
        let Ok(object_index) = u32::try_from(object_index) else {
            return false;
        };
        self.compiled.object_slot_is_live(object_index)
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
        self.last_patch_stats = RuntimePatchStats::default();
        if matches!(
            patch,
            ScenePatch::SetTransform { .. } | ScenePatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            return Ok(&self.frame);
        }
        if matches!(
            patch,
            ScenePatch::AddTrack(_) | ScenePatch::ReplaceTrack(_) | ScenePatch::RemoveTrack(_)
        ) {
            self.apply_timeline_patch(patch)?;
            return Ok(&self.frame);
        }

        if matches!(
            patch,
            ScenePatch::CreateObject(_) | ScenePatch::RemoveObject(_)
        ) {
            self.apply_structural_patch(patch)?;
            return Ok(&self.frame);
        }

        unreachable!("all ScenePatch variants are handled above")
    }

    fn apply_structural_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        let removed = match patch {
            ScenePatch::RemoveObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                Some((object_index, self.compiled.object_channels(*object)))
            }
            ScenePatch::CreateObject(_) => None,
            _ => unreachable!("structural patch helper accepts only create/remove"),
        };

        let compiled_stats = self.compiled.apply_patch_with_stats(patch)?;
        let mut patch_stats = RuntimePatchStats {
            object_slots_appended: compiled_stats.object_slots_appended,
            object_slots_retired: compiled_stats.object_slots_retired,
            track_locators_removed: compiled_stats.track_locators_removed,
            ..RuntimePatchStats::default()
        };

        match patch {
            ScenePatch::CreateObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(object.id)
                    .expect("compiled create must expose its appended slot")
                    as usize;
                debug_assert_eq!(object_index, self.frame.objects.len());
                append_object_frame(&self.compiled, &mut self.frame, object_index);
                self.rebind_reactive_object(object.id, object_index);
                self.reapply_reactive_for_object(object_index);
                self.changes.insert(object_index);
            }
            ScenePatch::RemoveObject(_) => {
                let (object_index, old_channels) = removed.expect("remove context captured above");
                for channel in old_channels {
                    let scheduler_stats = self.timeline_scheduler.relower_channel(channel, &[]);
                    patch_stats.channels_relowered += scheduler_stats.groups_relowered;
                    patch_stats.scheduler_events_removed += scheduler_stats.events_removed;
                    patch_stats.scheduler_events_inserted += scheduler_stats.events_inserted;
                    self.groups.remove(&channel);
                }
                let object_index = object_index as usize;
                self.frame.presences[object_index] = false;
                self.frame.render_geometries[object_index] = None;
                self.changes.insert(object_index);
            }
            _ => unreachable!("structural patch helper accepts only create/remove"),
        }

        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = patch_stats;
        Ok(())
    }

    fn apply_timeline_patch(&mut self, patch: &ScenePatch) -> Result<(), CompilePatchError> {
        let old_channel = match patch {
            ScenePatch::ReplaceTrack(track) => self.compiled.channel_for_track(track.id),
            ScenePatch::RemoveTrack(id) => self.compiled.channel_for_track(*id),
            ScenePatch::AddTrack(_) => None,
            _ => unreachable!("timeline patch helper accepts only track mutations"),
        };
        self.compiled.apply_patch(patch)?;
        let new_channel = match patch {
            ScenePatch::AddTrack(track) | ScenePatch::ReplaceTrack(track) => {
                self.compiled.channel_for_track(track.id)
            }
            ScenePatch::RemoveTrack(_) => None,
            _ => unreachable!("timeline patch helper accepts only track mutations"),
        };

        let mut affected_channels = [None, None];
        push_unique_channel(&mut affected_channels, old_channel);
        push_unique_channel(&mut affected_channels, new_channel);
        let mut patch_stats = RuntimePatchStats::default();
        for channel in affected_channels.into_iter().flatten() {
            let tracks = self.compiled.channel_tracks(channel);
            let scheduler_stats = self.timeline_scheduler.relower_channel(channel, tracks);
            patch_stats.channels_relowered += scheduler_stats.groups_relowered;
            patch_stats.scheduler_events_removed += scheduler_stats.events_removed;
            patch_stats.scheduler_events_inserted += scheduler_stats.events_inserted;
            if tracks.is_empty() {
                self.groups.remove(&channel);
            } else {
                let mapped = tracks.iter().any(|track| !track.time_map.is_identity());
                self.groups
                    .entry(channel)
                    .and_modify(|group| group.mapped = mapped)
                    .or_insert(TrackGroup {
                        channel,
                        cursor: 0,
                        mapped,
                    });
            }
        }

        let mut affected_objects = [None, None];
        push_unique_object(
            &mut affected_objects,
            old_channel.map(|channel| channel.object_index as usize),
        );
        push_unique_object(
            &mut affected_objects,
            new_channel.map(|channel| channel.object_index as usize),
        );
        let mut evaluation = EvaluationStats::default();
        for object_index in affected_objects.into_iter().flatten() {
            self.relower_object(object_index, self.frame.time, &mut evaluation);
            self.reapply_reactive_for_object(object_index);
            self.changes.insert(object_index);
            patch_stats.objects_recomputed += 1;
        }
        patch_stats.groups_evaluated = evaluation.groups_evaluated;
        self.last_stats = evaluation;
        self.last_patch_stats = patch_stats;
        Ok(())
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
                self.reapply_properties(
                    index,
                    &[Property::Transform, Property::Position, Property::Rotation],
                );
            }
            ScenePatch::SetStyle { style, .. } => {
                self.frame.objects[index].style = *style;
                self.reapply_properties(index, &[Property::Transform, Property::Opacity]);
            }
            _ => unreachable!("value patch helper only accepts transform or style patches"),
        }
        self.reapply_reactive_for_object(index);
        if self.frame.objects[index] != before {
            self.changes.insert(index);
        }
        Ok(())
    }

    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let mut stats = EvaluationStats::default();
        for property in properties {
            let channel = CompiledChannelKey::new(object_index as u32, *property);
            let tracks = self.compiled.channel_tracks(channel);
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, tracks, group, time);
            stats.groups_evaluated += 1;
        }
        self.last_stats = stats;
    }

    fn relower_object(&mut self, object_index: usize, time: f64, stats: &mut EvaluationStats) {
        reset_object_frame(&self.compiled, &mut self.frame, object_index);
        for property in PROPERTY_ORDER {
            let channel = CompiledChannelKey::new(object_index as u32, property);
            let tracks = self.compiled.channel_tracks(channel);
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, tracks, group, time);
            stats.groups_evaluated += 1;
        }
    }

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.changes.invalidate_all();
        let mut stats = EvaluationStats::default();

        for group in self.groups.values_mut() {
            let tracks = self.compiled.channel_tracks(group.channel);
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(&mut self.frame, tracks, group, time);
            stats.groups_evaluated += 1;
        }
        self.timeline_scheduler.seek(time);

        self.reapply_reactive();
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
            if apply_group(&mut self.frame, tracks, group, time) {
                self.changes.insert(channel.object_index as usize);
            }
            stats.groups_evaluated += 1;
        }

        self.last_stats = stats;
    }
}

fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    let appearances = initial_scalar_property(
        compiled,
        compiled.objects().len(),
        Property::Appearance,
        1.0,
    );
    let objects: Vec<_> = compiled
        .objects()
        .iter()
        .enumerate()
        .map(|(index, object)| FrameObjectState {
            id: object.id,
            geometry: object.geometry.clone(),
            transform: object.base_transform,
            style: object.base_style,
            appearance: appearances[index],
        })
        .collect();
    let mut presences = initial_bool_property(compiled, objects.len(), Property::Presence, true);
    for (index, object) in compiled.objects().iter().enumerate() {
        if !object.live {
            presences[index] = false;
        }
    }
    FrameState {
        time,
        presences,
        reveals: initial_scalar_property(compiled, objects.len(), Property::Reveal, 1.0),
        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),
        render_geometries: vec![None; objects.len()],
        objects,
    }
}

fn initial_bool_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: bool,
) -> Vec<bool> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Bool { from, .. } = &track.values else {
            unreachable!("compiled bool property must contain bool values");
        };
        values[index] = *from;
        initialized[index] = true;
    }
    values
}

fn initial_scalar_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: f32,
) -> Vec<f32> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Scalar { from, .. } = &track.values else {
            unreachable!("compiled scalar property must contain scalar values");
        };
        values[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    values
}

const PROPERTY_ORDER: [Property; 8] = [
    Property::Presence,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Opacity,
    Property::Appearance,
    Property::Reveal,
    Property::Morph,
];

fn build_groups(compiled: &CompiledScene) -> BTreeMap<CompiledChannelKey, TrackGroup> {
    let mut groups = BTreeMap::new();
    for channel in compiled.channels() {
        let channel_tracks = compiled.channel_tracks(channel);
        let mapped = channel_tracks
            .iter()
            .any(|track| !track.time_map.is_identity());
        groups.insert(
            channel,
            TrackGroup {
                channel,
                cursor: 0,
                mapped,
            },
        );
    }
    groups
}

fn push_unique_channel(
    slots: &mut [Option<CompiledChannelKey>; 2],
    channel: Option<CompiledChannelKey>,
) {
    let Some(channel) = channel else {
        return;
    };
    if slots.iter().flatten().any(|existing| *existing == channel) {
        return;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(channel);
    }
}

fn push_unique_object(slots: &mut [Option<usize>; 2], object: Option<usize>) {
    let Some(object) = object else {
        return;
    };
    if slots.iter().flatten().any(|existing| *existing == object) {
        return;
    }
    if let Some(slot) = slots.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(object);
    }
}

fn append_object_frame(compiled: &CompiledScene, frame: &mut FrameState, object_index: usize) {
    debug_assert_eq!(object_index, frame.objects.len());
    let object = &compiled.objects()[object_index];
    debug_assert!(object.live);
    frame.objects.push(FrameObjectState {
        id: object.id,
        geometry: object.geometry.clone(),
        transform: object.base_transform,
        style: object.base_style,
        appearance: initial_channel_scalar(compiled, object_index, Property::Appearance, 1.0),
    });
    frame.presences.push(initial_channel_bool(
        compiled,
        object_index,
        Property::Presence,
        true,
    ));
    frame.reveals.push(initial_channel_scalar(
        compiled,
        object_index,
        Property::Reveal,
        1.0,
    ));
    frame.morphs.push(initial_channel_scalar(
        compiled,
        object_index,
        Property::Morph,
        0.0,
    ));
    frame.render_geometries.push(None);
}

fn reset_object_frame(compiled: &CompiledScene, frame: &mut FrameState, object_index: usize) {
    let object = &compiled.objects()[object_index];
    frame.objects[object_index] = FrameObjectState {
        id: object.id,
        geometry: object.geometry.clone(),
        transform: object.base_transform,
        style: object.base_style,
        appearance: initial_channel_scalar(compiled, object_index, Property::Appearance, 1.0),
    };
    frame.presences[object_index] =
        initial_channel_bool(compiled, object_index, Property::Presence, true);
    frame.reveals[object_index] =
        initial_channel_scalar(compiled, object_index, Property::Reveal, 1.0);
    frame.morphs[object_index] =
        initial_channel_scalar(compiled, object_index, Property::Morph, 0.0);
    frame.render_geometries[object_index] = None;
}

fn initial_channel_bool(
    compiled: &CompiledScene,
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

fn initial_channel_scalar(
    compiled: &CompiledScene,
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
    if group.channel.property == Property::Presence {
        let track = &tracks[group.cursor - 1];
        let TrackValues::Bool { to, .. } = &track.values else {
            unreachable!("compiled Presence track must contain bool values");
        };
        let object_index = group.channel.object_index as usize;
        let changed = frame.presences[object_index] != *to;
        frame.presences[object_index] = *to;
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

    let object_index = group.channel.object_index as usize;
    if group.channel.property == Property::Transform {
        return apply_transform_track(frame, object_index, track, progress);
    }
    let value = interpolate(track, progress);
    apply_evaluated_value(frame, object_index, group.channel.property, value)
}

fn apply_evaluated_value(
    frame: &mut FrameState,
    object_index: usize,
    property: Property,
    value: EvaluatedValue,
) -> bool {
    match (property, value) {
        (Property::Appearance, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let object = &mut frame.objects[object_index];
            let changed = object.appearance != value;
            object.appearance = value;
            changed
        }
        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.reveals[object_index] != value;
            frame.reveals[object_index] = value;
            changed
        }
        (Property::Morph, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = frame.morphs[object_index] != value;
            frame.morphs[object_index] = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            let object = &mut frame.objects[object_index];
            let changed = object.transform.translation != value;
            object.transform.translation = value;
            changed
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            let object = &mut frame.objects[object_index];
            let changed = object.transform.rotation != value;
            object.transform.rotation = value;
            changed
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            let object = &mut frame.objects[object_index];
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

fn apply_transform_track(
    frame: &mut FrameState,
    object_index: usize,
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
    let next_transform = interpolate_transform(from.transform, to.transform, progress);
    let next_style = interpolate_style(from.style, to.style, progress);
    let next_morph = if matches!(plan, TransformGeometryPlan::PathPair(_)) {
        progress
    } else {
        0.0
    };
    let next_render_geometry = match plan {
        TransformGeometryPlan::PathPair(prepared) => Some(prepared),
        _ => None,
    };

    let object = &mut frame.objects[object_index];
    let mut changed = apply_transform_geometry(&mut object.geometry, plan, from, to, progress);
    if object.transform != next_transform {
        object.transform = next_transform;
        changed = true;
    }
    if object.style != next_style {
        object.style = next_style;
        changed = true;
    }
    if frame.morphs[object_index] != next_morph {
        frame.morphs[object_index] = next_morph;
        changed = true;
    }
    changed |= set_optional_geometry_if_changed(
        &mut frame.render_geometries[object_index],
        next_render_geometry,
    );
    changed
}

fn apply_transform_geometry(
    current: &mut GeometryRef,
    plan: &TransformGeometryPlan,
    from: &ObjectSnapshot,
    to: &ObjectSnapshot,
    progress: f32,
) -> bool {
    match plan {
        TransformGeometryPlan::Static => set_geometry_if_changed(current, &from.geometry),
        TransformGeometryPlan::Circle {
            from_radius,
            to_radius,
        } => {
            let next = lerp(*from_radius, *to_radius, progress);
            match current {
                GeometryRef::Circle { radius } if *radius == next => false,
                GeometryRef::Circle { radius } => {
                    *radius = next;
                    true
                }
                _ => {
                    *current = GeometryRef::circle(next);
                    true
                }
            }
        }
        TransformGeometryPlan::Rectangle { from_size, to_size } => {
            let next = interpolate_vec2(*from_size, *to_size, progress);
            match current {
                GeometryRef::Rectangle { size } if *size == next => false,
                GeometryRef::Rectangle { size } => {
                    *size = next;
                    true
                }
                _ => {
                    *current = GeometryRef::rectangle(next.x, next.y);
                    true
                }
            }
        }
        TransformGeometryPlan::Line {
            from_start,
            from_end,
            to_start,
            to_end,
        } => {
            let next_start = interpolate_vec2(*from_start, *to_start, progress);
            let next_end = interpolate_vec2(*from_end, *to_end, progress);
            match current {
                GeometryRef::Line { start, end } if *start == next_start && *end == next_end => {
                    false
                }
                GeometryRef::Line { start, end } => {
                    *start = next_start;
                    *end = next_end;
                    true
                }
                _ => {
                    *current = GeometryRef::line(next_start, next_end);
                    true
                }
            }
        }
        TransformGeometryPlan::PathPair(_) => {
            let semantic_geometry = if progress >= 1.0 {
                &to.geometry
            } else {
                &from.geometry
            };
            set_geometry_if_changed(current, semantic_geometry)
        }
    }
}

fn interpolate_vec2(from: Vec2, to: Vec2, progress: f32) -> Vec2 {
    Vec2::new(lerp(from.x, to.x, progress), lerp(from.y, to.y, progress))
}

fn set_geometry_if_changed(current: &mut GeometryRef, next: &GeometryRef) -> bool {
    if current == next {
        return false;
    }
    current.clone_from(next);
    true
}

fn set_optional_geometry_if_changed(
    current: &mut Option<GeometryRef>,
    next: Option<&GeometryRef>,
) -> bool {
    match next {
        Some(next) if current.as_ref() == Some(next) => false,
        Some(next) => {
            if let Some(current) = current.as_mut() {
                current.clone_from(next);
            } else {
                *current = Some(next.clone());
            }
            true
        }
        None if current.is_some() => {
            *current = None;
            true
        }
        None => false,
    }
}

fn interpolate_transform(from: Transform2D, to: Transform2D, progress: f32) -> Transform2D {
    Transform2D {
        translation: Vec2::new(
            lerp(from.translation.x, to.translation.x, progress),
            lerp(from.translation.y, to.translation.y, progress),
        ),
        rotation: lerp(from.rotation, to.rotation, progress),
        scale: Vec2::new(
            lerp(from.scale.x, to.scale.x, progress),
            lerp(from.scale.y, to.scale.y, progress),
        ),
    }
}

fn interpolate_style(from: Style, to: Style, progress: f32) -> Style {
    Style {
        fill: interpolate_optional_color(from.fill, to.fill, progress),
        stroke: interpolate_optional_color(from.stroke, to.stroke, progress),
        stroke_width: lerp(from.stroke_width, to.stroke_width, progress),
        stroke_join: if progress >= 1.0 {
            to.stroke_join
        } else {
            from.stroke_join
        },
        stroke_cap: if progress >= 1.0 {
            to.stroke_cap
        } else {
            from.stroke_cap
        },
        opacity: lerp(from.opacity, to.opacity, progress),
    }
}

fn interpolate_optional_color(
    from: Option<Color>,
    to: Option<Color>,
    progress: f32,
) -> Option<Color> {
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    match (from, to) {
        (None, None) => None,
        (Some(from), Some(to)) => Some(interpolate_color(from, to, progress)),
        (None, Some(to)) => Some(interpolate_color(
            Color::rgba(to.red, to.green, to.blue, 0.0),
            to,
            progress,
        )),
        (Some(from), None) => Some(interpolate_color(
            from,
            Color::rgba(from.red, from.green, from.blue, 0.0),
            progress,
        )),
    }
}

fn interpolate_color(from: Color, to: Color, progress: f32) -> Color {
    Color::rgba(
        lerp(from.red, to.red, progress),
        lerp(from.green, to.green, progress),
        lerp(from.blue, to.blue, progress),
        lerp(from.alpha, to.alpha, progress),
    )
}

fn track_progress(track: &CompiledTrack, time: f64) -> f32 {
    debug_assert!(!track.property.is_instant());
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    track.timing.easing.evaluate(raw)
}

fn mapped_track_progress(track: &CompiledTrack, time: f64) -> Option<f32> {
    debug_assert!(!track.property.is_instant());
    if time < track.timing.start_time {
        return None;
    }
    let end = track.timing.start_time + track.timing.duration;
    // Scene state is defined after animation finish/cleanup at the exact endpoint.
    // This intentionally settles reversing group rates to the authored target,
    // matching Manim's `finish()` semantics and Noon's deterministic seek model.
    if time >= end {
        return Some(1.0);
    }
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    if track.time_map.is_identity() {
        return Some(track.timing.easing.evaluate(raw));
    }
    let sample = track.time_map.evaluate(raw);
    sample
        .begun
        .then(|| track.timing.easing.evaluate(sample.alpha))
}

fn interpolate(track: &CompiledTrack, progress: f32) -> EvaluatedValue {
    match &track.values {
        TrackValues::Scalar { from, to } => EvaluatedValue::Scalar(lerp(*from, *to, progress)),
        TrackValues::Vec2 { from, to } => EvaluatedValue::Vec2(Vec2::new(
            lerp(from.x, to.x, progress),
            lerp(from.y, to.y, progress),
        )),
        TrackValues::Bool { .. } => {
            unreachable!("Presence tracks are evaluated as discrete events")
        }
        TrackValues::Object { .. } => {
            unreachable!("Transform tracks are evaluated atomically")
        }
    }
}

const fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{
        Color, CompositionTimeMap, CompositionTimeMapStep, Easing, GeometryRef, Property,
        RateFunction, SceneDefinition, Style, TrackDefinition, TrackTiming,
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
    fn manim_smooth_rate_function_is_evaluated_by_runtime() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::new(10.0, 0.0),
                TrackTiming::new(0.0, 2.0, RateFunction::Smooth),
            )
            .expect("valid track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let quarter = instance.seek(0.5).expect("valid time").objects[0]
            .transform
            .translation
            .x;
        assert!((quarter - 0.7010372).abs() < 1e-5);
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .translation
                .x,
            5.0
        );
    }

    #[test]
    fn nonlinear_composition_time_map_is_evaluated_before_leaf_rate() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .add_track_with_time_map(
                object,
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(10.0, 0.0),
                },
                TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    0.0,
                    1.0,
                    RateFunction::Smooth,
                )]),
            )
            .unwrap();
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let quarter = instance.seek(0.5).unwrap().objects[0]
            .transform
            .translation
            .x;
        assert!((quarter - 0.7010372).abs() < 1e-5);
    }

    #[test]
    fn mapped_succession_selects_latest_virtual_child() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            scene
                .add_track_with_time_map(
                    object,
                    Property::Position,
                    TrackValues::Vec2 {
                        from: Vec2::new(from, 0.0),
                        to: Vec2::new(to, 0.0),
                    },
                    TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                    CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                        start,
                        0.5,
                        RateFunction::Linear,
                    )]),
                )
                .unwrap();
        }
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(
            instance.seek(0.5).unwrap().objects[0]
                .transform
                .translation
                .x,
            5.0
        );
        assert_eq!(
            instance.seek(1.25).unwrap().objects[0]
                .transform
                .translation
                .x,
            12.5
        );
        assert_eq!(
            instance.seek(2.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
    }

    #[test]
    fn reversing_composition_reopens_earlier_child_then_settles_at_finish() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            scene
                .add_track_with_time_map(
                    object,
                    Property::Position,
                    TrackValues::Vec2 {
                        from: Vec2::new(from, 0.0),
                        to: Vec2::new(to, 0.0),
                    },
                    TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                    CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                        start,
                        0.5,
                        RateFunction::ThereAndBack,
                    )]),
                )
                .unwrap();
        }
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        assert_eq!(
            instance.seek(1.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
        let reopened = instance.seek(1.6).unwrap().objects[0]
            .transform
            .translation
            .x;
        assert!(reopened > 0.0 && reopened < 10.0);
        assert_eq!(
            instance.seek(2.0).unwrap().objects[0]
                .transform
                .translation
                .x,
            20.0
        );
    }

    #[test]
    fn presence_events_are_discrete_and_direct_seek_matches_forward_playback() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .set_presence_at(object, false, true, 1.0)
            .expect("valid create event");
        scene
            .set_presence_at(object, true, false, 3.0)
            .expect("valid remove event");
        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let mut sequential = SceneInstance::new(compiled.clone());
        let mut direct = SceneInstance::new(compiled);
        assert!(!sequential.frame().is_present(0));
        sequential.advance_to(0.999).expect("valid time");
        assert!(!sequential.frame().is_present(0));
        sequential.advance_to(1.0).expect("valid time");
        assert!(sequential.frame().is_present(0));
        sequential.advance_to(2.0).expect("valid time");
        assert!(sequential.frame().is_present(0));
        sequential.advance_to(3.0).expect("valid time");
        assert!(!sequential.frame().is_present(0));
        direct.seek(3.0).expect("valid time");
        assert_eq!(sequential.frame(), direct.frame());
        direct.seek(2.0).expect("valid time");
        assert!(direct.frame().is_present(0));
        direct.seek(0.0).expect("valid time");
        assert!(!direct.frame().is_present(0));
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
    fn appearance_and_semantic_opacity_are_independent() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .object_mut(object)
            .expect("object exists")
            .style
            .opacity = 0.4;
        scene
            .animate_appearance(object, 1.0, 0.0, TrackTiming::new(0.0, 2.0, Easing::Linear))
            .expect("valid appearance track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let frame = instance.seek(1.0).expect("valid time");
        assert_eq!(frame.objects[0].style.opacity, 0.4);
        assert_eq!(frame.appearance(0), 0.5);
    }

    #[test]
    fn reveal_and_morph_progress_are_independent() {
        let source = noon_core::VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0));
        let target = noon_core::VectorPath::new()
            .move_to(Vec2::new(0.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0));
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::path(source.with_morph_target(target)));
        scene
            .animate_reveal(object, 0.0, 1.0, TrackTiming::new(0.0, 2.0, Easing::Linear))
            .expect("valid reveal track");
        scene
            .animate_morph(object, 0.0, 1.0, TrackTiming::new(0.0, 4.0, Easing::Linear))
            .expect("valid morph track");
        let mut instance =
            SceneInstance::new(CompiledScene::compile(&scene).expect("scene must compile"));
        let frame = instance.seek(1.0).expect("valid time");
        assert_eq!(frame.reveal(0), 0.5);
        assert_eq!(frame.morph(0), 0.25);
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
            time_map: CompositionTimeMap::identity(),
        };
        let track_patch = ScenePatch::ReplaceTrack(replacement);
        let style_patch = ScenePatch::SetStyle {
            object,
            style: Style {
                opacity: 0.75,
                stroke_join: noon_core::StrokeJoin::Round,
                stroke_cap: noon_core::StrokeCap::Round,
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
    fn adding_presence_event_live_reconciles_at_current_time() {
        let mut definition = SceneDefinition::new();
        let object = definition.add(GeometryRef::circle(1.0));
        let mut live =
            SceneInstance::new(CompiledScene::compile(&definition).expect("scene must compile"));
        live.seek(2.0).expect("valid time");
        assert!(live.frame().is_present(0));
        let presence = TrackDefinition {
            id: noon_core::TrackId::new(7),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        };
        let patch = ScenePatch::AddTrack(presence);
        live.apply_patch(&patch).expect("presence patch must apply");
        definition
            .apply_patch(patch)
            .expect("definition patch must apply");
        let mut expected = SceneInstance::new(
            CompiledScene::compile(&definition).expect("scene must compile after patch"),
        );
        expected.seek(2.0).expect("valid time");
        assert_eq!(live.frame(), expected.frame());
        assert!(!live.frame().is_present(0));
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
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
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
                    stroke_join: noon_core::StrokeJoin::Round,
                    stroke_cap: noon_core::StrokeCap::Round,
                    ..Style::default()
                },
            })
            .expect("valid patch");
        instance.advance_to(0.75).expect("valid time");
        assert_eq!(instance.take_frame_changes().object_indices(), &[0, 1]);
        assert!(instance.take_frame_changes().is_empty());
    }
}

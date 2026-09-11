//! Deterministic renderer-free evaluation of compiled Noon scenes.

#![forbid(unsafe_code)]

mod derived_display_evaluation;
mod execution_slots;
mod frame;
mod prepared_frame;
mod reactive;
mod renderer_publication;
mod spatial_index;

pub use derived_display_evaluation::*;
pub use execution_slots::*;
use frame::{frame_row_mut, EffectiveBoundsBasis, FrameRowMut, FrameRowState};
pub use frame::{EffectiveObjectProperties, FrameChanges, FrameObjectState, FrameState};
pub use prepared_frame::*;
pub use reactive::*;
pub use renderer_publication::*;
pub use spatial_index::*;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use noon_compile::{
    CompilePatchError, CompiledChannelKey, CompiledFamilyAnimationChannel, CompiledScene,
    CompiledTrack, ExecutionPatch, TransformGeometryPlan,
};
use noon_core::{
    continuous_time_map_interval, mapped_continuous_progress, FamilyAnimationState,
    PublicationContext, RetainedFamilyAnimationPlan, TrackTiming,
};
use noon_core::{
    Color, GeometryRef, ObjectId, PathCommand, Property, StrokeWidthMode, Style, TrackDefinition,
    TrackValues, Transform2D, TransformTrackEndpoint, Vec2, VectorPath,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationStats {
    pub groups_evaluated: usize,
    pub tracks_advanced: usize,
    pub binary_search_steps: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationError {
    InvalidTime(f64),
    NonMonotonicPreparedAdvance { current: f64, requested: f64 },
    FrameEpochExhausted(noon_core::FrameEpoch),
    RequiredCallbackPending,
    RequiredCallbackBarrier,
    Reactive(noon_core::ReactiveError),
}

impl std::fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTime(time) => write!(formatter, "invalid scene time {time}"),
            Self::NonMonotonicPreparedAdvance { current, requested } => write!(
                formatter,
                "required callback preparation cannot move backward from {current} to {requested}"
            ),
            Self::FrameEpochExhausted(epoch) => {
                write!(formatter, "frame epoch exhausted after {epoch:?}")
            }
            Self::RequiredCallbackPending => {
                formatter.write_str("a required callback publication is pending")
            }
            Self::RequiredCallbackBarrier => formatter
                .write_str("semantic host callbacks require callback-aware session advancement"),
            Self::Reactive(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EvaluationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reactive(error) => Some(error),
            _ => None,
        }
    }
}

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
    pub object_slots_reactivated: usize,
    pub object_slots_retired: usize,
    pub track_locators_removed: usize,
    pub full_group_rebuilds: usize,
    pub full_seeks: usize,
}

static NEXT_RUNTIME_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Process-local identity of one mutable runtime incarnation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeIdentity(u64);

impl RuntimeIdentity {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    fn fresh() -> Self {
        let raw = NEXT_RUNTIME_IDENTITY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("runtime identity space exhausted");
        Self(raw)
    }
}

#[derive(Debug)]
pub struct SceneInstance {
    identity: RuntimeIdentity,
    compiled: CompiledScene,
    frame: FrameState,
    painter_order: Vec<u32>,
    painter_ranks: Vec<Option<u32>>,
    groups: BTreeMap<CompiledChannelKey, TrackGroup>,
    timeline_scheduler: TimelineEventScheduler,
    last_stats: EvaluationStats,
    last_patch_stats: RuntimePatchStats,
    changes: FrameChanges,
    spatial_changes: FrameChanges,
    reactive: Option<ReactiveRuntime>,
    last_reactive_stats: ReactiveRuntimeStats,
    publication: PublicationContext,
    effective_driver_rows: BTreeSet<usize>,
    active_family_animation_indices: BTreeSet<usize>,
    pending_family_endpoint_expirations: BTreeMap<usize, usize>,
}

impl Clone for SceneInstance {
    fn clone(&self) -> Self {
        Self {
            identity: RuntimeIdentity::fresh(),
            compiled: self.compiled.clone(),
            frame: self.frame.clone(),
            painter_order: self.painter_order.clone(),
            painter_ranks: self.painter_ranks.clone(),
            groups: self.groups.clone(),
            timeline_scheduler: self.timeline_scheduler.clone(),
            last_stats: self.last_stats,
            last_patch_stats: self.last_patch_stats,
            changes: self.changes.clone(),
            spatial_changes: self.spatial_changes.clone(),
            reactive: self.reactive.clone(),
            last_reactive_stats: self.last_reactive_stats,
            publication: self.publication,
            effective_driver_rows: self.effective_driver_rows.clone(),
            active_family_animation_indices: self.active_family_animation_indices.clone(),
            pending_family_endpoint_expirations: self.pending_family_endpoint_expirations.clone(),
        }
    }
}

impl SceneInstance {
    pub fn preflight_reconcilable_track_additions(
        &self,
        tracks: &[TrackDefinition],
    ) -> Result<(), CompilePatchError> {
        self.compiled.preflight_reconcilable_track_additions(tracks)
    }

    pub fn new(compiled: CompiledScene) -> Self {
        let frame = base_frame(&compiled, 0.0);
        let groups = build_groups(&compiled);
        let timeline_scheduler = TimelineEventScheduler::from_compiled(&compiled);
        let mut instance = Self {
            identity: RuntimeIdentity::fresh(),
            painter_order: compiled.painter_order().to_vec(),
            painter_ranks: (0..compiled.objects().len())
                .map(|index| compiled.painter_rank(index as u32))
                .collect(),
            compiled,
            frame,
            groups,
            timeline_scheduler,
            last_stats: EvaluationStats::default(),
            last_patch_stats: RuntimePatchStats::default(),
            changes: FrameChanges::all(),
            spatial_changes: FrameChanges::all(),
            reactive: None,
            last_reactive_stats: ReactiveRuntimeStats::default(),
            publication: PublicationContext::default(),
            effective_driver_rows: BTreeSet::new(),
            active_family_animation_indices: BTreeSet::new(),
            pending_family_endpoint_expirations: BTreeMap::new(),
        };
        instance.seek_unchecked(0.0);
        instance
    }

    pub const fn runtime_identity(&self) -> RuntimeIdentity {
        self.identity
    }

    pub fn frame(&self) -> &FrameState {
        &self.frame
    }

    pub fn planned_family_frame(&self) -> RetainedPlannedFamilyFrame<'_> {
        RetainedPlannedFamilyFrame {
            retained: &self.frame,
            family_animations: &self.frame.family_animations,
            family_plan_indices: &self.frame.family_animation_plan_indices,
        }
    }

    pub fn family_animation_plans(&self) -> &[RetainedFamilyAnimationPlan] {
        self.compiled.family_animation_plans()
    }

    pub fn active_family_animation_indices(&self) -> &BTreeSet<usize> {
        &self.active_family_animation_indices
    }

    pub fn effective_properties_at(
        &self,
        object_index: usize,
        cached_bounds: Option<noon_core::Rect>,
    ) -> Option<EffectiveObjectProperties> {
        self.object_slot_is_live(object_index).then(|| {
            EffectiveObjectProperties::from_frame(&self.frame, object_index, cached_bounds)
        })
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

    /// Borrow accumulated renderer invalidation without consuming it.
    ///
    /// This is intended for bounded publication diagnostics. Normal renderer
    /// preparation continues to consume changes through `take_renderer_publication`.
    pub const fn frame_changes(&self) -> &FrameChanges {
        &self.changes
    }

    /// Consume one renderer publication atomically with its accumulated changes.
    pub fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        let changes = self.take_frame_changes();
        RendererPublication::new(
            self.publication,
            &self.frame,
            changes,
            self.compiled.text_resources(),
            self.compiled.font_resources(),
            self.compiled.geometry_resources(),
            self.compiled.family_animation_plans(),
            &self.active_family_animation_indices,
            &self.painter_order,
        )
    }

    /// Consume one renderer publication while queueing a renderer-only full
    /// invalidation for the immediately following publication.
    ///
    /// This supports transient overlays whose exact endpoint must be presented once
    /// and then removed on the next redraw without mutating scene or spatial state.
    pub fn take_renderer_publication_with_followup_invalidation(
        &mut self,
    ) -> RendererPublication<'_> {
        let changes = self.take_frame_changes();
        self.changes.invalidate_all();
        RendererPublication::new(
            self.publication,
            &self.frame,
            changes,
            self.compiled.text_resources(),
            self.compiled.font_resources(),
            self.compiled.geometry_resources(),
            self.compiled.family_animation_plans(),
            &self.active_family_animation_indices,
            &self.painter_order,
        )
    }

    /// Consume derived spatial invalidation for the execution-session index owner.
    pub fn take_spatial_changes(&mut self) -> FrameChanges {
        std::mem::take(&mut self.spatial_changes)
    }

    pub(crate) fn mark_changed(&mut self, object_index: usize) {
        self.changes.insert(object_index);
        self.spatial_changes.insert(object_index);
    }

    pub(crate) fn mark_added(&mut self, object_index: usize) {
        self.changes.insert_added(object_index);
        self.spatial_changes.insert_added(object_index);
    }

    pub(crate) fn mark_removed(&mut self, object_index: usize) {
        self.changes.insert_removed(object_index);
        self.spatial_changes.insert_removed(object_index);
    }

    pub(crate) fn mark_painter_order_changed(&mut self, range: std::ops::Range<usize>) {
        self.changes.insert_painter_order_range(range.clone());
        self.spatial_changes.insert_painter_order_range(range);
    }

    pub(crate) fn mark_all_changed(&mut self) {
        self.changes.invalidate_all();
        self.spatial_changes.invalidate_all();
    }

    pub fn contains_object(&self, id: ObjectId) -> bool {
        self.compiled.object_index(id).is_some()
    }

    pub fn frame_index_for_object(&self, id: ObjectId) -> Option<usize> {
        self.compiled.object_index(id).map(|index| index as usize)
    }

    pub fn painter_order(&self) -> &[u32] {
        &self.painter_order
    }

    pub fn painter_rank(&self, object_index: usize) -> Option<u32> {
        self.painter_ranks.get(object_index).copied().flatten()
    }

    pub fn object_has_effective_driver(&self, id: ObjectId) -> bool {
        self.frame_index_for_object(id)
            .is_some_and(|index| self.effective_driver_rows.contains(&index))
    }

    pub fn text_resources(&self) -> &impl noon_core::TextResourceLookup {
        self.compiled.text_resources()
    }

    pub fn font_resources(&self) -> &impl noon_core::FontResourceLookup {
        self.compiled.font_resources()
    }

    pub fn geometry_resources(&self) -> &impl noon_core::GeometryResourceLookup {
        self.compiled.geometry_resources()
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
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    pub fn seek(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        let previous_time = self.frame.time;
        self.seek_unchecked(time);
        if self.frame.time != previous_time {
            self.publish_effective_change();
        }
        Ok(&self.frame)
    }

    pub fn advance_to(&mut self, time: f64) -> Result<&FrameState, EvaluationError> {
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        let previous_time = self.frame.time;
        if time < previous_time {
            self.seek_unchecked(time);
            if self.frame.time != previous_time {
                self.publish_effective_change();
            }
        } else {
            self.advance_unchecked(time);
        }
        Ok(&self.frame)
    }

    pub fn apply_execution_patch(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<&FrameState, CompilePatchError> {
        if !self.compiled.patch_changes_execution(patch) {
            self.last_patch_stats = RuntimePatchStats::default();
            return Ok(&self.frame);
        }
        self.apply_patch_unpublished(patch)?;
        self.publish_execution_change();
        Ok(&self.frame)
    }

    fn apply_patch_unpublished(
        &mut self,
        patch: &ExecutionPatch,
    ) -> Result<&FrameState, CompilePatchError> {
        self.last_patch_stats = RuntimePatchStats::default();
        if matches!(
            patch,
            ExecutionPatch::SetContent { .. }
                | ExecutionPatch::SetTransform { .. }
                | ExecutionPatch::SetStyle { .. }
        ) {
            self.apply_value_patch(patch)?;
            return Ok(&self.frame);
        }
        if matches!(
            patch,
            ExecutionPatch::AddTrack(_)
                | ExecutionPatch::AddFamilyAnimation(_)
                | ExecutionPatch::ReplaceTrack(_)
                | ExecutionPatch::RemoveTrack(_)
                | ExecutionPatch::ReconcileTrack { .. }
        ) {
            self.apply_timeline_patch(patch)?;
            return Ok(&self.frame);
        }

        if matches!(
            patch,
            ExecutionPatch::CreateObject(_)
                | ExecutionPatch::RemoveObject(_)
                | ExecutionPatch::ReorderObject { .. }
                | ExecutionPatch::SetZIndex { .. }
        ) {
            self.apply_structural_patch(patch)?;
            return Ok(&self.frame);
        }

        unreachable!("all ExecutionPatch variants are handled above")
    }

    fn apply_structural_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        let previous_order_len = self.painter_order.len();
        let previous_order_position = match patch {
            ExecutionPatch::RemoveObject(object)
            | ExecutionPatch::ReorderObject { object, .. }
            | ExecutionPatch::SetZIndex { object, .. } => self
                .compiled
                .object_index(*object)
                .and_then(|index| self.painter_rank(index as usize))
                .map(|rank| rank as usize),
            ExecutionPatch::CreateObject(_) => None,
            _ => unreachable!("structural patch helper accepts only create/remove/reorder"),
        };
        let removed = match patch {
            ExecutionPatch::RemoveObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(*object)
                    .ok_or(CompilePatchError::UnknownObject(*object))?;
                Some((object_index, self.compiled.object_channels(*object)))
            }
            ExecutionPatch::CreateObject(_) => None,
            ExecutionPatch::ReorderObject { .. } | ExecutionPatch::SetZIndex { .. } => None,
            _ => unreachable!("structural patch helper accepts only create/remove"),
        };

        let compiled_stats = self.compiled.apply_execution_patch_with_stats(patch)?;
        let mut patch_stats = RuntimePatchStats {
            object_slots_appended: compiled_stats.object_slots_appended,
            object_slots_reactivated: compiled_stats.object_slots_reactivated,
            object_slots_retired: compiled_stats.object_slots_retired,
            track_locators_removed: compiled_stats.track_locators_removed,
            ..RuntimePatchStats::default()
        };

        match patch {
            ExecutionPatch::CreateObject(object) => {
                let object_index = self
                    .compiled
                    .object_index(object.id)
                    .expect("compiled create must expose its appended slot")
                    as usize;
                if compiled_stats.object_slots_reactivated == 1 {
                    let time = self.frame.time;
                    reset_object_frame(&self.compiled, &mut self.frame, object_index, time);
                    self.clear_family_animation_runtime_state(object_index);
                    self.mark_added(object_index);
                } else {
                    debug_assert_eq!(object_index, self.frame.objects.len());
                    append_object_frame(&self.compiled, &mut self.frame, object_index);
                    self.mark_added(object_index);
                }
                self.painter_ranks.resize(self.frame.objects.len(), None);
                self.painter_ranks[object_index] = Some(self.painter_order.len() as u32);
                self.painter_order.push(object_index as u32);
                self.reposition_painter_row(object_index);
                self.rebind_reactive_object(object.id, object_index);
                self.reapply_reactive_for_object(object_index);
            }
            ExecutionPatch::RemoveObject(_) => {
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
                self.frame.render_transforms[object_index] = None;
                self.clear_family_animation_runtime_state(object_index);
                let position = self.painter_ranks[object_index]
                    .take()
                    .expect("removed row was live") as usize;
                self.painter_order.remove(position);
                for rank in position..self.painter_order.len() {
                    self.painter_ranks[self.painter_order[rank] as usize] = Some(rank as u32);
                }
                self.mark_removed(object_index);
            }
            ExecutionPatch::ReorderObject { object, .. } => {
                let index = self
                    .compiled
                    .object_index(*object)
                    .expect("live reordered row") as usize;
                self.reposition_painter_row(index);
            }
            ExecutionPatch::SetZIndex { object, .. } => {
                let index = self
                    .compiled
                    .object_index(*object)
                    .expect("live priority row") as usize;
                self.frame.objects[index].z_index = initial_z_index(&self.compiled, index);
                self.reapply_properties(index, &[Property::ZIndex]);
            }
            _ => unreachable!("structural patch helper accepts only create/remove"),
        }

        let next_position = match patch {
            ExecutionPatch::CreateObject(object) => self
                .compiled
                .object_index(object.id)
                .and_then(|index| self.painter_rank(index as usize))
                .map(|rank| rank as usize),
            ExecutionPatch::ReorderObject { object, .. }
            | ExecutionPatch::SetZIndex { object, .. } => self
                .compiled
                .object_index(*object)
                .and_then(|index| self.painter_rank(index as usize))
                .map(|rank| rank as usize),
            ExecutionPatch::RemoveObject(_) => None,
            _ => unreachable!("structural patch helper accepts only create/remove/reorder"),
        };
        if previous_order_position != next_position
            || previous_order_len != self.painter_order.len()
        {
            let first = previous_order_position
                .into_iter()
                .chain(next_position)
                .min()
                .unwrap_or(previous_order_len.min(self.painter_order.len()));
            let end = if previous_order_len == self.painter_order.len() {
                previous_order_position
                    .into_iter()
                    .chain(next_position)
                    .max()
                    .map_or(first, |position| position + 1)
            } else {
                previous_order_len.max(self.painter_order.len())
            };
            self.mark_painter_order_changed(first..end);
        }

        self.last_stats = EvaluationStats::default();
        self.last_patch_stats = patch_stats;
        Ok(())
    }

    fn apply_timeline_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        if matches!(patch, ExecutionPatch::AddFamilyAnimation(_)) {
            let animation_index = self.compiled.family_animations().len();
            self.compiled.apply_execution_patch(patch)?;
            let animation = self.compiled.family_animations()[animation_index].clone();
            let (start_time, end_time) = family_animation_interval(&animation);
            let scheduler_stats = self.timeline_scheduler.append_family_animation(
                animation_index,
                start_time,
                end_time,
            );
            self.update_family_animation(animation_index, self.frame.time);
            self.last_stats = EvaluationStats::default();
            self.last_patch_stats = RuntimePatchStats {
                channels_relowered: scheduler_stats.groups_relowered,
                scheduler_events_removed: scheduler_stats.events_removed,
                scheduler_events_inserted: scheduler_stats.events_inserted,
                ..RuntimePatchStats::default()
            };
            return Ok(());
        }
        if let ExecutionPatch::ReconcileTrack { track, .. } = patch {
            let channel = self
                .compiled
                .channel_for_track(*track)
                .ok_or(CompilePatchError::UnknownTrack(*track))?;
            self.compiled.apply_execution_patch(patch)?;
            let mut evaluation = EvaluationStats::default();
            self.relower_object(
                channel.object_index as usize,
                self.frame.time,
                &mut evaluation,
            );
            self.reapply_reactive_for_object(channel.object_index as usize);
            self.last_stats = evaluation;
            self.last_patch_stats = RuntimePatchStats {
                objects_recomputed: 1,
                groups_evaluated: evaluation.groups_evaluated,
                ..RuntimePatchStats::default()
            };
            self.mark_changed(channel.object_index as usize);
            return Ok(());
        }
        let old_channel = match patch {
            ExecutionPatch::ReplaceTrack(track) => self.compiled.channel_for_track(track.id),
            ExecutionPatch::RemoveTrack(id) => self.compiled.channel_for_track(*id),
            ExecutionPatch::AddTrack(_) => None,
            ExecutionPatch::AddFamilyAnimation(_) => unreachable!("handled above"),
            ExecutionPatch::ReconcileTrack { .. } => unreachable!("handled above"),
            _ => unreachable!("timeline patch helper accepts only track mutations"),
        };
        self.compiled.apply_execution_patch(patch)?;
        let new_channel = match patch {
            ExecutionPatch::AddTrack(track) | ExecutionPatch::ReplaceTrack(track) => {
                self.compiled.channel_for_track(track.id)
            }
            ExecutionPatch::RemoveTrack(_) => None,
            ExecutionPatch::AddFamilyAnimation(_) => unreachable!("handled above"),
            ExecutionPatch::ReconcileTrack { .. } => unreachable!("handled above"),
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
            self.mark_changed(object_index);
            patch_stats.objects_recomputed += 1;
        }
        patch_stats.groups_evaluated = evaluation.groups_evaluated;
        self.last_stats = evaluation;
        self.last_patch_stats = patch_stats;
        Ok(())
    }

    fn apply_value_patch(&mut self, patch: &ExecutionPatch) -> Result<(), CompilePatchError> {
        let object = match patch {
            ExecutionPatch::SetContent { object, .. }
            | ExecutionPatch::SetTransform { object, .. }
            | ExecutionPatch::SetStyle { object, .. } => *object,
            _ => unreachable!("value patch helper only accepts object-local property patches"),
        };
        let index = self
            .compiled
            .object_index(object)
            .ok_or(CompilePatchError::UnknownObject(object))? as usize;
        let before = self.frame.objects[index].clone();
        self.compiled.apply_execution_patch(patch)?;

        match patch {
            ExecutionPatch::SetContent {
                content,
                text_bounds,
                ..
            } => {
                self.frame.objects[index].content = content.clone();
                self.frame.objects[index].text_bounds = *text_bounds;
                // Host callbacks run after ordinary timeline/reactive evaluation for the frame.
                // Clearing a transient render override makes authored content authoritative for
                // this phase without rebuilding unrelated runtime slots.
                self.frame.render_geometries[index] = None;
                self.frame.render_transforms[index] = None;
            }
            ExecutionPatch::SetTransform { transform, .. } => {
                self.frame.release_render_transform(index);
                self.frame.objects[index].transform = *transform;
                self.reapply_properties(
                    index,
                    &[
                        Property::Transform,
                        Property::Position,
                        Property::Rotation,
                        Property::Scale,
                    ],
                );
            }
            ExecutionPatch::SetStyle { style, .. } => {
                self.frame.objects[index].style = *style;
                self.reapply_properties(
                    index,
                    &[
                        Property::Transform,
                        Property::Fill,
                        Property::Stroke,
                        Property::StrokeWidth,
                        Property::Opacity,
                    ],
                );
            }
            _ => unreachable!("value patch helper only accepts object-local property patches"),
        }
        self.reapply_reactive_for_object(index);
        if self.frame.objects[index] != before {
            self.mark_changed(index);
        }
        Ok(())
    }

    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {
        let time = self.frame.time;
        let mut stats = EvaluationStats::default();
        for property in properties {
            let channel = CompiledChannelKey::new(object_index as u32, *property);
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[object_index];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
        if properties.contains(&Property::ZIndex) {
            self.reposition_painter_row(object_index);
        }
        self.last_stats = stats;
    }

    fn relower_object(&mut self, object_index: usize, time: f64, stats: &mut EvaluationStats) {
        reset_object_frame(&self.compiled, &mut self.frame, object_index, time);
        for property in PROPERTY_ORDER {
            let channel = CompiledChannelKey::new(object_index as u32, property);
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[object_index];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
        self.reposition_painter_row(object_index);
    }

    fn rebuild_painter_order(&mut self) {
        self.painter_order = self.compiled.painter_order().to_vec();
        self.painter_order.sort_by(|&a, &b| {
            self.frame.objects[a as usize]
                .z_index
                .partial_cmp(&self.frame.objects[b as usize].z_index)
                .expect("finite priorities")
                .then_with(|| {
                    self.compiled
                        .family_rank(a)
                        .cmp(&self.compiled.family_rank(b))
                })
        });
        self.painter_ranks.resize(self.frame.objects.len(), None);
        self.painter_ranks.fill(None);
        for (rank, &index) in self.painter_order.iter().enumerate() {
            self.painter_ranks[index as usize] = Some(rank as u32);
        }
    }

    fn reposition_painter_row(&mut self, index: usize) {
        let range = noon_compile::order_index::reposition_order_row(
            &mut self.painter_order,
            &mut self.painter_ranks,
            index as u32,
            |a, b| {
                self.frame.objects[a as usize]
                    .z_index
                    .partial_cmp(&self.frame.objects[b as usize].z_index)
                    .expect("finite priorities")
                    .then_with(|| {
                        self.compiled
                            .family_rank(a)
                            .cmp(&self.compiled.family_rank(b))
                    })
            },
        );
        if !range.is_empty() {
            self.mark_painter_order_changed(range);
        }
    }

    fn seek_unchecked(&mut self, time: f64) {
        self.frame = base_frame(&self.compiled, time);
        self.effective_driver_rows.clear();
        self.active_family_animation_indices.clear();
        self.pending_family_endpoint_expirations.clear();
        self.mark_all_changed();
        let mut stats = EvaluationStats::default();

        for group in self.groups.values_mut() {
            let tracks = self.compiled.channel_tracks(group.channel);
            let object = &self.compiled.objects()[group.channel.object_index as usize];
            group.cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
            apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            );
            stats.groups_evaluated += 1;
        }
        self.rebuild_painter_order();
        self.timeline_scheduler.seek(time);
        for animation_index in 0..self.compiled.family_animations().len() {
            self.update_family_animation(animation_index, time);
        }

        self.reapply_reactive();
        self.last_stats = stats;
    }

    fn advance_unchecked(&mut self, time: f64) {
        if !self.effective_driver_rows.is_empty() {
            let prepared = self
                .prepare_advance_to(time)
                .expect("unchecked forward evaluation receives a valid monotonic time");
            let effective = self
                .prepare_effective_property_batch(&[])
                .expect("empty effective batch is valid");
            self.commit_prepared_frame(prepared, effective)
                .expect("immediate evaluation cannot stale its own prepared frame");
            return;
        }

        let previous_time = self.frame.time;
        self.frame.time = time;
        let requested_count = self.timeline_scheduler.advance(time);
        let mut stats = EvaluationStats::default();

        for request_index in 0..requested_count {
            let channel = self.timeline_scheduler.requested()[request_index];
            let tracks = self.compiled.channel_tracks(channel);
            let object = &self.compiled.objects()[channel.object_index as usize];
            let Some(group) = self.groups.get_mut(&channel) else {
                continue;
            };
            while group.cursor < tracks.len() && tracks[group.cursor].timing.start_time <= time {
                group.cursor += 1;
                stats.tracks_advanced += 1;
            }
            if apply_group(
                &self.compiled,
                &mut self.frame,
                tracks,
                group,
                time,
                object.base_transform,
                object.base_style,
            ) {
                if channel.property == Property::ZIndex {
                    self.reposition_painter_row(channel.object_index as usize);
                }
                self.mark_changed(channel.object_index as usize);
            }
            stats.groups_evaluated += 1;
        }
        self.update_requested_family_animations(time);

        self.last_stats = stats;
        if self.frame.time != previous_time {
            self.publish_effective_change();
        }
    }

    fn update_requested_family_animations(&mut self, time: f64) -> bool {
        // End events leave one exact endpoint publication active for lifecycle
        // reconciliation. Revisit only those crossed endpoint channels on the next
        // tick so they expire without scanning historical family animations.
        let mut requested = std::mem::take(&mut self.pending_family_endpoint_expirations)
            .into_values()
            .collect::<BTreeSet<_>>();
        requested.extend(
            self.timeline_scheduler
                .requested_family_animations()
                .iter()
                .copied(),
        );
        let mut changed = false;
        for animation_index in requested {
            changed = self.update_family_animation(animation_index, time) || changed;
        }
        changed
    }

    fn clear_family_animation_runtime_state(&mut self, object_index: usize) {
        self.frame.family_animations[object_index] = None;
        self.frame.family_animation_plan_indices[object_index] = None;
        self.active_family_animation_indices.remove(&object_index);
        self.pending_family_endpoint_expirations
            .remove(&object_index);
    }

    fn update_family_animation(&mut self, animation_index: usize, time: f64) -> bool {
        let animation = self.compiled.family_animations()[animation_index].clone();
        let object_index = animation.object_index as usize;
        if let Some(state) = family_state_at(&animation, time) {
            let (_, end_time) = family_animation_interval(&animation);
            if time == end_time {
                self.pending_family_endpoint_expirations
                    .insert(object_index, animation_index);
            } else {
                self.pending_family_endpoint_expirations
                    .remove(&object_index);
            }
            return self.set_family_animation(animation_index, state);
        }
        if self.pending_family_endpoint_expirations.get(&object_index) == Some(&animation_index) {
            self.pending_family_endpoint_expirations
                .remove(&object_index);
        }
        if self.frame.family_animation_plan_indices[object_index] != Some(animation.plan_index) {
            return false;
        }
        self.frame.family_animations[object_index] = None;
        self.frame.family_animation_plan_indices[object_index] = None;
        self.active_family_animation_indices.remove(&object_index);
        self.mark_changed(object_index);
        true
    }

    fn set_family_animation(
        &mut self,
        animation_index: usize,
        state: FamilyAnimationState,
    ) -> bool {
        let animation = &self.compiled.family_animations()[animation_index];
        let object_index = animation.object_index as usize;
        if self.frame.family_animations[object_index] == Some(state)
            && self.frame.family_animation_plan_indices[object_index] == Some(animation.plan_index)
        {
            return false;
        }
        self.frame.family_animations[object_index] = Some(state);
        self.frame.family_animation_plan_indices[object_index] = Some(animation.plan_index);
        self.active_family_animation_indices.insert(object_index);
        self.mark_changed(object_index);
        true
    }
}

fn family_animation_timing(animation: &CompiledFamilyAnimationChannel) -> TrackTiming {
    TrackTiming::new(
        animation.spec.start_time,
        animation.spec.duration,
        noon_core::RateFunction::Linear,
    )
}

fn family_animation_interval(animation: &CompiledFamilyAnimationChannel) -> (f64, f64) {
    continuous_time_map_interval(family_animation_timing(animation), &animation.time_map)
        .expect("compiled family animation retains a validated continuous time map")
}

fn family_state_at(
    animation: &CompiledFamilyAnimationChannel,
    time: f64,
) -> Option<FamilyAnimationState> {
    let (start_time, end_time) = family_animation_interval(animation);
    // Keep the exact endpoint available for one coherent publication. Completion
    // reconciliation applies lifecycle removals after that publication; expiring
    // here would briefly expose the object's canonical state between an Unwrite
    // endpoint and its atomic removal.
    if time < start_time || time > end_time {
        return None;
    }
    let progress = mapped_continuous_progress(
        family_animation_timing(animation),
        &animation.time_map,
        time,
    )?;
    animation
        .spec
        .state_at(animation.spec.start_time + f64::from(progress) * animation.spec.duration)
        .ok()
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
            z_index: initial_z_index(compiled, index),
            id: object.id,
            content: object.content.clone(),
            text_bounds: object.text_bounds,
            transform: affine_base_at_time(compiled, index, object.base_transform, time),
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
        render_transforms: vec![None; objects.len()],
        family_animations: vec![None; objects.len()],
        family_animation_plan_indices: vec![None; objects.len()],
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
        let (TrackValues::Scalar { from, .. } | TrackValues::PreparedMorph { from, .. }) =
            &track.values
        else {
            unreachable!("compiled scalar property must contain scalar values");
        };
        values[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    values
}

const PROPERTY_ORDER: [Property; 13] = [
    Property::Presence,
    Property::ZIndex,
    Property::Transform,
    Property::Position,
    Property::Rotation,
    Property::Scale,
    Property::Fill,
    Property::Stroke,
    Property::StrokeWidth,
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
        z_index: initial_z_index(compiled, object_index),
        id: object.id,
        content: object.content.clone(),
        text_bounds: object.text_bounds,
        transform: affine_base_at_time(compiled, object_index, object.base_transform, frame.time),
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
    frame.render_transforms.push(None);
    frame.family_animations.push(None);
    frame.family_animation_plan_indices.push(None);
}

fn reset_object_frame(
    compiled: &CompiledScene,
    frame: &mut FrameState,
    object_index: usize,
    time: f64,
) {
    let object = &compiled.objects()[object_index];
    frame.objects[object_index] = FrameObjectState {
        z_index: initial_z_index(compiled, object_index),
        id: object.id,
        content: object.content.clone(),
        text_bounds: object.text_bounds,
        transform: affine_base_at_time(compiled, object_index, object.base_transform, time),
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
    frame.render_transforms[object_index] = None;
}

fn initial_z_index(compiled: &CompiledScene, object_index: usize) -> f64 {
    let channel = CompiledChannelKey::new(object_index as u32, Property::ZIndex);
    match compiled
        .channel_tracks(channel)
        .first()
        .map(|track| &track.values)
    {
        Some(TrackValues::ZIndex { from, .. }) => *from,
        None => compiled.objects()[object_index].base_z_index,
        _ => unreachable!("priority channel must contain exact priority values"),
    }
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
    let (TrackValues::Scalar { from, .. } | TrackValues::PreparedMorph { from, .. }) =
        &track.values
    else {
        unreachable!("compiled scalar property must contain scalar values");
    };
    from.clamp(0.0, 1.0)
}

fn affine_base_at_time(
    compiled: &CompiledScene,
    object_index: usize,
    mut transform: Transform2D,
    time: f64,
) -> Transform2D {
    for property in [Property::Position, Property::Rotation, Property::Scale] {
        let channel = CompiledChannelKey::new(object_index as u32, property);
        let tracks = compiled.channel_tracks(channel);
        let Some(track) = tracks.first() else {
            continue;
        };
        if tracks.last().is_some_and(|last| {
            last.reconciled && time >= last.timing.start_time + last.timing.duration
        }) {
            continue;
        }
        match (property, &track.values) {
            (Property::Position, TrackValues::Vec2 { from, .. }) => transform.translation = *from,
            (Property::Rotation, TrackValues::Scalar { from, .. }) => transform.rotation = *from,
            (Property::Scale, TrackValues::Vec2 { from, .. }) => transform.scale = *from,
            _ => unreachable!("validated affine track must carry matching values"),
        }
    }
    transform
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
    compiled: &CompiledScene,
    frame: &mut FrameState,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
    base_transform: Transform2D,
    base_style: Style,
) -> bool {
    let object_index = group.channel.object_index as usize;
    apply_group_to_row(
        compiled,
        frame_row_mut(frame, object_index),
        tracks,
        group,
        time,
        base_transform,
        base_style,
    )
}

fn apply_group_to_row(
    compiled: &CompiledScene,
    mut row: FrameRowMut<'_>,
    tracks: &[CompiledTrack],
    group: &TrackGroup,
    time: f64,
    base_transform: Transform2D,
    base_style: Style,
) -> bool {
    if group.cursor == 0 {
        return false;
    }
    if group.channel.property == Property::ZIndex {
        let TrackValues::ZIndex { to, .. } = &tracks[group.cursor - 1].values else {
            unreachable!("priority channel must contain exact priority values");
        };
        let value = if group.cursor == tracks.len() && tracks[group.cursor - 1].reconciled {
            compiled.objects()[group.channel.object_index as usize].base_z_index
        } else {
            *to
        };
        let changed = *row.z_index != value;
        *row.z_index = value;
        return changed;
    }
    if group.channel.property == Property::Presence {
        let track = &tracks[group.cursor - 1];
        let TrackValues::Bool { to, .. } = &track.values else {
            unreachable!("compiled Presence track must contain bool values");
        };
        let changed = *row.presence != *to;
        *row.presence = *to;
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
    let preserve_render_frame =
        matches!(
            group.channel.property,
            Property::Position | Property::Rotation | Property::Scale
        ) && prepared_morph_owns_render_frame(compiled, group.channel.object_index, time);
    if track.reconciled && time >= track.timing.start_time + track.timing.duration {
        if group.channel.property == Property::Morph
            && matches!(track.values, TrackValues::PreparedMorph { .. })
        {
            let changed = *row.morph != 0.0
                || row.render_geometry.is_some()
                || row.render_transform.is_some();
            *row.morph = 0.0;
            *row.render_geometry = None;
            *row.render_transform = None;
            return changed;
        }
        let base = match group.channel.property {
            Property::Position => Some(EvaluatedValue::Vec2(base_transform.translation)),
            Property::Rotation => Some(EvaluatedValue::Scalar(base_transform.rotation)),
            Property::Scale => Some(EvaluatedValue::Vec2(base_transform.scale)),
            Property::Fill => Some(EvaluatedValue::Color(base_style.fill)),
            Property::Stroke => Some(EvaluatedValue::Color(base_style.stroke)),
            Property::StrokeWidth => Some(EvaluatedValue::Scalar(base_style.stroke_width)),
            Property::Opacity => Some(EvaluatedValue::Scalar(base_style.opacity)),
            Property::Appearance | Property::Reveal => Some(EvaluatedValue::Scalar(1.0)),
            Property::Morph => Some(EvaluatedValue::Scalar(0.0)),
            Property::Presence | Property::ZIndex | Property::Transform => None,
        };
        return base.is_some_and(|value| {
            apply_evaluated_value(
                &mut row,
                group.channel.property,
                value,
                preserve_render_frame,
            )
        });
    }

    if group.channel.property == Property::Transform {
        return apply_transform_track(&mut row, track, progress);
    }
    if group.channel.property == Property::Morph
        && matches!(track.values, TrackValues::PreparedMorph { .. })
    {
        return apply_prepared_morph_track(&mut row, track, progress);
    }
    let value = interpolate(track, progress);
    apply_evaluated_value(
        &mut row,
        group.channel.property,
        value,
        preserve_render_frame,
    )
}

fn apply_effective_property_to_row(row: FrameRowMut<'_>, write: EffectivePropertyWrite) {
    match write {
        EffectivePropertyWrite::Transform { transform, .. } => {
            release_render_transform(row.render_geometry, row.render_transform, *row.transform);
            *row.transform = transform;
        }
        EffectivePropertyWrite::Style { style, .. } => *row.style = style,
    }
}

fn apply_evaluated_value(
    row: &mut FrameRowMut<'_>,
    property: Property,
    value: EvaluatedValue,
    preserve_render_frame: bool,
) -> bool {
    match (property, value) {
        (Property::Appearance, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.appearance != value;
            *row.appearance = value;
            changed
        }
        (Property::Reveal, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.reveal != value;
            *row.reveal = value;
            changed
        }
        (Property::Morph, EvaluatedValue::Scalar(value)) => {
            let value = value.clamp(0.0, 1.0);
            let changed = *row.morph != value;
            *row.morph = value;
            changed
        }
        (Property::Position, EvaluatedValue::Vec2(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.translation != value;
            row.transform.translation = value;
            changed || render_changed
        }
        (Property::Rotation, EvaluatedValue::Scalar(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.rotation != value;
            row.transform.rotation = value;
            changed || render_changed
        }
        (Property::Scale, EvaluatedValue::Vec2(value)) => {
            let render_changed = !preserve_render_frame
                && release_render_transform(
                    row.render_geometry,
                    row.render_transform,
                    *row.transform,
                );
            let changed = row.transform.scale != value;
            row.transform.scale = value;
            changed || render_changed
        }
        (Property::Fill, EvaluatedValue::Color(value)) => {
            let changed = row.style.fill != value;
            row.style.fill = value;
            changed
        }
        (Property::Stroke, EvaluatedValue::Color(value)) => {
            let changed = row.style.stroke != value;
            row.style.stroke = value;
            changed
        }
        (Property::StrokeWidth, EvaluatedValue::Scalar(value)) => {
            let changed = row.style.stroke_width != value;
            row.style.stroke_width = value;
            changed
        }
        (Property::Opacity, EvaluatedValue::Scalar(value)) => {
            let changed = row.style.opacity != value;
            row.style.opacity = value;
            changed
        }
        _ => unreachable!("compiled track value type must match its property"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum EvaluatedValue {
    Scalar(f32),
    Vec2(Vec2),
    Color(Option<Color>),
}

// A prepared morph owns its fixed rendering frame alongside ordinary semantic
// affine channels. Preserve that frame during their evaluation instead of rebuilding
// both paths only to reinstall the same compiled resource in the final Morph channel.
fn prepared_morph_owns_render_frame(
    compiled: &CompiledScene,
    object_index: u32,
    time: f64,
) -> bool {
    let tracks = compiled.channel_tracks(CompiledChannelKey::new(object_index, Property::Morph));
    let cursor = tracks.partition_point(|track| track.timing.start_time <= time);
    cursor.checked_sub(1).is_some_and(|index| {
        let track = &tracks[index];
        matches!(track.values, TrackValues::PreparedMorph { .. })
            && !(track.reconciled && time >= track.timing.start_time + track.timing.duration)
            && mapped_track_progress(track, time).is_some()
    })
}

fn apply_prepared_morph_track(
    row: &mut FrameRowMut<'_>,
    track: &CompiledTrack,
    progress: f32,
) -> bool {
    let plan = track
        .transform_geometry_plan
        .as_ref()
        .expect("prepared Morph track must carry compiled endpoint geometry");
    apply_prepared_morph_values(row, &track.values, plan, progress)
        .expect("prepared Morph track must contain matching scalar endpoints and path plan")
}

fn apply_prepared_morph_values(
    row: &mut FrameRowMut<'_>,
    values: &TrackValues,
    plan: &TransformGeometryPlan,
    progress: f32,
) -> Option<bool> {
    let TrackValues::PreparedMorph { from, to, .. } = values else {
        return None;
    };
    let TransformGeometryPlan::PathPair {
        geometry,
        render_transform,
    } = plan
    else {
        return None;
    };
    let morph = lerp(*from, *to, progress).clamp(0.0, 1.0);
    let mut changed = *row.morph != morph || *row.render_transform != *render_transform;
    *row.morph = morph;
    *row.render_transform = *render_transform;
    changed |= set_optional_geometry_if_changed(row.render_geometry, Some(geometry), true);
    Some(changed)
}

fn apply_transform_track(row: &mut FrameRowMut<'_>, track: &CompiledTrack, progress: f32) -> bool {
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
    let fixed_endpoint = matches!(
        plan,
        TransformGeometryPlan::PathPair {
            render_transform: Some(_),
            ..
        }
    ) && (progress <= 0.0 || progress >= 1.0);
    let next_morph = if !fixed_endpoint && matches!(plan, TransformGeometryPlan::PathPair { .. }) {
        progress
    } else {
        0.0
    };
    let owned_render_geometry = match plan {
        TransformGeometryPlan::PathPair {
            geometry: prepared,
            render_transform: None,
        } if from.style.stroke_width_mode == StrokeWidthMode::ScreenSpace
            && to.style.stroke_width_mode == StrokeWidthMode::ScreenSpace =>
        {
            screen_space_path_pair_relative_to_current(prepared, from, to, next_transform)
                .map(Arc::new)
        }
        _ => None,
    };
    let mut next_render_geometry = if let Some(geometry) = owned_render_geometry.as_ref() {
        Some(geometry)
    } else {
        match plan {
            TransformGeometryPlan::PathPair { geometry, .. } => Some(geometry),
            _ => None,
        }
    };

    if fixed_endpoint {
        next_render_geometry = None;
    }
    let next_render_transform = match plan {
        TransformGeometryPlan::PathPair {
            render_transform, ..
        } if !fixed_endpoint => *render_transform,
        _ => None,
    };
    let transform_changed = *row.render_transform != next_render_transform;
    *row.render_transform = next_render_transform;

    let Some(geometry) = row.content.geometry_mut() else {
        unreachable!("compiler rejects geometry-only Transform tracks on text content");
    };
    let mut changed =
        transform_changed | apply_transform_geometry(geometry, plan, from, to, progress);
    if *row.transform != next_transform {
        *row.transform = next_transform;
        changed = true;
    }
    if *row.style != next_style {
        *row.style = next_style;
        changed = true;
    }
    if *row.morph != next_morph {
        *row.morph = next_morph;
        changed = true;
    }
    changed |= set_optional_geometry_if_changed(
        row.render_geometry,
        next_render_geometry,
        owned_render_geometry.is_none(),
    );
    changed
}

// Independent affine drivers take ownership in semantic coordinates. The common
// morph-only lane never enters this conversion, so its compiled pair stays shared.
fn release_render_transform(
    geometry: &mut Option<Arc<GeometryRef>>,
    render_transform: &mut Option<Transform2D>,
    semantic_transform: Transform2D,
) -> bool {
    let Some(render) = *render_transform else {
        return false;
    };
    let Some(GeometryRef::VectorPath(source)) = geometry.as_deref() else {
        return false;
    };
    let Some(target) = source.morph_target() else {
        return false;
    };
    let source = path_relative_to_current(source, render, semantic_transform)
        .expect("fixed morph plan guarantees invertible semantic scale until driver takeover");
    let target = path_relative_to_current(target, render, semantic_transform)
        .expect("fixed morph plan guarantees invertible semantic scale until driver takeover");
    *geometry = Some(Arc::new(GeometryRef::path(
        source.with_morph_target(target),
    )));
    *render_transform = None;
    true
}

fn screen_space_path_pair_relative_to_current(
    prepared: &GeometryRef,
    from: &TransformTrackEndpoint,
    to: &TransformTrackEndpoint,
    current: Transform2D,
) -> Option<GeometryRef> {
    let GeometryRef::VectorPath(source) = prepared else {
        return None;
    };
    let target = source.morph_target()?;
    let source = path_relative_to_current(source, from.transform, current)?;
    let target = path_relative_to_current(target, to.transform, current)?;
    Some(GeometryRef::path(source.with_morph_target(target)))
}

fn path_relative_to_current(
    path: &VectorPath,
    endpoint: Transform2D,
    current: Transform2D,
) -> Option<VectorPath> {
    let mut transformed = VectorPath::new();
    for command in path.commands() {
        transformed = match *command {
            PathCommand::MoveTo { to } => {
                transformed.move_to(point_relative_to_current(to, endpoint, current)?)
            }
            PathCommand::LineTo { to } => {
                transformed.line_to(point_relative_to_current(to, endpoint, current)?)
            }
            PathCommand::QuadraticTo { control, to } => transformed.quadratic_to(
                point_relative_to_current(control, endpoint, current)?,
                point_relative_to_current(to, endpoint, current)?,
            ),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => transformed.cubic_to(
                point_relative_to_current(control1, endpoint, current)?,
                point_relative_to_current(control2, endpoint, current)?,
                point_relative_to_current(to, endpoint, current)?,
            ),
            PathCommand::Close => transformed.close(),
        };
    }
    Some(transformed)
}

fn point_relative_to_current(
    point: Vec2,
    endpoint: Transform2D,
    current: Transform2D,
) -> Option<Vec2> {
    const MIN_SCALE: f32 = 1.0e-7;
    if !current.scale.x.is_finite()
        || !current.scale.y.is_finite()
        || current.scale.x.abs() <= MIN_SCALE
        || current.scale.y.abs() <= MIN_SCALE
    {
        return None;
    }
    let world = endpoint.transform_point(point);
    let relative = (world - current.translation).rotate(-current.rotation);
    let result = Vec2::new(relative.x / current.scale.x, relative.y / current.scale.y);
    (result.x.is_finite() && result.y.is_finite()).then_some(result)
}

fn apply_transform_geometry(
    current: &mut GeometryRef,
    plan: &TransformGeometryPlan,
    from: &TransformTrackEndpoint,
    to: &TransformTrackEndpoint,
    progress: f32,
) -> bool {
    match plan {
        TransformGeometryPlan::Static | TransformGeometryPlan::PointwiseRotation => {
            set_geometry_if_changed(current, &from.geometry)
        }
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
        TransformGeometryPlan::PathPair { .. } => {
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

// Compiled resources must recover their registered identity after a temporary
// affine-driver conversion, even when the owned points happen to be identical.
// Only dynamically rebuilt, unregistered geometry may retain a content-equal Arc.
fn set_optional_geometry_if_changed(
    current: &mut Option<Arc<GeometryRef>>,
    next: Option<&Arc<GeometryRef>>,
    require_shared_identity: bool,
) -> bool {
    match next {
        Some(next)
            if current.as_ref().is_some_and(|current| {
                Arc::ptr_eq(current, next)
                    || (!require_shared_identity && current.as_ref() == next.as_ref())
            }) =>
        {
            false
        }
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

fn interpolate_pointwise_rotation_transform(
    from: Transform2D,
    to: Transform2D,
    progress: f32,
) -> Transform2D {
    if progress <= 0.0 {
        return from;
    }
    if progress >= 1.0 {
        return to;
    }
    debug_assert_eq!(from.scale, to.scale);
    let inverse = 1.0 - progress;
    let cosine = inverse * from.rotation.cos() + progress * to.rotation.cos();
    let sine = inverse * from.rotation.sin() + progress * to.rotation.sin();
    let magnitude = cosine.hypot(sine);
    Transform2D {
        translation: interpolate_vec2(from.translation, to.translation, progress),
        rotation: sine.atan2(cosine),
        scale: Vec2::new(from.scale.x * magnitude, from.scale.y * magnitude),
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
        stroke_width_mode: if progress >= 1.0 {
            to.stroke_width_mode
        } else {
            from.stroke_width_mode
        },
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
    if track.timing.is_instant() {
        return 1.0;
    }
    let raw = ((time - track.timing.start_time) / track.timing.duration).clamp(0.0, 1.0) as f32;
    track.timing.easing.evaluate(raw)
}

fn mapped_track_progress(track: &CompiledTrack, time: f64) -> Option<f32> {
    mapped_continuous_progress(track.timing, &track.time_map, time)
}

fn interpolate(track: &CompiledTrack, progress: f32) -> EvaluatedValue {
    interpolate_track_values(&track.values, progress)
        .expect("compiled continuous track carries an interpolable value kind")
}

fn interpolate_track_values(values: &TrackValues, progress: f32) -> Option<EvaluatedValue> {
    match values {
        TrackValues::Scalar { from, to } => {
            Some(EvaluatedValue::Scalar(lerp(*from, *to, progress)))
        }
        TrackValues::Vec2 { from, to } => Some(EvaluatedValue::Vec2(Vec2::new(
            lerp(from.x, to.x, progress),
            lerp(from.y, to.y, progress),
        ))),
        TrackValues::Color { from, to } => Some(EvaluatedValue::Color(interpolate_optional_color(
            *from, *to, progress,
        ))),
        TrackValues::Bool { .. }
        | TrackValues::ZIndex { .. }
        | TrackValues::Object { .. }
        | TrackValues::PreparedMorph { .. } => None,
    }
}

const fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use noon_compile::{CompiledFamilyAnimation, CompiledObject, CompiledScene};
    use noon_core::ObjectContentRef;
    use noon_core::TrackId;
    use noon_core::{
        Color, CompositionTimeMap, CompositionTimeMapStep, GeometryRef, Property, RateFunction,
        Style, TrackDefinition, TrackTiming,
    };
    use noon_core::{
        FamilyAnimationMode, FamilyAnimationSpec, RetainedAnimationMembers,
        RetainedFamilyAnimationPlan, SemanticStore, TextResourceArena,
    };

    use super::*;

    fn compile_linear_scene() -> CompiledScene {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        CompiledScene::compile_objects(objects, &tracks).expect("scene must compile")
    }

    #[test]
    fn shared_family_channel_uses_main_scheduler_and_sparse_active_publication() {
        let object = CompiledObject::new(
            ObjectId::new(40),
            ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let compiled = CompiledScene::compile_objects(vec![object.clone()], &[]).unwrap();
        let mut semantics = SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let members =
            RetainedAnimationMembers::resolve(&object.content, &TextResourceArena::new()).unwrap();
        let plan = RetainedFamilyAnimationPlan::single_leaf(leaf, object.id, members).unwrap();
        let spec = FamilyAnimationSpec::new(
            FamilyAnimationMode::DrawBorderThenFill,
            0.0,
            4.0,
            0.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        let time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Linear,
        )]);
        let mut instance = SceneInstance::new(compiled);
        instance
            .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
                CompiledFamilyAnimation {
                    target: object.id,
                    plan: plan.clone(),
                    spec,
                    time_map,
                },
            ))
            .unwrap();
        let reverse = FamilyAnimationSpec::new(
            FamilyAnimationMode::DrawBorderThenFill,
            3.0,
            2.0,
            0.0,
            RateFunction::Linear,
            false,
            true,
        )
        .unwrap();
        instance
            .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
                CompiledFamilyAnimation {
                    target: object.id,
                    plan,
                    spec: reverse,
                    time_map: CompositionTimeMap::identity(),
                },
            ))
            .unwrap();

        instance.seek(0.5).unwrap();
        assert!(instance.active_family_animation_indices().is_empty());
        instance.seek(2.0).unwrap();
        assert_eq!(
            instance.frame().family_animations[0]
                .expect("mapped family channel is active")
                .overall_progress,
            0.5
        );
        assert_eq!(
            instance
                .active_family_animation_indices()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(instance.frame().family_animation_plan_indices[0], Some(0));
        assert_eq!(instance.family_animation_plans().len(), 2);
        instance.seek(4.0).unwrap();
        assert_eq!(instance.frame().family_animation_plan_indices[0], Some(1));
        instance.seek(2.0).unwrap();
        assert_eq!(instance.frame().family_animation_plan_indices[0], Some(0));
        instance.advance_to(3.0).unwrap();
        assert_eq!(instance.frame().family_animation_plan_indices[0], Some(1));
        instance.advance_to(5.0).unwrap();
        let endpoint = instance.frame().family_animations[0]
            .expect("reverse family channel retains its exact endpoint");
        assert_eq!(endpoint.overall_progress, 1.0);
        assert!(endpoint.reverse_member_order);
        assert_eq!(instance.frame().family_animation_plan_indices[0], Some(1));
        assert_eq!(instance.pending_family_endpoint_expirations.len(), 1);
        instance.advance_to(5.0 + 1e-9).unwrap();
        assert!(instance.active_family_animation_indices().is_empty());
        assert!(instance.frame().family_animations[0].is_none());
        assert!(instance.pending_family_endpoint_expirations.is_empty());
        assert!(instance
            .timeline_scheduler
            .requested_family_animations()
            .is_empty());

        let forward = &instance.compiled.family_animations()[0];
        let (_, forward_end) = family_animation_interval(forward);
        assert_eq!(
            family_state_at(forward, forward_end)
                .expect("forward family channel retains its exact endpoint")
                .overall_progress,
            1.0
        );
        assert!(family_state_at(forward, forward_end + 1e-9).is_none());

        instance.take_frame_changes();
        instance.seek(5.0).unwrap();
        instance.take_frame_changes();
        assert_eq!(instance.pending_family_endpoint_expirations.len(), 1);
        instance
            .apply_execution_patch(&ExecutionPatch::RemoveObject(object.id))
            .unwrap();
        assert!(instance.pending_family_endpoint_expirations.is_empty());
        assert_eq!(instance.take_frame_changes().removed_indices(), &[0]);
        instance.advance_to(5.0 + 1e-9).unwrap();
        assert!(instance.take_frame_changes().is_empty());
        assert!(instance.active_family_animation_indices().is_empty());
        assert!(instance.frame().family_animations[0].is_none());
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
    fn scale_timeline_endpoints_and_midpoint_are_exact() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Scale,
            values: TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(3.0, 2.0),
            },
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        assert_eq!(
            instance.seek(1.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::ONE
        );
        assert_eq!(
            instance.seek(2.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::new(2.0, 1.5)
        );
        assert_eq!(
            instance.seek(3.0).expect("valid time").objects[0]
                .transform
                .scale,
            Vec2::new(3.0, 2.0)
        );
    }

    #[test]
    fn manim_smooth_rate_function_is_evaluated_by_runtime() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Smooth),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
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
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.0,
                1.0,
                RateFunction::Smooth,
            )]),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        let quarter = instance.seek(0.5).unwrap().objects[0]
            .transform
            .translation
            .x;
        assert!((quarter - 0.7010372).abs() < 1e-5);
    }

    #[test]
    fn mapped_succession_selects_latest_virtual_child() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            tracks.push(TrackDefinition {
                id: TrackId::new(tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::new(from, 0.0),
                    to: Vec2::new(to, 0.0),
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    start,
                    0.5,
                    RateFunction::Linear,
                )]),
            });
        }
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
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
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        for (from, to, start) in [(0.0, 10.0, 0.0), (10.0, 20.0, 0.5)] {
            tracks.push(TrackDefinition {
                id: TrackId::new(tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::new(from, 0.0),
                    to: Vec2::new(to, 0.0),
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                    start,
                    0.5,
                    RateFunction::ThereAndBack,
                )]),
            });
        }
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
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
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(1.0),
            time_map: CompositionTimeMap::identity(),
        });
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Presence,
            values: TrackValues::Bool {
                from: true,
                to: false,
            },
            timing: TrackTiming::instant(3.0),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled =
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile");
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
    fn nested_mapped_presence_has_one_forward_and_seek_boundary() {
        let mut store = noon_core::SemanticStore::new();
        let node = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        store.attach_semantic_object(node).unwrap();
        let mut index = noon_compile::SemanticExecutionIndex::new();
        let (mut compiled, _) = noon_compile::lower_semantic_execution(&store, &mut index)
            .unwrap()
            .into_parts();
        let object = index.execution_object_id(node).unwrap();
        compiled
            .apply_execution_patch(&noon_compile::ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(0),
                object,
                property: Property::Presence,
                values: TrackValues::Bool {
                    from: false,
                    to: true,
                },
                timing: TrackTiming::new(2.0, 4.0, RateFunction::Linear),
                time_map: CompositionTimeMap::from_steps(vec![
                    CompositionTimeMapStep::new(0.2, 0.6, RateFunction::Smooth),
                    CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
                ]),
            }))
            .unwrap();
        let boundary = compiled.tracks()[0].timing.start_time;
        assert!(boundary > 2.0 && boundary < 6.0);

        let mut forward = SceneInstance::new(compiled.clone());
        forward.advance_to(boundary - 1e-6).unwrap();
        assert!(!forward.frame().is_present(0));
        forward.advance_to(boundary).unwrap();
        assert!(forward.frame().is_present(0));
        assert_eq!(forward.last_timeline_scheduler_stats().events_crossed, 1);
        forward.advance_to(boundary + 0.25).unwrap();
        assert!(forward.frame().is_present(0));
        assert_eq!(forward.last_timeline_scheduler_stats().groups_requested, 0);

        let mut direct = SceneInstance::new(compiled);
        direct.seek(boundary).unwrap();
        assert_eq!(direct.frame(), forward.seek(boundary).unwrap());
        direct.seek(boundary - 1e-6).unwrap();
        assert!(!direct.frame().is_present(0));
    }

    #[test]
    fn reveal_endpoints_midpoint_and_prestart_state_are_deterministic() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::path(
                noon_core::VectorPath::new()
                    .move_to(Vec2::ZERO)
                    .line_to(Vec2::new(3.0, 4.0)),
            ),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Reveal,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        assert_eq!(instance.seek(0.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(1.0).expect("valid time").reveal(0), 0.0);
        assert_eq!(instance.seek(2.0).expect("valid time").reveal(0), 0.5);
        assert_eq!(instance.seek(3.0).expect("valid time").reveal(0), 1.0);
    }

    #[test]
    fn appearance_and_semantic_opacity_are_independent() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        objects[object.get() as usize].base_style.opacity = 0.4;
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Appearance,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
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
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::path(source.with_morph_target(target)),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Reveal,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Morph,
            values: TrackValues::Scalar { from: 0.0, to: 1.0 },
            timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
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
    fn timeline_publication_advances_only_frame_epoch_and_same_time_is_a_no_op() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let before = instance.publication_context();

        instance.advance_to(2.0).expect("valid time");
        let advanced = instance.publication_context();
        assert_eq!(advanced.scene_revision(), before.scene_revision());
        assert_eq!(advanced.execution_revision(), before.execution_revision());
        assert_eq!(
            advanced.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );

        instance.advance_to(2.0).expect("same time is valid");
        assert_eq!(instance.publication_context(), advanced);

        instance.seek(1.0).expect("backward seek is valid");
        let sought = instance.publication_context();
        assert_eq!(sought.scene_revision(), advanced.scene_revision());
        assert_eq!(sought.execution_revision(), advanced.execution_revision());
        assert_eq!(
            sought.frame_epoch(),
            advanced.frame_epoch().checked_next().unwrap()
        );
    }

    #[test]
    fn renderer_publication_binds_one_frame_context_and_consumes_its_changes() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let initial_context = instance.publication_context();
        {
            let publication = instance.take_renderer_publication();
            assert_eq!(publication.context(), initial_context);
            assert!(publication.changes().is_all());
            assert_eq!(publication.frame().time, 0.0);
            assert_eq!(publication.frame().objects.len(), 1);
        }
        assert!(instance.take_frame_changes().is_empty());

        instance.advance_to(2.0).expect("valid time");
        let advanced_context = instance.publication_context();
        let publication = instance.take_renderer_publication();
        assert_eq!(publication.context(), advanced_context);
        assert_eq!(publication.frame().time, 2.0);
        assert!(!publication.changes().is_empty());
        assert_ne!(
            publication.context().frame_epoch(),
            initial_context.frame_epoch()
        );
    }

    #[test]
    fn completed_history_is_not_rescanned_during_forward_steps() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        for index in 0..1_000 {
            let start = f64::from(index);
            let from = index as f32;
            tracks.push(TrackDefinition {
                id: TrackId::new(tracks.len() as u64),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::new(from, 0.0),
                    to: Vec2::new(from + 1.0, 0.0),
                },
                timing: TrackTiming::new(start, 0.5, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            });
        }
        let compiled =
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile");
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
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled =
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile");
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
        let object = ObjectId::new(0);
        let mut source = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let track_id = noon_core::TrackId::new(0);
        let initial_track = TrackDefinition {
            id: track_id,
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(4.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled =
            CompiledScene::compile_objects(vec![source.clone()], &[initial_track]).unwrap();
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
            timing: TrackTiming::new(0.0, 4.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let track_patch = ExecutionPatch::ReplaceTrack(replacement.clone());
        source.base_style = Style {
            opacity: 0.75,
            stroke_join: noon_core::StrokeJoin::Round,
            stroke_cap: noon_core::StrokeCap::Round,
            ..Style::default()
        };
        let style_patch = ExecutionPatch::SetStyle {
            object,
            style: source.base_style,
        };
        live.apply_execution_patch(&track_patch)
            .expect("valid patch");
        live.apply_execution_patch(&style_patch)
            .expect("valid patch");
        let expected_compiled =
            CompiledScene::compile_objects(vec![source], &[replacement]).unwrap();
        let mut expected = SceneInstance::new(expected_compiled);
        expected.seek(2.0).expect("valid time");
        assert_eq!(live.frame(), expected.frame());
    }

    #[test]
    fn adding_presence_event_live_reconciles_at_current_time() {
        let object = ObjectId::new(0);
        let source = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let mut live =
            SceneInstance::new(CompiledScene::compile_objects(vec![source.clone()], &[]).unwrap());
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
        let patch = ExecutionPatch::AddTrack(presence.clone());
        live.apply_execution_patch(&patch)
            .expect("presence patch must apply");
        let mut expected =
            SceneInstance::new(CompiledScene::compile_objects(vec![source], &[presence]).unwrap());
        expected.seek(2.0).expect("valid time");
        assert_eq!(live.frame(), expected.frame());
        assert!(!live.frame().is_present(0));
    }

    #[test]
    fn removing_track_restores_base_property_at_current_time() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let track_id = TrackId::new(tracks.len() as u64);
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let compiled =
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.seek(1.0).expect("valid time");
        assert_eq!(instance.frame().objects[0].style.opacity, 0.5);
        instance
            .apply_execution_patch(&ExecutionPatch::RemoveTrack(track_id))
            .expect("valid patch");
        assert_eq!(instance.frame().objects[0].style.opacity, 1.0);
    }

    #[test]
    fn fill_track_uses_optional_paint_appearance_and_local_style_dirtying() {
        let object = ObjectId::new(1);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Fill,
            values: TrackValues::Color {
                from: None,
                to: Some(Color::RED),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::default(),
                Style {
                    fill: None,
                    ..Style::default()
                },
            )],
            &[track],
        )
        .expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.take_frame_changes();

        instance.advance_to(1.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.fill,
            Some(Color {
                alpha: 0.5,
                ..Color::RED
            })
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        instance.advance_to(1.0).unwrap();
        assert!(instance.take_frame_changes().is_empty());
        instance.advance_to(2.0).unwrap();
        assert_eq!(instance.frame().objects[0].style.fill, Some(Color::RED));
    }

    #[test]
    fn stroke_track_uses_optional_paint_appearance_and_local_style_dirtying() {
        let object = ObjectId::new(1);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object,
            property: Property::Stroke,
            values: TrackValues::Color {
                from: None,
                to: Some(Color {
                    alpha: 0.4,
                    ..Color::BLUE
                }),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled = CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                Transform2D::default(),
                Style {
                    stroke: None,
                    ..Style::default()
                },
            )],
            &[track],
        )
        .expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.take_frame_changes();

        instance.advance_to(1.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.stroke,
            Some(Color {
                alpha: 0.2,
                ..Color::BLUE
            })
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        instance.advance_to(1.0).unwrap();
        assert!(instance.take_frame_changes().is_empty());
        instance.advance_to(2.0).unwrap();
        assert_eq!(
            instance.frame().objects[0].style.stroke,
            Some(Color {
                alpha: 0.4,
                ..Color::BLUE
            })
        );
    }

    #[test]
    fn stroke_width_track_updates_only_its_style_row_and_spatial_bounds() {
        let target = ObjectId::new(4);
        let untouched = ObjectId::new(9);
        let style = Style {
            stroke: Some(Color::WHITE),
            stroke_width: 0.0,
            ..Style::default()
        };
        let compiled = CompiledScene::compile_objects(
            vec![
                CompiledObject::new(
                    target,
                    GeometryRef::rectangle(2.0, 2.0),
                    Transform2D::IDENTITY,
                    style,
                ),
                CompiledObject::new(
                    untouched,
                    GeometryRef::circle(1.0),
                    Transform2D::IDENTITY,
                    Style::default(),
                ),
            ],
            &[TrackDefinition {
                id: TrackId::new(0),
                object: target,
                property: Property::StrokeWidth,
                values: TrackValues::Scalar { from: 0.0, to: 2.0 },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        )
        .expect("scene must compile");
        let mut instance = SceneInstance::new(compiled);
        instance.take_frame_changes();
        instance.take_spatial_changes();

        instance.advance_to(1.0).expect("valid time");

        assert_eq!(instance.frame().objects[0].style.stroke_width, 1.0);
        assert_eq!(instance.frame().objects[1].id, untouched);
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
        assert_eq!(instance.take_spatial_changes().object_indices(), &[0]);

        instance.advance_to(1.0).expect("same time remains valid");
        assert!(instance.take_frame_changes().is_empty());

        instance.seek(2.0).expect("endpoint seek remains valid");
        assert_eq!(instance.frame().objects[0].style.stroke_width, 2.0);
        instance.take_frame_changes();
        instance.take_spatial_changes();
        instance.seek(0.5).expect("historical seek remains valid");
        assert_eq!(instance.frame().objects[0].style.stroke_width, 0.5);
        // Historical seek may invalidate the full frame; both forms must cover
        // the changed width. Normal forward updates above remain strictly local.
        assert!(instance.take_frame_changes().contains_object(0));
        assert!(instance.take_spatial_changes().contains_object(0));
    }

    #[test]
    fn value_patch_updates_base_fields_without_overwriting_animated_values() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object,
            property: Property::Opacity,
            values: TrackValues::Scalar { from: 1.0, to: 0.0 },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        instance.seek(1.0).expect("valid time");
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
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
        let mut objects = Vec::new();
        let tracks = Vec::new();
        let _object = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            _object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        assert!(instance.take_frame_changes().is_all());
        instance.advance_to(0.5).expect("valid time");
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn frame_changes_accumulate_animation_and_patches_until_consumed() {
        let mut objects = Vec::new();
        let mut tracks = Vec::new();
        let animated = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            animated,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        let patched = ObjectId::new(objects.len() as u64);
        objects.push(CompiledObject::new(
            patched,
            GeometryRef::rectangle(2.0, 1.0),
            Transform2D::IDENTITY,
            Style::default(),
        ));
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object: animated,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
        let mut instance = SceneInstance::new(
            CompiledScene::compile_objects(objects, &tracks).expect("scene must compile"),
        );
        instance.take_frame_changes();
        instance.advance_to(0.5).expect("valid time");
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
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

    #[test]
    fn painter_reorder_keeps_dense_rows_and_publishes_only_shifted_order() {
        let [first, second, third] = [1, 2, 3].map(ObjectId::new);
        let objects = [first, second, third]
            .into_iter()
            .map(|id| {
                CompiledObject::new(
                    id,
                    ObjectContentRef::Geometry(GeometryRef::circle(id.get() as f32)),
                    Transform2D::IDENTITY,
                    Style::default(),
                )
            })
            .collect();
        let mut instance =
            SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
        instance.take_frame_changes();

        instance
            .apply_execution_patch(&ExecutionPatch::ReorderObject {
                object: third,
                before: Some(first),
            })
            .expect("live objects can reorder");

        assert_eq!(instance.painter_order(), &[2, 0, 1]);
        assert_eq!(instance.frame().objects[0].id, first);
        assert_eq!(instance.frame().objects[1].id, second);
        assert_eq!(instance.frame().objects[2].id, third);
        let changes = instance.take_frame_changes();
        assert_eq!(changes.painter_order_range(), Some(0..3));
        assert!(changes.object_indices().is_empty());
        assert!(!changes.is_structural());
    }
}

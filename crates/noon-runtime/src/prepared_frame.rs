use std::collections::{BTreeMap, BTreeSet, HashSet};

use noon_compile::{CompilePatchError, CompiledChannelKey, ExecutionMutationTransaction};
use noon_core::PublicationContext;
use noon_core::{ReactiveValue, SignalId};

use crate::numeric_text::PreparedNumericTextUpdate;
use crate::{
    apply_effective_property_to_row, apply_group_to_row, apply_reactive_value_to_row,
    effective_object_conservative_bounds_with_resources, upper_bound_start, EffectiveBoundsBasis,
    EffectiveObjectProperties, EffectivePropertyWrite, FrameRowState, FrameState, RuntimeIdentity,
    SceneInstance, TrackGroup, PROPERTY_ORDER,
};
use crate::{EvaluationError, EvaluationStats, TimelineSchedulerStats};

#[derive(Clone, Debug)]
struct PreparedFrameRow {
    object_index: usize,
    state: FrameRowState,
}

/// Sparse, unpublished timeline/native evaluation for a required callback phase.
#[derive(Clone, Debug)]
pub struct PreparedFrameEvaluation {
    runtime: RuntimeIdentity,
    expected: PublicationContext,
    base_time: f64,
    time: f64,
    requested_channels: Vec<CompiledChannelKey>,
    requested_family_animations: Vec<usize>,
    cursor_updates: Vec<(CompiledChannelKey, usize)>,
    rows: Vec<PreparedFrameRow>,
    stats: EvaluationStats,
    scheduler_stats: TimelineSchedulerStats,
    prior_driver_rows: usize,
    reactive: Option<crate::PreparedReactiveRuntimeUpdate>,
    deferred_reactive_bindings: Vec<(noon_core::ObjectId, noon_core::Property)>,
    numeric_text: Vec<PreparedNumericTextUpdate>,
    pub(crate) authored_scalar_inputs: bool,
}

impl PreparedFrameEvaluation {
    pub const fn time(&self) -> f64 {
        self.time
    }

    pub const fn expected_publication(&self) -> PublicationContext {
        self.expected
    }

    pub(crate) fn staged_row(&self, object_index: usize) -> Option<&FrameRowState> {
        self.rows
            .binary_search_by_key(&object_index, |row| row.object_index)
            .ok()
            .map(|index| &self.rows[index].state)
    }

    pub fn staged_row_count(&self) -> usize {
        self.rows.len()
    }

    pub const fn evaluation_stats(&self) -> EvaluationStats {
        self.stats
    }

    pub const fn scheduler_stats(&self) -> TimelineSchedulerStats {
        self.scheduler_stats
    }

    pub const fn prior_driver_rows(&self) -> usize {
        self.prior_driver_rows
    }

    pub fn deferred_reactive_bindings(&self) -> &[(noon_core::ObjectId, noon_core::Property)] {
        &self.deferred_reactive_bindings
    }
}

/// Fully validated final effective writes. Construction validates every supplied
/// write before retaining only the last value for each object/property.
#[derive(Clone, Debug)]
pub struct PreparedEffectivePropertyBatch {
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) expected: PublicationContext,
    pub(crate) writes: Vec<(usize, EffectivePropertyWrite)>,
}

impl PreparedEffectivePropertyBatch {
    pub fn len(&self) -> usize {
        self.writes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedFrameCommitError {
    ReplaySealed,
    ForeignRuntime {
        expected: RuntimeIdentity,
        actual: RuntimeIdentity,
    },
    StalePublication {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    StaleTime {
        expected: f64,
        actual: f64,
    },
    FrameEpochExhausted(noon_core::FrameEpoch),
}

impl std::fmt::Display for PreparedFrameCommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReplaySealed => {
                formatter.write_str("sealed replay cannot publish a live prepared frame")
            }
            Self::ForeignRuntime { expected, actual } => write!(
                formatter,
                "prepared runtime identity {actual:?} does not match {expected:?}"
            ),
            Self::StalePublication { expected, actual } => write!(
                formatter,
                "prepared frame expected publication {expected:?}, found {actual:?}"
            ),
            Self::StaleTime { expected, actual } => write!(
                formatter,
                "prepared frame expected time {expected}, found {actual}"
            ),
            Self::FrameEpochExhausted(epoch) => {
                write!(formatter, "frame epoch exhausted after {epoch:?}")
            }
        }
    }
}

impl std::error::Error for PreparedFrameCommitError {}

impl SceneInstance {
    /// Publish already-validated transient effective writes without advancing the
    /// authored timeline or touching replay retention.  This is deliberately
    /// narrower than `commit_prepared_frame`: interaction drivers use it while a
    /// replay is sealed, but cannot mutate scheduler state, replay inputs, or the
    /// authored frame time.
    pub(crate) fn commit_transient_effective_properties(
        &mut self,
        effective: PreparedEffectivePropertyBatch,
    ) -> Result<&FrameState, PreparedFrameCommitError> {
        if effective.runtime != self.identity {
            return Err(PreparedFrameCommitError::ForeignRuntime {
                expected: self.identity,
                actual: effective.runtime,
            });
        }
        if effective.expected != self.publication {
            return Err(PreparedFrameCommitError::StalePublication {
                expected: effective.expected,
                actual: self.publication,
            });
        }
        if effective.writes.is_empty() {
            return Ok(&self.frame);
        }
        let next = self.publication.frame_epoch().checked_next().ok_or(
            PreparedFrameCommitError::FrameEpochExhausted(self.publication.frame_epoch()),
        )?;
        let mut changed = false;
        for (object_index, write) in effective.writes {
            let mut row = FrameRowState::from_frame(&self.frame, object_index);
            apply_effective_property_to_row(
                row.as_mut(&self.frame.objects[object_index].content),
                write,
            );
            if row.differs_from_frame(&self.frame, object_index) {
                let priority_changed = row.z_index != self.frame.objects[object_index].z_index;
                row.write_to_frame(&mut self.frame, object_index);
                if priority_changed {
                    self.reposition_painter_row(object_index);
                }
                self.mark_changed(object_index);
                changed = true;
            }
        }
        if changed {
            self.publication = self.publication.with_frame_epoch(next);
        }
        Ok(&self.frame)
    }

    /// Atomically advance timeline channels, then apply one already-lowered reactive
    /// input batch before publishing the resulting frame.
    pub fn advance_to_with_reactive_inputs(
        &mut self,
        time: f64,
        reactive_inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<&FrameState, EvaluationError> {
        let prepared = self.prepare_advance_to_with_reactive_inputs(time, reactive_inputs)?;
        let effective = self
            .prepare_effective_property_batch(&[])
            .expect("an empty effective-property batch is always valid");
        Ok(self
            .commit_prepared_frame(prepared, effective)
            .expect("an immediately committed prepared frame cannot become stale"))
    }

    /// Read effective properties from an unpublished phase. Unchanged bounds can
    /// be supplied from the caller's retained spatial index; a spatially changed
    /// sparse row derives fresh bounds from borrowed retained content.
    pub fn prepared_properties_at(
        &self,
        prepared: &PreparedFrameEvaluation,
        object_index: usize,
        cached_bounds: Option<noon_core::Rect>,
    ) -> Option<EffectiveObjectProperties> {
        if prepared.runtime != self.identity || !self.object_slot_is_live(object_index) {
            return None;
        }
        let Some(row) = prepared.staged_row(object_index) else {
            return self.effective_properties_at(object_index, cached_bounds);
        };
        let bounds = if row.spatially_differs_from_frame(&self.frame, object_index) {
            let object = &self.frame.objects[object_index];
            let content = row.content_override.as_ref().unwrap_or(&object.content);
            let local_bounds = object
                .text_bounds
                .or_else(|| content.image().map(|image| image.local_bounds()));
            effective_object_conservative_bounds_with_resources(
                row.render_geometry
                    .as_deref()
                    .or_else(|| content.geometry()),
                local_bounds,
                row.render_transform.unwrap_or(row.transform),
                row.style,
                self.geometry_resources(),
            )
        } else {
            cached_bounds
        };
        let object = &self.frame.objects[object_index];
        let content = row.content_override.as_ref().unwrap_or(&object.content);
        let local_bounds = object
            .text_bounds
            .or_else(|| content.image().map(|image| image.local_bounds()));
        let bounds_basis = EffectiveBoundsBasis::from_content(
            row.render_geometry
                .as_deref()
                .or_else(|| content.geometry()),
            local_bounds,
        );
        Some(row.properties(bounds, bounds_basis))
    }

    /// Read one reactive value through the same unpublished phase evaluation.
    pub fn prepared_reactive_value(
        &self,
        prepared: &PreparedFrameEvaluation,
        signal: noon_core::SignalId,
    ) -> Option<noon_core::ReactiveValue> {
        if prepared.runtime != self.identity {
            return None;
        }
        let reactive = self.reactive.as_ref()?;
        match prepared.reactive.as_ref() {
            Some(update) => reactive.prepared_value(update, signal),
            None => reactive.state_value(signal).cloned(),
        }
    }

    /// Prepare a forward timeline/native evaluation without changing the current
    /// coherent frame, scheduler, cursors, dirty sets, or publication context.
    pub fn prepare_advance_to(
        &mut self,
        time: f64,
    ) -> Result<PreparedFrameEvaluation, EvaluationError> {
        self.prepare_advance_to_with_reactive_inputs(time, &[])
    }

    pub fn prepare_advance_to_with_reactive_inputs(
        &mut self,
        time: f64,
        reactive_inputs: &[(SignalId, ReactiveValue)],
    ) -> Result<PreparedFrameEvaluation, EvaluationError> {
        self.prepare_advance_to_with_reactive_inputs_deferred(time, reactive_inputs, false)
    }

    /// Prepare reactive signal values while leaving selected binding writes for
    /// an ordered callback region to apply after its preceding host updaters.
    pub fn prepare_advance_to_with_reactive_inputs_deferred(
        &mut self,
        time: f64,
        reactive_inputs: &[(SignalId, ReactiveValue)],
        defer_reactive_bindings: bool,
    ) -> Result<PreparedFrameEvaluation, EvaluationError> {
        if self.replay_is_sealed() {
            return Err(EvaluationError::ReplaySealed);
        }
        if !time.is_finite() {
            return Err(EvaluationError::InvalidTime(time));
        }
        if time < self.frame.time {
            return Err(EvaluationError::NonMonotonicPreparedAdvance {
                current: self.frame.time,
                requested: time,
            });
        }
        if time != self.frame.time && self.publication.frame_epoch().checked_next().is_none() {
            return Err(EvaluationError::FrameEpochExhausted(
                self.publication.frame_epoch(),
            ));
        }

        let preview = self.timeline_scheduler.preview_advance(time);
        let mut rows = BTreeMap::<usize, FrameRowState>::new();
        let mut cursor_updates = BTreeMap::<CompiledChannelKey, usize>::new();
        let mut stats = EvaluationStats::default();
        let mut prior_driver_rows = 0;

        for &object_index in &self.effective_driver_rows {
            if !self.object_slot_is_live(object_index) {
                continue;
            }
            prior_driver_rows += 1;
            let mut row = FrameRowState::from_frame(&self.frame, object_index);
            let base_content = &self.frame.objects[object_index].content;
            for property in PROPERTY_ORDER {
                let channel = CompiledChannelKey::new(object_index as u32, property);
                let tracks = self.compiled.channel_tracks(channel);
                let Some(group) = self.groups.get(&channel) else {
                    continue;
                };
                let cursor = upper_bound_start(tracks, time, &mut stats.binary_search_steps);
                let prepared_group = TrackGroup {
                    channel,
                    cursor,
                    mapped: group.mapped,
                };
                let object = &self.compiled.objects()[object_index];
                apply_group_to_row(
                    &self.compiled,
                    row.as_mut(base_content),
                    tracks,
                    &prepared_group,
                    time,
                    object.base_transform,
                    object.base_style,
                );
                cursor_updates.insert(channel, cursor);
                stats.groups_evaluated += 1;
            }
            rows.insert(object_index, row);
        }

        for &channel in preview.requested() {
            let object_index = channel.object_index as usize;
            if self.effective_driver_rows.contains(&object_index) {
                continue;
            }
            let tracks = self.compiled.channel_tracks(channel);
            let Some(group) = self.groups.get(&channel) else {
                continue;
            };
            let mut cursor = group.cursor;
            while cursor < tracks.len() && tracks[cursor].timing.start_time <= time {
                cursor += 1;
                stats.tracks_advanced += 1;
            }
            let prepared_group = TrackGroup {
                channel,
                cursor,
                mapped: group.mapped,
            };
            let object = &self.compiled.objects()[object_index];
            let row = rows
                .entry(object_index)
                .or_insert_with(|| FrameRowState::from_frame(&self.frame, object_index));
            apply_group_to_row(
                &self.compiled,
                row.as_mut(&self.frame.objects[object_index].content),
                tracks,
                &prepared_group,
                time,
                object.base_transform,
                object.base_style,
            );
            cursor_updates.insert(channel, cursor);
            stats.groups_evaluated += 1;
        }

        let mut deferred = HashSet::new();
        for (&object_index, row) in &mut rows {
            if defer_reactive_bindings {
                for property in self.reactive_properties_for_object(object_index) {
                    deferred.insert((object_index, property));
                }
            } else {
                self.reapply_reactive_to_row(
                    object_index,
                    row.as_mut(&self.frame.objects[object_index].content),
                );
            }
        }

        let reactive = if reactive_inputs.is_empty() {
            None
        } else {
            Some(
                self.reactive
                    .as_mut()
                    .ok_or_else(|| {
                        EvaluationError::Reactive(noon_core::ReactiveError::UnknownSignal(
                            reactive_inputs[0].0,
                        ))
                    })?
                    .prepare_input_batch(reactive_inputs)
                    .map_err(EvaluationError::Reactive)?,
            )
        };
        if let Some(reactive) = reactive.as_ref() {
            for (object_index, property, value) in reactive.property_changes() {
                if !self.object_slot_is_live(*object_index) {
                    continue;
                }
                if defer_reactive_bindings {
                    deferred.insert((*object_index, *property));
                    continue;
                }
                let row = rows
                    .entry(*object_index)
                    .or_insert_with(|| FrameRowState::from_frame(&self.frame, *object_index));
                apply_reactive_value_to_row(
                    &mut row.as_mut(&self.frame.objects[*object_index].content),
                    *property,
                    value,
                );
            }
        }

        // Pointer translation leases join after ordinary timeline/reactive
        // evaluation. They own only Position, retaining the rest of the row's
        // current effective channels and requiring no frame tick while idle.
        for (&object_index, &translation) in &self.translation_drag_rows {
            if !self.object_slot_is_live(object_index) {
                continue;
            }
            let row = rows
                .entry(object_index)
                .or_insert_with(|| FrameRowState::from_frame(&self.frame, object_index));
            row.transform.translation = translation;
        }

        let numeric_text = match reactive.as_ref() {
            Some(reactive) => self
                .prepare_changed_numeric_text(reactive)
                .map_err(EvaluationError::NumericText)?,
            None => Vec::new(),
        };

        self.refresh_prepared_graph_dependencies(&mut rows);

        Ok(PreparedFrameEvaluation {
            runtime: self.identity,
            expected: self.publication,
            base_time: self.frame.time,
            time,
            requested_channels: preview.requested().to_vec(),
            requested_family_animations: preview.requested_family_animations().to_vec(),
            cursor_updates: cursor_updates.into_iter().collect(),
            rows: rows
                .into_iter()
                .map(|(object_index, state)| PreparedFrameRow {
                    object_index,
                    state,
                })
                .collect(),
            stats,
            scheduler_stats: preview.stats(),
            prior_driver_rows,
            reactive,
            deferred_reactive_bindings: deferred
                .into_iter()
                .map(|(index, property)| (self.compiled.objects()[index].id, property))
                .collect(),
            numeric_text,
            authored_scalar_inputs: false,
        })
    }

    /// Validate every effective write against the current execution shape before
    /// retaining only the final ordered value for each object/property.
    pub fn prepare_effective_property_batch(
        &self,
        writes: &[EffectivePropertyWrite],
    ) -> Result<PreparedEffectivePropertyBatch, CompilePatchError> {
        let transaction = ExecutionMutationTransaction::from_mutations(
            writes
                .iter()
                .copied()
                .map(EffectivePropertyWrite::as_execution_patch),
        );
        self.compiled
            .preflight_execution_transaction(&transaction)?;

        let mut seen = HashSet::new();
        let mut retained = Vec::with_capacity(writes.len());
        for write in writes.iter().copied().rev() {
            // Whole-domain and component writes overlap. Preserve their supplied
            // order, coalescing only later writes of the exact same shape.
            // Even superseded writes have already passed validation above.
            let property_tag = std::mem::discriminant(&write);
            if !seen.insert((write.object(), property_tag)) {
                continue;
            }
            let object_index = self
                .compiled
                .object_index(write.object())
                .expect("effective write was validated against the compiled scene")
                as usize;
            retained.push((object_index, write));
        }
        retained.reverse();
        Ok(PreparedEffectivePropertyBatch {
            runtime: self.identity,
            expected: self.publication,
            writes: retained,
        })
    }

    /// Atomically publish one prepared timeline/native phase plus its final host
    /// effective writes. All fallible validation precedes scheduler/frame mutation.
    pub fn preflight_prepared_frame_commit(
        &self,
        prepared: &PreparedFrameEvaluation,
        effective: &PreparedEffectivePropertyBatch,
    ) -> Result<(), PreparedFrameCommitError> {
        if self.replay_is_sealed() {
            return Err(PreparedFrameCommitError::ReplaySealed);
        }
        if prepared.runtime != self.identity {
            return Err(PreparedFrameCommitError::ForeignRuntime {
                expected: self.identity,
                actual: prepared.runtime,
            });
        }
        if effective.runtime != self.identity {
            return Err(PreparedFrameCommitError::ForeignRuntime {
                expected: self.identity,
                actual: effective.runtime,
            });
        }
        if prepared.expected != self.publication {
            return Err(PreparedFrameCommitError::StalePublication {
                expected: prepared.expected,
                actual: self.publication,
            });
        }
        if effective.expected != self.publication {
            return Err(PreparedFrameCommitError::StalePublication {
                expected: effective.expected,
                actual: self.publication,
            });
        }
        if prepared.base_time != self.frame.time {
            return Err(PreparedFrameCommitError::StaleTime {
                expected: prepared.base_time,
                actual: self.frame.time,
            });
        }
        let may_publish = self.prepared_frame_may_change(prepared) || !effective.writes.is_empty();
        if may_publish && self.publication.frame_epoch().checked_next().is_none() {
            return Err(PreparedFrameCommitError::FrameEpochExhausted(
                self.publication.frame_epoch(),
            ));
        }
        Ok(())
    }

    pub(crate) fn prepared_frame_may_change(&self, prepared: &PreparedFrameEvaluation) -> bool {
        prepared.time != self.frame.time
            || !prepared.rows.is_empty()
            || !prepared.requested_family_animations.is_empty()
            || !self.pending_family_endpoint_expirations.is_empty()
            || prepared
                .reactive
                .as_ref()
                .is_some_and(|update| !update.is_empty())
            || !prepared.numeric_text.is_empty()
    }

    pub fn commit_prepared_frame(
        &mut self,
        prepared: PreparedFrameEvaluation,
        effective: PreparedEffectivePropertyBatch,
    ) -> Result<&FrameState, PreparedFrameCommitError> {
        self.commit_prepared_frame_inner(prepared, effective, true)?;
        Ok(&self.frame)
    }

    /// The caller holds the complete authored-publication proof and publishes
    /// its final context in the same exclusive call. Never expose this suffix.
    pub(crate) fn commit_prepared_frame_unpublished(
        &mut self,
        prepared: PreparedFrameEvaluation,
        effective: PreparedEffectivePropertyBatch,
    ) -> Result<bool, PreparedFrameCommitError> {
        self.commit_prepared_frame_inner(prepared, effective, false)
    }

    pub(crate) fn commit_prepared_frame_inner(
        &mut self,
        prepared: PreparedFrameEvaluation,
        effective: PreparedEffectivePropertyBatch,
        publish: bool,
    ) -> Result<bool, PreparedFrameCommitError> {
        self.preflight_prepared_frame_commit(&prepared, &effective)?;
        let may_publish = self.prepared_frame_may_change(&prepared) || !effective.writes.is_empty();
        let next_frame = if may_publish {
            Some(self.publication.frame_epoch().checked_next().ok_or(
                PreparedFrameCommitError::FrameEpochExhausted(self.publication.frame_epoch()),
            )?)
        } else {
            None
        };

        // Preparing an input is speculative. Classify only a validated commit,
        // and do not poison finite history for an unchanged input/no-op frame.
        let reactive_changed = prepared
            .reactive
            .as_ref()
            .is_some_and(|update| !update.is_empty());
        if reactive_changed && !prepared.authored_scalar_inputs {
            self.invalidate_replay_input();
        } else if reactive_changed && self.has_recorded_replay_inputs() {
            self.invalidate_replay_domain();
        } else if !effective.is_empty() || !self.effective_driver_rows.is_empty() {
            self.invalidate_replay_domain();
        }

        self.timeline_scheduler.advance(prepared.time);
        debug_assert_eq!(
            self.timeline_scheduler.requested(),
            prepared.requested_channels
        );
        debug_assert_eq!(
            self.timeline_scheduler.requested_family_animations(),
            prepared.requested_family_animations
        );
        for (channel, cursor) in &prepared.cursor_updates {
            if let Some(group) = self.groups.get_mut(channel) {
                group.cursor = *cursor;
            }
        }
        if let Some(reactive) = prepared.reactive {
            self.last_reactive_stats = self
                .reactive
                .as_mut()
                .expect("prepared reactive update retains its runtime")
                .commit_prepared_input_batch(reactive);
        }

        let mut final_rows = prepared
            .rows
            .into_iter()
            .map(|row| (row.object_index, row.state))
            .collect::<BTreeMap<_, _>>();
        let mut next_drivers = BTreeSet::new();
        next_drivers.extend(
            self.numeric_text
                .object_indices()
                .filter(|&object_index| self.object_slot_is_live(object_index)),
        );
        for (object_index, write) in effective.writes {
            let row = final_rows
                .entry(object_index)
                .or_insert_with(|| FrameRowState::from_frame(&self.frame, object_index));
            apply_effective_property_to_row(
                row.as_mut(&self.frame.objects[object_index].content),
                write,
            );
            next_drivers.insert(object_index);
        }

        let time_changed = self.frame.time != prepared.time;
        self.frame.time = prepared.time;
        let mut changed = self.update_requested_family_animations(prepared.time);
        for (object_index, row) in final_rows {
            if row.differs_from_frame(&self.frame, object_index) {
                let priority_changed = row.z_index != self.frame.objects[object_index].z_index;
                row.write_to_frame(&mut self.frame, object_index);
                if priority_changed {
                    self.reposition_painter_row(object_index);
                }
                self.mark_changed(object_index);
                changed = true;
            }
        }
        self.effective_driver_rows = next_drivers;
        self.commit_numeric_text_updates(prepared.numeric_text, true);
        self.last_stats = prepared.stats;
        let changed = time_changed || changed || reactive_changed;
        if publish && changed {
            self.publication = self.publication.with_frame_epoch(
                next_frame.expect("a changed prepared frame reserved a frame epoch"),
            );
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{
        lower_semantic_execution, CompilePatchError, CompiledObject, CompiledScene, ExecutionPatch,
        SemanticExecutionIndex,
    };
    use noon_core::{
        CompositionTimeMap, GeometryRef, GeometryResourceArena, ObjectId, Property,
        RasterImageResourceArena, RateFunction, SemanticImageContent, SemanticObjectProperty,
        SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Style, TrackDefinition,
        TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
    };

    use super::*;

    fn compile_linear_scene() -> CompiledScene {
        let object = ObjectId::new(1);
        let compiled = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let track = TrackDefinition {
            id: TrackId::new(1),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(1.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::default(),
        };
        CompiledScene::compile_objects(vec![compiled], &[track]).expect("scene must compile")
    }

    #[test]
    fn prepared_bounds_resolve_leased_path_and_image_for_staged_transform() {
        let object = ObjectId::new(1);
        let compiled = CompiledObject::new(
            object,
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        );
        let track = TrackDefinition {
            id: TrackId::new(1),
            object,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::default(),
        };
        let mut instance =
            SceneInstance::new(CompiledScene::compile_objects(vec![compiled], &[track]).unwrap());
        let mut source = GeometryResourceArena::new();
        let handle = source.insert_path(
            VectorPath::new()
                .move_to(Vec2::new(-2.0, -1.0))
                .line_to(Vec2::new(2.0, 3.0)),
        );
        let replacement = instance
            .prepare_effective_geometry_replacement(object, handle, &source, None)
            .unwrap();
        let lease = instance
            .commit_effective_content_replacement(replacement)
            .unwrap();

        let prepared = instance.prepare_advance_to(1.0).unwrap();
        let properties = instance.prepared_properties_at(&prepared, 0, None).unwrap();
        assert_eq!(
            properties.bounds,
            Some(noon_core::Rect {
                min: Vec2::new(3.0, -1.0),
                max: Vec2::new(7.0, 3.0),
            })
        );

        instance.release_effective_content(lease).unwrap();
        let mut images = RasterImageResourceArena::new();
        let image = images.intern_rgba8(2, 1, vec![255; 8]).unwrap();
        let replacement = instance
            .prepare_effective_image_replacement(
                object,
                SemanticImageContent::new(image),
                &images,
                None,
            )
            .unwrap();
        instance
            .commit_effective_content_replacement(replacement)
            .unwrap();
        let prepared = instance.prepare_advance_to(1.0).unwrap();
        let properties = instance.prepared_properties_at(&prepared, 0, None).unwrap();
        assert_eq!(
            properties.bounds,
            Some(noon_core::Rect {
                min: Vec2::new(4.0, -0.5),
                max: Vec2::new(6.0, 0.5),
            })
        );
    }

    #[test]
    fn prepared_advance_keeps_frame_scheduler_and_publication_coherent_until_commit() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        instance.take_frame_changes();
        let before_frame = instance.frame().clone();
        let before_publication = instance.publication_context();
        let before_scheduler = instance.last_timeline_scheduler_stats();
        let mut expected = instance.clone();
        expected.advance_to(2.0).unwrap();

        let prepared = instance.prepare_advance_to(2.0).unwrap();
        assert_eq!(instance.frame(), &before_frame);
        assert_eq!(instance.publication_context(), before_publication);
        assert_eq!(instance.last_timeline_scheduler_stats(), before_scheduler);
        assert_eq!(prepared.staged_row_count(), 1);
        assert_eq!(prepared.evaluation_stats().groups_evaluated, 1);

        let effective = instance.prepare_effective_property_batch(&[]).unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(instance.frame(), expected.frame());
        assert_eq!(
            instance.publication_context(),
            expected.publication_context()
        );
        assert_eq!(
            instance.last_timeline_scheduler_stats(),
            expected.last_timeline_scheduler_stats()
        );
    }

    #[test]
    fn timeline_owned_component_is_recomputed_before_next_host_phase() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let object = instance.frame().objects[0].id;
        instance.take_frame_changes();
        let prepared = instance.prepare_advance_to(2.0).unwrap();
        let effective = instance
            .prepare_effective_property_batch(&[EffectivePropertyWrite::Transform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(40.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            }])
            .unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(40.0, 0.0)
        );

        let before = instance.frame().clone();
        let prepared = instance.prepare_advance_to(2.5).unwrap();
        assert_eq!(prepared.prior_driver_rows(), 1);
        assert_eq!(prepared.staged_row_count(), 1);
        assert_eq!(
            instance
                .prepared_properties_at(&prepared, 0, None)
                .unwrap()
                .transform
                .translation,
            Vec2::new(7.5, 0.0)
        );
        assert_eq!(instance.frame(), &before);

        let effective = instance.prepare_effective_property_batch(&[]).unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(7.5, 0.0)
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn future_timeline_channel_does_not_reset_unowned_host_state() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let object = instance.frame().objects[0].id;
        let prepared = instance.prepare_advance_to(0.25).unwrap();
        let effective = instance
            .prepare_effective_property_batch(&[EffectivePropertyWrite::Transform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(4.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            }])
            .unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();

        let prepared = instance.prepare_advance_to(0.5).unwrap();
        assert_eq!(prepared.prior_driver_rows(), 1);
        assert_eq!(
            instance
                .prepared_properties_at(&prepared, 0, None)
                .unwrap()
                .transform
                .translation,
            Vec2::new(4.0, 0.0)
        );
    }

    #[test]
    fn native_owned_component_is_reapplied_before_next_host_phase() {
        let mut store = SemanticStore::new();
        let object =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 1.0,
            }));
        store.attach_to_scene(object).unwrap();
        let position = store
            .insert_semantic_input_signal(SemanticVec3::new(3.0, -1.0, 0.0))
            .unwrap();
        store
            .bind_semantic_signal(position, object, SemanticObjectProperty::Translation)
            .unwrap();
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let object = index.execution_object_id(object).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        instance.take_frame_changes();

        let prepared = instance.prepare_advance_to(0.5).unwrap();
        let effective = instance
            .prepare_effective_property_batch(&[EffectivePropertyWrite::Transform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(40.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            }])
            .unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(40.0, 0.0)
        );

        let prepared = instance.prepare_advance_to(1.0).unwrap();
        assert_eq!(prepared.prior_driver_rows(), 1);
        assert_eq!(
            instance
                .prepared_properties_at(&prepared, 0, None)
                .unwrap()
                .transform
                .translation,
            Vec2::new(3.0, -1.0)
        );
        let effective = instance.prepare_effective_property_batch(&[]).unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(
            instance.frame().objects[0].transform.translation,
            Vec2::new(3.0, -1.0)
        );
    }

    #[test]
    fn invalid_effective_batch_leaves_prepared_scheduler_and_frame_unpublished() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        instance.take_frame_changes();
        let object = instance.frame().objects[0].id;
        let before_frame = instance.frame().clone();
        let before_publication = instance.publication_context();
        let before_scheduler = instance.last_timeline_scheduler_stats();
        let mut expected = instance.clone();
        expected.advance_to(2.0).unwrap();
        let prepared = instance.prepare_advance_to(2.0).unwrap();
        let error = instance
            .prepare_effective_property_batch(&[
                EffectivePropertyWrite::Transform {
                    object,
                    transform: Transform2D {
                        translation: Vec2::new(4.0, 0.0),
                        ..Transform2D::IDENTITY
                    },
                },
                EffectivePropertyWrite::Style {
                    object: ObjectId::new(u64::MAX),
                    style: Style::default(),
                },
            ])
            .unwrap_err();
        assert_eq!(
            error,
            CompilePatchError::UnknownObject(ObjectId::new(u64::MAX))
        );
        assert_eq!(instance.frame(), &before_frame);
        assert_eq!(instance.publication_context(), before_publication);
        assert_eq!(instance.last_timeline_scheduler_stats(), before_scheduler);
        assert!(instance.take_frame_changes().is_empty());
        let effective = instance.prepare_effective_property_batch(&[]).unwrap();
        instance.commit_prepared_frame(prepared, effective).unwrap();
        assert_eq!(instance.frame(), expected.frame());
        assert_eq!(
            instance.last_timeline_scheduler_stats(),
            expected.last_timeline_scheduler_stats()
        );
    }

    #[test]
    fn prepared_values_are_scoped_to_the_runtime_that_created_them() {
        let compiled = compile_linear_scene();
        let mut first = SceneInstance::new(compiled.clone());
        let second = SceneInstance::new(compiled);
        let prepared = first.prepare_advance_to(2.0).unwrap();
        let effective = first.prepare_effective_property_batch(&[]).unwrap();

        assert!(matches!(
            second.preflight_prepared_frame_commit(&prepared, &effective),
            Err(PreparedFrameCommitError::ForeignRuntime { .. })
        ));

        let cloned = first.clone();
        assert!(matches!(
            cloned.preflight_prepared_frame_commit(&prepared, &effective),
            Err(PreparedFrameCommitError::ForeignRuntime { .. })
        ));
    }

    #[test]
    fn execution_publication_invalidates_prepared_effective_values() {
        let mut instance = SceneInstance::new(compile_linear_scene());
        let prepared = instance.prepare_advance_to(2.0).unwrap();
        let effective = instance.prepare_effective_property_batch(&[]).unwrap();
        let object = instance.frame().objects[0].id;
        instance
            .apply_execution_patch(&ExecutionPatch::SetStyle {
                object,
                style: Style {
                    opacity: 0.5,
                    ..Style::default()
                },
            })
            .unwrap();

        assert!(matches!(
            instance.preflight_prepared_frame_commit(&prepared, &effective),
            Err(PreparedFrameCommitError::StalePublication { .. })
        ));
        assert_eq!(instance.frame().time, 0.0);
    }
}

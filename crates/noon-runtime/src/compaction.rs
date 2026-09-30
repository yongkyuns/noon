//! Explicit runtime maintenance for retired execution rows.

use noon_compile::{CompiledSceneCompactionError, CompiledSceneCompactionStats};
use noon_core::{ExecutionRevision, FrameEpoch, PublicationContext};

use crate::{
    base_frame, build_groups, FrameRowState, RuntimePatchStats, SceneInstance,
    TimelineEventScheduler,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeCompactionError {
    ReplayRetentionActive,
    EffectiveDriverActive,
    ReactiveTargetRetired,
    ExecutionRevisionExhausted(ExecutionRevision),
    FrameEpochExhausted(FrameEpoch),
    FamilyAnimationActive,
    UnsupportedCompiledState(CompiledSceneCompactionError),
}

impl std::fmt::Display for RuntimeCompactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReplayRetentionActive => formatter.write_str(
                "retired-row compaction cannot relocate rows retained by an active replay scope",
            ),
            Self::EffectiveDriverActive => formatter.write_str(
                "retired-row compaction cannot relocate rows with active effective drivers",
            ),
            Self::ReactiveTargetRetired => formatter.write_str(
                "retired-row compaction cannot discard a row still targeted by reactive state",
            ),
            Self::ExecutionRevisionExhausted(revision) => {
                write!(formatter, "execution revision exhausted after {revision:?}")
            }
            Self::FrameEpochExhausted(epoch) => {
                write!(formatter, "frame epoch exhausted after {epoch:?}")
            }
            Self::FamilyAnimationActive => formatter
                .write_str("retired-row compaction cannot relocate active family animation state"),
            Self::UnsupportedCompiledState(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RuntimeCompactionError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimeCompactionStats {
    pub compiled: CompiledSceneCompactionStats,
}

impl SceneInstance {
    /// Explicitly reclaim tombstoned execution rows and eligible static resources.
    ///
    /// The barrier visits retained slots and rebuilds live row-indexed structures.
    /// It preserves the runtime identity, authored scene revision, current time,
    /// and each live effective row by `ObjectId`, then publishes a new execution
    /// and frame revision so all previous derived receipts become stale.
    pub fn reclaim_retired_object_slots(
        &mut self,
    ) -> Result<RuntimeCompactionStats, RuntimeCompactionError> {
        if self.compiled.retired_object_slot_count() == 0 {
            // Replay may retain an older resource projection that this local
            // barrier cannot traverse. Active property/interaction drivers may
            // depend on this execution revision. Effective content leases are
            // explicit roots below, so they need not pin unrelated history.
            if self.replay_history.is_some()
                || !self.effective_driver_rows.is_empty()
                || !self.translation_drag_rows.is_empty()
                || self.interactions_active()
            {
                let count = self.compiled.objects().len();
                return Ok(RuntimeCompactionStats {
                    compiled: CompiledSceneCompactionStats {
                        object_slots_before: count,
                        object_slots_after: count,
                        ..CompiledSceneCompactionStats::default()
                    },
                });
            }
            let retained_contents: Vec<_> = self
                .effective_content_drivers
                .values()
                .map(|driver| driver.content.clone())
                .collect();
            let resources = self.compiled.resources();
            // Resource pruning changes the compiled dependency closure even
            // when every execution row stays put. Reserve a fresh publication
            // before allowing that mutation so outstanding prepared work cannot
            // commit against a resource set it no longer owns.
            let next_publication = if resources.image_count() == 0
                && resources.text_count() == 0
                && resources.font_count() == 0
                && resources.geometry_count() == 0
            {
                None
            } else {
                let execution = self.publication.execution_revision().checked_next().ok_or(
                    RuntimeCompactionError::ExecutionRevisionExhausted(
                        self.publication.execution_revision(),
                    ),
                )?;
                let frame_epoch = self.publication.frame_epoch().checked_next().ok_or(
                    RuntimeCompactionError::FrameEpochExhausted(self.publication.frame_epoch()),
                )?;
                Some(PublicationContext::new(
                    self.publication.scene_revision(),
                    execution,
                    frame_epoch,
                ))
            };
            let compiled = self
                .compiled
                .compact_retired_object_slots_with_content_roots(&retained_contents)
                .map_err(RuntimeCompactionError::UnsupportedCompiledState)?;
            if compiled.resource_entries_reclaimed != 0 {
                self.publication =
                    next_publication.expect("reclaimed resource had a closure entry");
                if self.changes.is_empty() {
                    self.changes = crate::FrameChanges::presentation_redraw();
                }
            }
            return Ok(RuntimeCompactionStats { compiled });
        }
        if self.replay_history.is_some() && self.compiled.retired_object_slot_count() != 0 {
            return Err(RuntimeCompactionError::ReplayRetentionActive);
        }
        if !self.effective_driver_rows.is_empty()
            || !self.effective_content_drivers.is_empty()
            || !self.translation_drag_rows.is_empty()
            || self.interactions_active()
        {
            return Err(RuntimeCompactionError::EffectiveDriverActive);
        }
        if self
            .reactive
            .as_ref()
            .is_some_and(|reactive| reactive.has_retired_targets(&self.compiled))
        {
            return Err(RuntimeCompactionError::ReactiveTargetRetired);
        }
        let execution = self.publication.execution_revision().checked_next().ok_or(
            RuntimeCompactionError::ExecutionRevisionExhausted(
                self.publication.execution_revision(),
            ),
        )?;
        let frame_epoch = self.publication.frame_epoch().checked_next().ok_or(
            RuntimeCompactionError::FrameEpochExhausted(self.publication.frame_epoch()),
        )?;
        let next_publication =
            PublicationContext::new(self.publication.scene_revision(), execution, frame_epoch);
        if !self.active_family_animation_indices.is_empty()
            || !self.pending_family_endpoint_expirations.is_empty()
        {
            return Err(RuntimeCompactionError::FamilyAnimationActive);
        }

        let retained_rows: Vec<_> = self
            .frame
            .objects
            .iter()
            .enumerate()
            .filter(|(index, _)| self.object_slot_is_live(*index))
            .map(|(index, object)| (object.id, FrameRowState::from_frame(&self.frame, index)))
            .collect();
        let painter_ids: Vec<_> = self
            .painter_order
            .iter()
            .map(|&index| self.frame.objects[index as usize].id)
            .collect();
        let time = self.frame.time;
        let compiled = self
            .compiled
            .compact_retired_object_slots()
            .map_err(RuntimeCompactionError::UnsupportedCompiledState)?;
        let mut frame = base_frame(&self.compiled, time);
        for (index, (object, row)) in retained_rows.into_iter().enumerate() {
            debug_assert_eq!(object, self.compiled.objects()[index].id);
            row.write_to_frame(&mut frame, index);
        }
        self.frame = frame;
        if let Some(reactive) = self.reactive.as_mut() {
            reactive.remap_object_indices(&self.compiled);
        }
        self.painter_order = painter_ids
            .iter()
            .map(|id| {
                self.compiled
                    .object_index(*id)
                    .expect("painter order contains only live compacted ids")
            })
            .collect();
        self.painter_ranks = vec![None; self.frame.objects.len()];
        for (rank, &index) in self.painter_order.iter().enumerate() {
            self.painter_ranks[index as usize] = Some(rank as u32);
        }
        self.groups = build_groups(&self.compiled);
        for group in self.groups.values_mut() {
            let tracks = self.compiled.channel_tracks(group.channel);
            group.cursor = tracks.partition_point(|track| track.timing.start_time <= time);
        }
        self.timeline_scheduler = TimelineEventScheduler::from_compiled(&self.compiled);
        self.timeline_scheduler.seek(time);
        self.last_stats = Default::default();
        self.last_patch_stats = RuntimePatchStats::default();
        self.mark_all_changed();
        self.publication = next_publication;
        Ok(RuntimeCompactionStats { compiled })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_compile::{
        lower_semantic_execution, CompiledObject, CompiledScene, ExecutionPatch,
        SemanticExecutionIndex,
    };
    use noon_core::{
        Color, CompositionTimeMap, FontResourceArena, GeometryRef, GeometryResourceArena,
        ObjectContentRef, ObjectId, Property, RateFunction, Rect, SemanticClickIndicate,
        SemanticMutationTransaction, SemanticObjectContent, SemanticObjectProperty,
        SemanticObjectState, SemanticStore, SemanticVec3, StoredGeometry, Style, TextResource,
        TextResourceLookup, TextSourceKind, TrackDefinition, TrackId, TrackTiming, TrackValues,
        Transform2D, Vec2,
    };

    fn object(id: u64) -> CompiledObject {
        CompiledObject::new(
            ObjectId::new(id),
            GeometryRef::circle(1.0),
            Transform2D::IDENTITY,
            Style::default(),
        )
    }

    fn churn(runtime: &mut SceneInstance, first: u64, count: u64) {
        for id in first..first + count {
            let object = object(id);
            runtime
                .apply_execution_patch(&ExecutionPatch::CreateObject(object.clone()))
                .unwrap();
            runtime
                .apply_execution_patch(&ExecutionPatch::RemoveObject(object.id))
                .unwrap();
        }
    }

    fn compiled_text_history(
        sources: &[&str],
    ) -> (CompiledScene, Vec<noon_core::TextResourceHandle>) {
        let mut store = SemanticStore::new();
        let handles: Vec<_> = sources
            .iter()
            .map(|source| {
                store
                    .import_text_resource(
                        TextResource {
                            source: (*source).into(),
                            kind: TextSourceKind::Plain,
                            runs: Default::default(),
                            vector_items: Default::default(),
                            render_items: Default::default(),
                            parts: Default::default(),
                            bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
                            baseline: 0.0,
                            layout_artifact: None,
                        },
                        &FontResourceArena::new(),
                        &GeometryResourceArena::new(),
                    )
                    .unwrap()
            })
            .collect();
        let target = store.insert_semantic_object(SemanticObjectState::new(handles[0]));
        store.attach_to_scene(target).unwrap();
        let mut compiled = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new())
            .unwrap()
            .compiled()
            .clone();
        for &handle in handles.iter().skip(1) {
            let previous = compiled.resources().clone();
            let mut edit = SemanticMutationTransaction::new();
            edit.replace_content(target, SemanticObjectContent::Text(handle));
            edit.apply(&mut store).unwrap();
            compiled = lower_semantic_execution(&store, &mut SemanticExecutionIndex::new())
                .unwrap()
                .compiled()
                .clone();
            compiled.merge_prepared_resources(previous);
        }
        (compiled, handles)
    }

    #[test]
    fn explicit_compaction_bounds_repeated_churn_and_preserves_live_frame() {
        let survivor = ObjectId::new(1);
        let track = TrackDefinition {
            id: TrackId::new(1),
            object: survivor,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(8.0, 0.0),
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        };
        let compiled = CompiledScene::compile_objects(vec![object(1)], &[track]).unwrap();
        let mut runtime = SceneInstance::new(compiled.clone());
        let mut reference = SceneInstance::new(compiled);
        runtime.advance_to(1.0).unwrap();
        reference.advance_to(1.0).unwrap();
        runtime.take_frame_changes();
        churn(&mut runtime, 10, 32);
        let before = runtime.effective_object(survivor).unwrap().clone();
        let identity = runtime.runtime_identity();
        let publication = runtime.publication_context();
        assert_eq!(runtime.frame().objects.len(), 33);

        let stats = runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(stats.compiled.object_slots_before, 33);
        assert_eq!(stats.compiled.object_slots_after, 1);
        assert_eq!(stats.compiled.object_slots_reclaimed, 32);
        assert_eq!(runtime.runtime_identity(), identity);
        assert_eq!(runtime.frame().time, 1.0);
        assert_eq!(runtime.effective_object(survivor), Some(&before));
        assert_eq!(runtime.frame().objects.len(), 1);
        assert_eq!(
            runtime.publication_context().scene_revision(),
            publication.scene_revision()
        );
        assert_eq!(
            runtime.publication_context().execution_revision(),
            publication.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            runtime.publication_context().frame_epoch(),
            publication.frame_epoch().checked_next().unwrap()
        );
        assert!(runtime.take_frame_changes().is_all());

        runtime.advance_to(1.5).unwrap();
        reference.advance_to(1.5).unwrap();
        assert_eq!(
            runtime.effective_object(survivor),
            reference.effective_object(survivor)
        );

        churn(&mut runtime, 100, 32);
        runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(runtime.frame().objects.len(), 1);
    }

    #[test]
    fn no_row_compaction_preserves_a_live_effective_content_lease() {
        let mut runtime =
            SceneInstance::new(CompiledScene::compile_objects(vec![object(1)], &[]).unwrap());
        let prepared = runtime
            .prepare_effective_content_replacement(
                ObjectId::new(1),
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let lease = runtime
            .commit_effective_content_replacement(prepared)
            .unwrap();
        let publication = runtime.publication_context();

        let stats = runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(stats.compiled.object_slots_reclaimed, 0);
        assert_eq!(stats.compiled.resource_entries_reclaimed, 0);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(
            runtime.effective_content_lease(ObjectId::new(1)),
            Some(lease)
        );
    }

    #[test]
    fn static_resource_pruning_invalidates_prepared_publication_without_repacking_rows() {
        let (current, handles) = compiled_text_history(&["old", "new"]);
        let [old, replacement] = [handles[0], handles[1]];
        assert_eq!(current.resources().text_count(), 2);

        let mut runtime = SceneInstance::new(current);
        runtime.take_frame_changes();
        let before = runtime.publication_context();
        let prepared = runtime
            .prepare_effective_content_replacement(
                runtime.frame().objects[0].id,
                ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                None,
                None,
            )
            .unwrap();
        let stats = runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(stats.compiled.object_slots_reclaimed, 0);
        assert_eq!(stats.compiled.resource_entries_reclaimed, 1);
        assert_eq!(
            runtime.publication_context().scene_revision(),
            before.scene_revision()
        );
        assert_eq!(
            runtime.publication_context().execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            runtime.publication_context().frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        let changes = runtime.take_frame_changes();
        assert!(changes.requires_presentation_redraw());
        assert!(!changes.has_stable_changes());
        assert!(matches!(
            runtime.commit_effective_content_replacement(prepared),
            Err(crate::EffectiveContentError::StalePublication { .. })
        ));
        assert!(TextResourceLookup::get(runtime.text_resources(), old).is_none());
        assert!(TextResourceLookup::get(runtime.text_resources(), replacement).is_some());
    }

    #[test]
    fn property_animation_does_not_pin_obsolete_text_versions() {
        let (compiled, handles) = compiled_text_history(&["old", "current"]);
        let [old, current] = [handles[0], handles[1]];
        let mut runtime = SceneInstance::new(compiled);
        let object = runtime.frame().objects[0].id;
        runtime
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(1),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::ONE,
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }))
            .unwrap();
        runtime.advance_to(0.5).unwrap();
        let before = runtime.effective_object(object).unwrap().clone();

        let stats = runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(stats.compiled.resource_entries_reclaimed, 1);
        assert_eq!(runtime.effective_object(object), Some(&before));
        assert!(TextResourceLookup::get(runtime.text_resources(), old).is_none());
        assert!(TextResourceLookup::get(runtime.text_resources(), current).is_some());
        runtime.advance_to(1.0).unwrap();
    }

    #[test]
    fn content_lease_roots_only_its_own_resource_history() {
        let (compiled, handles) = compiled_text_history(&["held", "obsolete", "authored"]);
        let [held, obsolete, authored] = [handles[0], handles[1], handles[2]];
        assert_eq!(compiled.resources().text_count(), 3);
        let mut runtime = SceneInstance::new(compiled);
        let object = runtime.frame().objects[0].id;
        let prepared = runtime
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Text(held),
                Some(Rect::new(Vec2::ZERO, Vec2::ZERO)),
                None,
            )
            .unwrap();
        let lease = runtime
            .commit_effective_content_replacement(prepared)
            .unwrap();
        runtime.take_frame_changes();
        let before = runtime.publication_context();

        let stats = runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(stats.compiled.object_slots_reclaimed, 0);
        assert_eq!(stats.compiled.resource_entries_reclaimed, 1);
        assert_eq!(runtime.effective_content_lease(object), Some(lease));
        assert_eq!(
            runtime.frame().objects[0].content,
            ObjectContentRef::Text(held)
        );
        assert_eq!(
            runtime.publication_context().scene_revision(),
            before.scene_revision()
        );
        assert_ne!(
            runtime.publication_context().execution_revision(),
            before.execution_revision()
        );
        assert!(runtime.take_frame_changes().requires_presentation_redraw());
        assert!(TextResourceLookup::get(runtime.text_resources(), held).is_some());
        assert!(TextResourceLookup::get(runtime.text_resources(), authored).is_some());
        assert!(TextResourceLookup::get(runtime.text_resources(), obsolete).is_none());

        let next = runtime
            .prepare_effective_content_replacement(
                object,
                ObjectContentRef::Text(authored),
                Some(Rect::new(Vec2::ZERO, Vec2::ZERO)),
                Some(lease),
            )
            .unwrap();
        assert_eq!(
            runtime.commit_effective_content_replacement(next).unwrap(),
            lease
        );
        assert_eq!(
            runtime
                .reclaim_retired_object_slots()
                .unwrap()
                .compiled
                .resource_entries_reclaimed,
            1
        );
        assert!(TextResourceLookup::get(runtime.text_resources(), held).is_none());
        assert!(TextResourceLookup::get(runtime.text_resources(), authored).is_some());
        runtime.release_effective_content(lease).unwrap();
        assert_eq!(
            runtime.frame().objects[0].content,
            ObjectContentRef::Text(authored)
        );
    }

    #[test]
    fn resource_only_compaction_waits_for_an_active_interaction() {
        let (compiled, handles) = compiled_text_history(&["obsolete", "authored"]);
        let mut runtime = SceneInstance::new(compiled);
        let target = ObjectId::new(99);
        runtime
            .apply_execution_patch(&ExecutionPatch::CreateObject(object(99)))
            .unwrap();
        let effect = runtime
            .prepare_click_indicate(target, SemanticClickIndicate::new(1.2, Color::YELLOW, 1.0))
            .unwrap()
            .unwrap();
        runtime.start_transient_animation(effect).unwrap();
        let publication = runtime.publication_context();

        assert_eq!(
            runtime
                .reclaim_retired_object_slots()
                .unwrap()
                .compiled
                .resource_entries_reclaimed,
            0
        );
        assert_eq!(runtime.publication_context(), publication);
        assert!(runtime.interactions_active());
        runtime.advance_interactions(0.0).unwrap();
        runtime.advance_interactions(0.5).unwrap();
        assert!(runtime.interactions_active());
        runtime.advance_interactions(1.0).unwrap();
        assert!(!runtime.interactions_active());
        assert_eq!(
            runtime
                .reclaim_retired_object_slots()
                .unwrap()
                .compiled
                .resource_entries_reclaimed,
            1
        );
        assert!(TextResourceLookup::get(runtime.text_resources(), handles[0]).is_none());
        assert!(TextResourceLookup::get(runtime.text_resources(), handles[1]).is_some());
    }

    #[test]
    fn active_interaction_rejects_compaction_without_changing_the_frame() {
        let survivor = ObjectId::new(1);
        let mut runtime =
            SceneInstance::new(CompiledScene::compile_objects(vec![object(1)], &[]).unwrap());
        churn(&mut runtime, 10, 1);
        let effect = runtime
            .prepare_click_indicate(
                survivor,
                SemanticClickIndicate::new(1.2, Color::YELLOW, 1.0),
            )
            .unwrap()
            .unwrap();
        runtime.start_transient_animation(effect).unwrap();
        let before = runtime.frame().clone();
        let publication = runtime.publication_context();
        assert_eq!(
            runtime.reclaim_retired_object_slots(),
            Err(RuntimeCompactionError::EffectiveDriverActive)
        );
        assert_eq!(runtime.frame(), &before);
        assert_eq!(runtime.publication_context(), publication);
    }

    #[test]
    fn exhausted_revision_rejects_before_compaction_writes() {
        let mut runtime =
            SceneInstance::new(CompiledScene::compile_objects(vec![object(1)], &[]).unwrap());
        churn(&mut runtime, 10, 1);
        runtime.take_frame_changes();
        let base = runtime.publication_context();
        runtime.publication = PublicationContext::new(
            base.scene_revision(),
            noon_core::ExecutionRevision::new(u64::MAX),
            base.frame_epoch(),
        );
        let frame = runtime.frame().clone();
        assert_eq!(
            runtime.reclaim_retired_object_slots(),
            Err(RuntimeCompactionError::ExecutionRevisionExhausted(
                noon_core::ExecutionRevision::new(u64::MAX)
            ))
        );
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.frame().objects.len(), 2);
        assert!(runtime.take_frame_changes().is_empty());
    }

    #[test]
    fn compaction_remaps_reactive_targets_without_losing_current_or_future_values() {
        let mut scene = SemanticStore::new();
        let node = scene.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }));
        scene.attach_to_scene(node).unwrap();
        let input = scene
            .insert_semantic_input_signal(SemanticVec3::new(1.0, 2.0, 0.0))
            .unwrap();
        scene
            .bind_semantic_signal(input, node, SemanticObjectProperty::Translation)
            .unwrap();
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&scene, &mut index).unwrap();
        let target = index.execution_object_id(node).unwrap();
        let input = lowered.reactive().execution_signal_id(input).unwrap();
        let mut runtime = SceneInstance::from_semantic_execution(lowered);
        runtime
            .set_reactive_input(input, Vec2::new(3.0, 4.0))
            .unwrap();
        churn(&mut runtime, 10, 4);

        runtime.reclaim_retired_object_slots().unwrap();
        assert_eq!(
            runtime.effective_transform(target).unwrap().translation,
            Vec2::new(3.0, 4.0)
        );
        runtime
            .set_reactive_input(input, Vec2::new(5.0, 6.0))
            .unwrap();
        assert_eq!(
            runtime.effective_transform(target).unwrap().translation,
            Vec2::new(5.0, 6.0)
        );
    }
}

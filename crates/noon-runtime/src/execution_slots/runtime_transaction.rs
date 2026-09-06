use noon_compile::{
    CompilePatchError, CompiledResources, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    ExecutionRevision, FrameEpoch, MutationTransaction, ObjectId, PublicationContext,
    SceneRevision, Transform2D,
};
use std::collections::HashSet;

use crate::{FrameObjectState, FrameState, SceneInstance};

/// Retain the final value write in each region bounded by structural/timeline
/// edits. The caller must preflight all writes, including superseded ones.
pub(super) fn final_value_writes(
    transaction: &ExecutionMutationTransaction,
) -> Vec<&ExecutionPatch> {
    let mut final_writes = HashSet::new();
    let mut retained = Vec::with_capacity(transaction.mutations().len());
    for patch in transaction.mutations().iter().rev() {
        match patch {
            ExecutionPatch::SetContent { object, .. }
            | ExecutionPatch::SetTransform { object, .. }
            | ExecutionPatch::SetStyle { object, .. } => {
                if !final_writes.insert((*object, std::mem::discriminant(patch))) {
                    continue;
                }
            }
            _ => final_writes.clear(),
        }
        retained.push(patch);
    }
    retained.reverse();
    retained
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthoredPublicationError {
    StalePublication {
        expected: PublicationContext,
        actual: PublicationContext,
    },
    InvalidSceneRevision {
        current: SceneRevision,
        proposed: SceneRevision,
    },
    ExecutionRevisionExhausted(ExecutionRevision),
    FrameEpochExhausted(FrameEpoch),
    Compile(CompilePatchError),
}

impl std::fmt::Display for AuthoredPublicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StalePublication { expected, actual } => write!(
                formatter,
                "expected publication context {expected:?}, found {actual:?}"
            ),
            Self::InvalidSceneRevision { current, proposed } => write!(
                formatter,
                "authored publication scene revision must remain at {current:?} or advance once, got {proposed:?}"
            ),
            Self::ExecutionRevisionExhausted(revision) => {
                write!(formatter, "execution revision exhausted after {revision:?}")
            }
            Self::FrameEpochExhausted(epoch) => {
                write!(formatter, "frame epoch exhausted after {epoch:?}")
            }
            Self::Compile(error) => write!(formatter, "authored transaction failed: {error}"),
        }
    }
}

impl std::error::Error for AuthoredPublicationError {}

impl From<CompilePatchError> for AuthoredPublicationError {
    fn from(value: CompilePatchError) -> Self {
        Self::Compile(value)
    }
}

impl SceneInstance {
    /// Resolve one live object through the stable execution index without scanning
    /// unrelated frame objects. Removed objects have no live index entry.
    pub fn effective_object(&self, id: ObjectId) -> Option<&FrameObjectState> {
        let object_index = self.compiled.object_index(id)? as usize;
        self.frame.objects.get(object_index)
    }

    /// Resolve one live execution object to its current effective transform.
    ///
    /// The stable compiled object index is used to address the corresponding frame
    /// slot directly; callers do not scan renderer-facing frame objects by identity.
    pub fn effective_transform(&self, id: ObjectId) -> Option<Transform2D> {
        self.effective_object(id).map(|object| object.transform)
    }

    /// Finish every fallible runtime check available before transaction-local
    /// semantic identities are assigned. `transaction` contains value edits and
    /// conservative removals; `additional_objects` reserves append-only rows.
    pub fn preflight_authored_transaction_shape(
        &self,
        transaction: &ExecutionMutationTransaction,
        expected: PublicationContext,
        scene_revision: SceneRevision,
        additional_objects: usize,
        structural_change_possible: bool,
    ) -> Result<(), AuthoredPublicationError> {
        self.preflight_authored_transaction_shape_with_resources(
            transaction,
            &CompiledResources::default(),
            expected,
            scene_revision,
            additional_objects,
            structural_change_possible,
        )
    }

    pub fn preflight_authored_transaction_shape_with_resources(
        &self,
        transaction: &ExecutionMutationTransaction,
        resource_additions: &CompiledResources,
        expected: PublicationContext,
        scene_revision: SceneRevision,
        additional_objects: usize,
        structural_change_possible: bool,
    ) -> Result<(), AuthoredPublicationError> {
        let current = self.publication_context();
        if expected != current {
            return Err(AuthoredPublicationError::StalePublication {
                expected,
                actual: current,
            });
        }
        let scene_changed = scene_revision != current.scene_revision();
        if scene_changed && current.scene_revision().checked_next() != Some(scene_revision) {
            return Err(AuthoredPublicationError::InvalidSceneRevision {
                current: current.scene_revision(),
                proposed: scene_revision,
            });
        }
        self.compiled
            .preflight_execution_transaction_with_resources(transaction, resource_additions)?;
        self.compiled.preflight_object_appends(additional_objects)?;
        let execution_change_possible = structural_change_possible
            || final_value_writes(transaction)
                .into_iter()
                .any(|patch| self.compiled.patch_changes_execution(patch));
        if execution_change_possible && current.execution_revision().checked_next().is_none() {
            return Err(AuthoredPublicationError::ExecutionRevisionExhausted(
                current.execution_revision(),
            ));
        }
        if (scene_changed || execution_change_possible)
            && current.frame_epoch().checked_next().is_none()
        {
            return Err(AuthoredPublicationError::FrameEpochExhausted(
                current.frame_epoch(),
            ));
        }
        Ok(())
    }

    /// Publish one already-authored mutation transaction atomically into the runtime.
    ///
    /// The compiled scene preflights the complete transaction against staged
    /// identity/channel metadata before any frame-visible mutation occurs. Once that
    /// succeeds, each existing patch application is infallible by the compiled
    /// preflight contract, so no partially applied transaction can escape this call.
    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&FrameState, CompilePatchError> {
        let transaction = ExecutionMutationTransaction::decode(transaction);
        self.apply_execution_transaction(&transaction)
    }

    pub fn apply_execution_transaction(
        &mut self,
        transaction: &ExecutionMutationTransaction,
    ) -> Result<&FrameState, CompilePatchError> {
        self.compiled.preflight_execution_transaction(transaction)?;
        let changed = self.apply_preflighted_transaction(transaction);
        if changed {
            self.publish_execution_change();
        }
        Ok(&self.frame)
    }

    /// Atomically publish an authored semantic revision and its prepared executable edits.
    ///
    /// `expected` pins the complete live context observed by the caller. The supplied
    /// scene revision may remain current for execution-only work or advance by exactly
    /// one for a committed authored transaction. All compile validation and revision
    /// capacity checks finish before the first runtime write.
    pub fn apply_authored_transaction(
        &mut self,
        transaction: &MutationTransaction,
        expected: PublicationContext,
        scene_revision: SceneRevision,
    ) -> Result<&FrameState, AuthoredPublicationError> {
        let transaction = ExecutionMutationTransaction::decode(transaction);
        self.apply_authored_execution_transaction(
            &transaction,
            CompiledResources::default(),
            expected,
            scene_revision,
        )
    }

    pub fn apply_authored_execution_transaction(
        &mut self,
        transaction: &ExecutionMutationTransaction,
        resource_additions: CompiledResources,
        expected: PublicationContext,
        scene_revision: SceneRevision,
    ) -> Result<&FrameState, AuthoredPublicationError> {
        let current = self.publication_context();
        if expected != current {
            return Err(AuthoredPublicationError::StalePublication {
                expected,
                actual: current,
            });
        }

        let scene_changed = scene_revision != current.scene_revision();
        if scene_changed && current.scene_revision().checked_next() != Some(scene_revision) {
            return Err(AuthoredPublicationError::InvalidSceneRevision {
                current: current.scene_revision(),
                proposed: scene_revision,
            });
        }

        self.compiled
            .preflight_execution_transaction_with_resources(transaction, &resource_additions)?;
        let execution_changed = final_value_writes(transaction)
            .into_iter()
            .any(|patch| self.compiled.patch_changes_execution(patch));
        let next_execution = if execution_changed {
            Some(current.execution_revision().checked_next().ok_or(
                AuthoredPublicationError::ExecutionRevisionExhausted(current.execution_revision()),
            )?)
        } else {
            None
        };
        let frame_changed = scene_changed || execution_changed;
        let next_frame = if frame_changed {
            Some(current.frame_epoch().checked_next().ok_or(
                AuthoredPublicationError::FrameEpochExhausted(current.frame_epoch()),
            )?)
        } else {
            None
        };

        self.compiled.merge_prepared_resources(resource_additions);
        let applied_execution_change = self.apply_preflighted_transaction(transaction);
        debug_assert_eq!(applied_execution_change, execution_changed);
        if frame_changed {
            self.publication = PublicationContext::new(
                scene_revision,
                next_execution.unwrap_or(current.execution_revision()),
                next_frame.expect("changed authored publication reserved a frame epoch"),
            );
        }
        Ok(&self.frame)
    }

    fn apply_preflighted_transaction(
        &mut self,
        transaction: &ExecutionMutationTransaction,
    ) -> bool {
        self.last_patch_stats = crate::RuntimePatchStats::default();
        // Intermediate value writes in an atomic batch are not observable. Keep
        // the last write per property, bounded by structural/timeline operations
        // that may change what a property write means. Full preflight above still
        // validates overwritten writes rather than hiding malformed input.
        let mut changed = false;
        for patch in final_value_writes(transaction) {
            if !self.compiled.patch_changes_execution(patch) {
                continue;
            }
            self.apply_patch_unpublished(patch)
                .expect("runtime transaction was fully preflighted");
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
    use noon_core::{
        CompositionTimeMap, FontResourceArena, GeometryResourceArena, Property, RateFunction, Rect,
        ScenePatch, SemanticObjectState, SemanticStore, StoredGeometry, Style, TextResource,
        TextSourceKind, TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
    };

    use super::*;

    fn semantic_instances<const N: usize>(radii: [f32; N]) -> (SceneInstance, [ObjectId; N]) {
        let mut store = SemanticStore::new();
        let semantic_objects = radii.map(|radius| {
            let object =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius,
                }));
            store.attach_to_scene(object).unwrap();
            object
        });
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let objects = semantic_objects.map(|object| index.execution_object_id(object).unwrap());
        (SceneInstance::from_semantic_execution(lowered), objects)
    }

    fn semantic_instance() -> (SceneInstance, ObjectId) {
        let (instance, [object]) = semantic_instances([1.0]);
        (instance, object)
    }

    #[test]
    fn effective_transform_uses_the_stable_compiled_object_slot() {
        let (mut instance, object) = semantic_instance();
        let transform = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: 0.25,
            scale: Vec2::new(2.0, 0.5),
        };

        instance
            .apply_patch(&ScenePatch::SetTransform { object, transform })
            .unwrap();

        assert_eq!(instance.effective_transform(object), Some(transform));
        assert_eq!(instance.effective_transform(ObjectId::new(999)), None);
    }

    #[test]
    fn transaction_preflight_rejects_late_failure_before_runtime_publication() {
        let (mut instance, object) = semantic_instance();
        let before = instance.frame().clone();
        let publication_before = instance.publication_context();
        let missing = ObjectId::new(999);
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(4.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::SetStyle {
                object: missing,
                style: Style::default(),
            },
        ]);

        assert_eq!(
            instance.apply_transaction(&transaction).unwrap_err(),
            CompilePatchError::UnknownObject(missing)
        );
        assert_eq!(instance.frame(), &before);
        assert_eq!(instance.publication_context(), publication_before);
        assert_eq!(
            instance.effective_transform(object),
            Some(Transform2D::IDENTITY)
        );
    }

    #[test]
    fn text_morph_rejection_leaves_live_frame_and_publication_unchanged() {
        let mut store = SemanticStore::new();
        let text = store
            .import_text_resource(
                TextResource {
                    source: Arc::from("text"),
                    kind: TextSourceKind::Plain,
                    runs: Arc::from([]),
                    vector_items: Arc::from([]),
                    render_items: Arc::from([]),
                    parts: Arc::from([]),
                    bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
                    baseline: 0.0,
                    layout_artifact: None,
                },
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();
        let semantic_object = store.insert_semantic_object(SemanticObjectState::new(text));
        store.attach_to_scene(semantic_object).unwrap();
        let mut index = SemanticExecutionIndex::new();
        let lowered = lower_semantic_execution(&store, &mut index).unwrap();
        let object = index.execution_object_id(semantic_object).unwrap();
        let mut instance = SceneInstance::from_semantic_execution(lowered);
        let before_frame = instance.frame().clone();
        let before_publication = instance.publication_context();
        let changed_style = Style {
            opacity: 0.25,
            ..Style::default()
        };
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetStyle {
                object,
                style: changed_style,
            },
            ScenePatch::AddTrack(TrackDefinition {
                id: TrackId::new(9),
                object,
                property: Property::Morph,
                values: TrackValues::Scalar { from: 0.0, to: 1.0 },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }),
        ]);

        assert_eq!(
            instance.apply_transaction(&transaction),
            Err(CompilePatchError::GeometryTrackTargetsText {
                track: TrackId::new(9),
                property: Property::Morph,
            })
        );
        assert_eq!(instance.frame(), &before_frame);
        assert_eq!(instance.publication_context(), before_publication);
    }

    #[test]
    fn authored_scene_only_publication_advances_scene_and_frame_once() {
        let (mut instance, _) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let next_scene = before.scene_revision().checked_next().unwrap();

        // Semantic precision may change while its compact f32 projection remains
        // identical. The empty execution transaction still publishes that authored
        // revision without claiming an executable change or dirtying a render row.
        instance
            .apply_authored_transaction(&MutationTransaction::default(), before, next_scene)
            .unwrap();

        let after = instance.publication_context();
        assert_eq!(after.scene_revision(), next_scene);
        assert_eq!(after.execution_revision(), before.execution_revision());
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert!(instance.take_frame_changes().is_empty());

        instance
            .apply_authored_transaction(
                &MutationTransaction::default(),
                after,
                after.scene_revision(),
            )
            .unwrap();
        assert_eq!(instance.publication_context(), after);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn authored_execution_publication_advances_each_changed_domain_once() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(2.0, -1.0),
                ..Transform2D::IDENTITY
            },
        }]);

        instance
            .apply_authored_transaction(
                &transaction,
                before,
                before.scene_revision().checked_next().unwrap(),
            )
            .unwrap();

        let after = instance.publication_context();
        assert_eq!(
            after.scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn authored_execution_only_publication_keeps_scene_revision_current() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([ScenePatch::SetStyle {
            object,
            style: Style {
                opacity: 0.25,
                ..Style::default()
            },
        }]);

        instance
            .apply_authored_transaction(&transaction, before, before.scene_revision())
            .unwrap();

        let after = instance.publication_context();
        assert_eq!(after.scene_revision(), before.scene_revision());
        assert_eq!(
            after.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    }

    #[test]
    fn authored_publication_rejects_stale_context_and_invalid_scene_revision() {
        let (mut instance, _) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let stale = PublicationContext::new(
            before.scene_revision(),
            before.execution_revision().checked_next().unwrap(),
            before.frame_epoch(),
        );

        assert_eq!(
            instance.apply_authored_transaction(
                &MutationTransaction::default(),
                stale,
                before.scene_revision(),
            ),
            Err(AuthoredPublicationError::StalePublication {
                expected: stale,
                actual: before,
            })
        );
        let skipped = SceneRevision::new(before.scene_revision().get() + 2);
        assert_eq!(
            instance.apply_authored_transaction(&MutationTransaction::default(), before, skipped,),
            Err(AuthoredPublicationError::InvalidSceneRevision {
                current: before.scene_revision(),
                proposed: skipped,
            })
        );
        assert_eq!(instance.publication_context(), before);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn authored_compile_failure_leaves_runtime_and_publication_untouched() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let frame_before = instance.frame().clone();
        let before = instance.publication_context();
        let missing = ObjectId::new(999);
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(4.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::SetStyle {
                object: missing,
                style: Style::default(),
            },
        ]);

        assert_eq!(
            instance.apply_authored_transaction(
                &transaction,
                before,
                before.scene_revision().checked_next().unwrap(),
            ),
            Err(AuthoredPublicationError::Compile(
                CompilePatchError::UnknownObject(missing)
            ))
        );
        assert_eq!(instance.frame(), &frame_before);
        assert_eq!(instance.publication_context(), before);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn authored_publication_rejects_revision_overflow_before_writes() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let base = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(1.0, 0.0),
                ..Transform2D::IDENTITY
            },
        }]);

        let execution_max = PublicationContext::new(
            base.scene_revision(),
            ExecutionRevision::new(u64::MAX),
            base.frame_epoch(),
        );
        instance.publication = execution_max;
        let frame_before = instance.frame().clone();
        assert_eq!(
            instance.apply_authored_transaction(
                &transaction,
                execution_max,
                execution_max.scene_revision(),
            ),
            Err(AuthoredPublicationError::ExecutionRevisionExhausted(
                ExecutionRevision::new(u64::MAX)
            ))
        );
        assert_eq!(instance.frame(), &frame_before);
        assert_eq!(instance.publication_context(), execution_max);
        assert!(instance.take_frame_changes().is_empty());

        let frame_max = PublicationContext::new(
            base.scene_revision(),
            base.execution_revision(),
            FrameEpoch::new(u64::MAX),
        );
        instance.publication = frame_max;
        assert_eq!(
            instance.apply_authored_transaction(
                &MutationTransaction::default(),
                frame_max,
                frame_max.scene_revision().checked_next().unwrap(),
            ),
            Err(AuthoredPublicationError::FrameEpochExhausted(
                FrameEpoch::new(u64::MAX)
            ))
        );
        assert_eq!(instance.publication_context(), frame_max);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn successful_execution_transaction_advances_execution_and_frame_once() {
        let (mut instance, object) = semantic_instance();
        let before = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(4.0, 0.0),
                ..Transform2D::IDENTITY
            },
        }]);

        instance.apply_transaction(&transaction).unwrap();
        let after = instance.publication_context();
        assert_eq!(after.scene_revision(), before.scene_revision());
        assert_eq!(
            after.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            after.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );

        instance.take_frame_changes();
        instance.apply_transaction(&transaction).unwrap();
        assert_eq!(instance.publication_context(), after);
        assert_eq!(
            instance.last_patch_stats(),
            crate::RuntimePatchStats::default()
        );
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn unchanged_value_patches_do_not_publish_or_dirty_the_frame() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let original = instance.effective_object(object).unwrap().clone();
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: original.transform,
            },
            ScenePatch::SetStyle {
                object,
                style: original.style,
            },
            ScenePatch::SetGeometry {
                object,
                geometry: original.geometry().unwrap().clone(),
            },
        ]);
        instance.apply_transaction(&transaction).unwrap();
        assert_eq!(instance.publication_context(), before);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn cancelling_value_writes_in_one_batch_publish_nothing() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(4.0, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::SetTransform {
                object,
                transform: Transform2D::IDENTITY,
            },
        ]);
        instance.apply_transaction(&transaction).unwrap();
        assert_eq!(instance.publication_context(), before);
        assert_eq!(
            instance.effective_transform(object),
            Some(Transform2D::IDENTITY)
        );
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn overwritten_invalid_value_still_fails_before_publication() {
        let (mut instance, object) = semantic_instance();
        instance.take_frame_changes();
        let before = instance.publication_context();
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(f32::NAN, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            ScenePatch::SetTransform {
                object,
                transform: Transform2D::IDENTITY,
            },
        ]);
        assert!(instance.apply_transaction(&transaction).is_err());
        assert_eq!(instance.publication_context(), before);
        assert!(instance.take_frame_changes().is_empty());
    }

    #[test]
    fn direct_patch_publishes_once_and_repeated_or_failed_patch_does_not() {
        let (mut instance, object) = semantic_instance();
        let before = instance.publication_context();
        let patch = ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(2.0, 3.0),
                ..Transform2D::IDENTITY
            },
        };
        instance.apply_patch(&patch).unwrap();
        let committed = instance.publication_context();
        assert_eq!(
            committed.execution_revision(),
            before.execution_revision().checked_next().unwrap()
        );
        assert_eq!(
            committed.frame_epoch(),
            before.frame_epoch().checked_next().unwrap()
        );
        instance.take_frame_changes();
        instance.apply_patch(&patch).unwrap();
        assert_eq!(instance.publication_context(), committed);
        assert!(instance.take_frame_changes().is_empty());
        assert!(instance
            .apply_patch(&ScenePatch::RemoveObject(ObjectId::new(999)))
            .is_err());
        assert_eq!(instance.publication_context(), committed);
    }

    #[test]
    fn effective_lookup_rejects_removed_slots_and_preserves_later_objects() {
        let (mut instance, [first, later]) = semantic_instances([1.0, 2.0]);
        let expected = instance.effective_object(later).unwrap().clone();
        instance
            .apply_patch(&ScenePatch::RemoveObject(first))
            .unwrap();
        assert!(instance.effective_object(first).is_none());
        assert_eq!(instance.effective_object(later), Some(&expected));
        assert!(instance.effective_object(ObjectId::new(999)).is_none());
    }
}

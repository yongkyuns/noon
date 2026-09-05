use noon_compile::CompilePatchError;
use noon_core::{MutationTransaction, ObjectId, Transform2D};
use std::collections::HashSet;

use crate::{FrameObjectState, FrameState, SceneInstance};

/// Retain the final value write in each region bounded by structural/timeline
/// edits. The caller must preflight all writes, including superseded ones.
pub(super) fn final_value_writes(transaction: &MutationTransaction) -> Vec<&noon_core::ScenePatch> {
    let mut final_writes = HashSet::new();
    let mut retained = Vec::with_capacity(transaction.mutations().len());
    for patch in transaction.mutations().iter().rev() {
        match patch {
            noon_core::ScenePatch::SetGeometry { object, .. }
            | noon_core::ScenePatch::SetTransform { object, .. }
            | noon_core::ScenePatch::SetStyle { object, .. } => {
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
        self.compiled.preflight_transaction(transaction)?;
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
        if changed {
            self.publish_execution_change();
        }
        Ok(&self.frame)
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
    use noon_core::{
        ScenePatch, SemanticObjectState, SemanticStore, StoredGeometry, Style, Transform2D, Vec2,
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
                geometry: original.geometry,
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

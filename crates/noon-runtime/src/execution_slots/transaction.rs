use noon_compile::CompilePatchError;
use noon_core::{MutationTransaction, ObjectId, Transform2D};

use crate::SceneInstance;

impl SceneInstance {
    /// Return the already-evaluated effective transform for one live execution object.
    ///
    /// Stable compiled object identity resolves directly to the matching frame slot;
    /// this does not resample the scene or scan renderer-facing frame objects.
    pub fn effective_transform(&self, object: ObjectId) -> Option<Transform2D> {
        let object_index = self.compiled.object_index(object)? as usize;
        self.frame
            .objects
            .get(object_index)
            .map(|state| state.transform)
    }

    /// Publish one preflighted mutation transaction as a single runtime operation.
    ///
    /// Validation completes against the full transaction before the first live patch is
    /// applied. Individual patches then reuse the existing localized runtime update path;
    /// after successful preflight, a patch rejection would indicate an internal compiler/
    /// runtime invariant violation rather than a recoverable transaction error.
    pub fn apply_transaction(
        &mut self,
        transaction: &MutationTransaction,
    ) -> Result<&crate::FrameState, CompilePatchError> {
        self.compiled.preflight_transaction(transaction)?;
        for patch in transaction.mutations() {
            self.apply_patch(patch)
                .expect("preflighted runtime transaction patch must remain valid");
        }
        Ok(&self.frame)
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{GeometryRef, MutationTransaction, ObjectId, SceneDefinition, ScenePatch, Vec2};

    use super::*;

    #[test]
    fn effective_transform_resolves_through_compiled_object_slot() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let transform = Transform2D {
            translation: Vec2::new(3.0, -2.0),
            rotation: 0.25,
            scale: Vec2::new(2.0, 0.5),
        };
        scene.object_mut(object).unwrap().transform = transform;
        let instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());

        assert_eq!(instance.effective_transform(object), Some(transform));
        assert_eq!(
            instance.effective_transform(ObjectId::new(u64::MAX)),
            None
        );
    }

    #[test]
    fn transaction_preflight_rejects_before_any_live_patch_is_published() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
        let before = instance.frame().clone();
        let moved = Transform2D {
            translation: Vec2::new(5.0, 0.0),
            ..Transform2D::IDENTITY
        };
        let missing = ObjectId::new(u64::MAX);
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::SetTransform {
                object,
                transform: moved,
            },
            ScenePatch::SetTransform {
                object: missing,
                transform: moved,
            },
        ]);

        assert_eq!(
            instance.apply_transaction(&transaction),
            Err(CompilePatchError::UnknownObject(missing))
        );
        assert_eq!(instance.frame(), &before);
        assert_eq!(
            instance.effective_transform(object),
            Some(Transform2D::IDENTITY)
        );
    }
}

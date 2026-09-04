use noon_compile::CompilePatchError;
use noon_core::{MutationTransaction, ObjectId, Transform2D};

use crate::{FrameState, SceneInstance};

impl SceneInstance {
    /// Resolve one live execution object to its current effective transform.
    ///
    /// The stable compiled object index is used to address the corresponding frame
    /// slot directly; callers do not scan renderer-facing frame objects by identity.
    pub fn effective_transform(&self, id: ObjectId) -> Option<Transform2D> {
        let object_index = self.compiled.object_index(id)? as usize;
        self.frame
            .objects
            .get(object_index)
            .map(|object| object.transform)
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
        for patch in transaction.mutations() {
            self.apply_patch(patch)
                .expect("runtime transaction was fully preflighted");
        }
        Ok(&self.frame)
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{
        GeometryRef, SceneDefinition, ScenePatch, Style, Transform2D, Vec2,
    };

    use super::*;

    #[test]
    fn effective_transform_uses_the_stable_compiled_object_slot() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
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
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        let mut instance = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
        let before = instance.frame().clone();
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
        assert_eq!(
            instance.effective_transform(object),
            Some(Transform2D::IDENTITY)
        );
    }
}

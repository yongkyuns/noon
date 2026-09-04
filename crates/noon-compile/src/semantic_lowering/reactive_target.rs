use noon_core::{ObjectId, Property, ReactiveCompileTarget};

use crate::{CompiledChannelKey, CompiledScene};

/// Let the native reactive compiler validate directly against the compiled
/// execution target rather than reconstructing a legacy authored scene.
impl ReactiveCompileTarget for CompiledScene {
    fn contains_object(&self, object: ObjectId) -> bool {
        self.object_index(object).is_some()
    }

    fn has_timeline_driver(&self, object: ObjectId, property: Property) -> bool {
        let Some(object_index) = self.object_index(object) else {
            return false;
        };
        self.has_channel(CompiledChannelKey::new(object_index, property))
    }

    fn has_timeline_activity(&self, object: ObjectId) -> bool {
        let Some(object_index) = self.object_index(object) else {
            return false;
        };
        self.objects()[object_index as usize].dynamic.any()
    }

    fn visit_objects(&self, visitor: &mut dyn FnMut(ObjectId)) {
        for object in self.objects().iter().filter(|object| object.live) {
            visitor(object.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        ObjectExecutionClass, ReactiveError, ReactiveGraphDefinition, ReactiveProgram,
        SemanticObjectState, SemanticStore, StoredGeometry,
    };

    use super::*;
    use crate::SemanticExecutionIndex;

    fn compiled_circle() -> (CompiledScene, ObjectId) {
        let mut store = SemanticStore::new();
        let object = store.insert_semantic_object(SemanticObjectState::new(
            StoredGeometry::Circle { radius: 1.0 },
        ));
        store.attach_to_scene(object).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let projection = index.lower_scene(&store).unwrap();
        let execution_id = index.execution_object_id(object).unwrap();
        let compiled = CompiledScene::from_semantic_projection(&projection).unwrap();
        (compiled, execution_id)
    }

    #[test]
    fn compiled_scene_is_a_native_reactive_compile_target() {
        let (compiled, object) = compiled_circle();
        let mut graph = ReactiveGraphDefinition::new();
        let signal = graph.add_input(0.5_f32);
        graph.bind(signal, object, Property::Opacity);

        let program = ReactiveProgram::compile_for_target(&compiled, &graph).unwrap();
        assert_eq!(
            program.analysis().class_for(object),
            Some(ObjectExecutionClass::Reactive)
        );
    }

    #[test]
    fn compiled_target_rejects_bindings_to_nonexistent_execution_objects() {
        let (compiled, _) = compiled_circle();
        let unknown = ObjectId::new(u64::MAX);
        let mut graph = ReactiveGraphDefinition::new();
        let signal = graph.add_input(0.5_f32);
        graph.bind(signal, unknown, Property::Opacity);

        assert_eq!(
            ReactiveProgram::compile_for_target(&compiled, &graph).unwrap_err(),
            ReactiveError::UnknownObject(unknown)
        );
    }
}

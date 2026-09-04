use noon_core::{ObjectId, Property, ReactiveCompileTarget};

use crate::{CompiledChannelKey, CompiledScene};

impl ReactiveCompileTarget for CompiledScene {
    fn object_slot_count(&self) -> usize {
        self.objects().len()
    }

    fn object_at_slot(&self, slot: usize) -> Option<ObjectId> {
        let slot = u32::try_from(slot).ok()?;
        CompiledScene::object_id_at_slot(self, slot)
    }

    fn contains_object(&self, object: ObjectId) -> bool {
        self.object_index(object).is_some()
    }

    fn has_timeline_driver(&self, object: ObjectId, property: Property) -> bool {
        self.object_index(object)
            .is_some_and(|object_index| self.has_channel(CompiledChannelKey::new(object_index, property)))
    }

    fn has_any_timeline_driver(&self, object: ObjectId) -> bool {
        self.object_index(object).is_some_and(|object_index| {
            self.channels()
                .any(|channel| channel.object_index == object_index)
        })
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, RateFunction, ReactiveError, ReactiveGraphDefinition, ReactiveProgram,
        SceneDefinition, TrackTiming,
    };

    use super::*;

    #[test]
    fn compiled_scene_drives_existing_reactive_program_validation() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_scalar(
                object,
                Property::Opacity,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();

        let mut graph = ReactiveGraphDefinition::new();
        let signal = graph.add_input(0.5_f32);
        graph.bind(signal, object, Property::Opacity);

        assert!(matches!(
            ReactiveProgram::compile(&compiled, &graph),
            Err(ReactiveError::ConflictingDriver {
                object: conflict_object,
                property: Property::Opacity,
            }) if conflict_object == object
        ));
    }

    #[test]
    fn compiled_scene_preserves_execution_analysis_classes() {
        let mut scene = SceneDefinition::new();
        let static_object = scene.add(GeometryRef::circle(1.0));
        let timeline_object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_scalar(
                timeline_object,
                Property::Rotation,
                0.0,
                1.0,
                TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        let compiled = CompiledScene::compile(&scene).unwrap();

        let mut graph = ReactiveGraphDefinition::new();
        let signal = graph.add_input(0.5_f32);
        graph.bind(signal, static_object, Property::Opacity);

        let program = ReactiveProgram::compile(&compiled, &graph).unwrap();
        assert_eq!(program.analysis().static_objects, 0);
        assert_eq!(program.analysis().timeline_only_objects, 1);
        assert_eq!(program.analysis().reactive_only_objects, 1);
    }
}

//! Compiler from Noon's authoring-oriented scene definition to dense runtime data.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use noon_core::{
    GeometryRef, ObjectId, Property, SceneDefinition, Style, TrackDefinition, TrackId, TrackTiming,
    TrackValues, Transform2D,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynamicProperties {
    pub position: bool,
    pub rotation: bool,
    pub opacity: bool,
}

impl DynamicProperties {
    fn mark(&mut self, property: Property) {
        match property {
            Property::Position => self.position = true,
            Property::Rotation => self.rotation = true,
            Property::Opacity => self.opacity = true,
        }
    }

    pub const fn any(self) -> bool {
        self.position || self.rotation || self.opacity
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledObject {
    pub id: ObjectId,
    pub geometry: GeometryRef,
    pub base_transform: Transform2D,
    pub base_style: Style,
    pub dynamic: DynamicProperties,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledTrack {
    pub id: TrackId,
    pub object_index: u32,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledScene {
    objects: Vec<CompiledObject>,
    tracks: Vec<CompiledTrack>,
    object_indices: BTreeMap<ObjectId, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    TooManyObjects(usize),
    UnknownObject(ObjectId),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many objects: {count}")
            }
            Self::UnknownObject(id) => {
                write!(formatter, "track references unknown object {}", id.get())
            }
        }
    }
}

impl std::error::Error for CompileError {}

impl CompiledScene {
    pub fn compile(scene: &SceneDefinition) -> Result<Self, CompileError> {
        let mut object_indices = BTreeMap::new();
        let mut objects = Vec::with_capacity(scene.objects().len());

        for (index, object) in scene.objects().iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| CompileError::TooManyObjects(scene.objects().len()))?;
            object_indices.insert(object.id, index);
            objects.push(CompiledObject {
                id: object.id,
                geometry: object.geometry,
                base_transform: object.transform,
                base_style: object.style,
                dynamic: DynamicProperties::default(),
            });
        }

        let mut tracks = Vec::with_capacity(scene.tracks().len());
        for track in scene.tracks() {
            let object_index = *object_indices
                .get(&track.object)
                .ok_or(CompileError::UnknownObject(track.object))?;
            objects[object_index as usize].dynamic.mark(track.property);
            tracks.push(compile_track(track, object_index));
        }

        tracks.sort_by(|left, right| {
            left.object_index
                .cmp(&right.object_index)
                .then_with(|| property_rank(left.property).cmp(&property_rank(right.property)))
                .then_with(|| left.timing.start_time.total_cmp(&right.timing.start_time))
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(Self {
            objects,
            tracks,
            object_indices,
        })
    }

    pub fn objects(&self) -> &[CompiledObject] {
        &self.objects
    }

    pub fn tracks(&self) -> &[CompiledTrack] {
        &self.tracks
    }

    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }
}

fn compile_track(track: &TrackDefinition, object_index: u32) -> CompiledTrack {
    CompiledTrack {
        id: track.id,
        object_index,
        property: track.property,
        values: track.values,
        timing: track.timing,
    }
}

const fn property_rank(property: Property) -> u8 {
    match property {
        Property::Position => 0,
        Property::Rotation => 1,
        Property::Opacity => 2,
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{Easing, GeometryRef, Property, TrackTiming, Vec2};

    use super::*;

    #[test]
    fn object_ids_resolve_to_dense_indices() {
        let mut scene = SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(1.0));
        let rectangle = scene.add(GeometryRef::rectangle(2.0, 3.0));

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");

        assert_eq!(compiled.object_index(circle), Some(0));
        assert_eq!(compiled.object_index(rectangle), Some(1));
        assert_eq!(compiled.objects()[0].id, circle);
        assert_eq!(compiled.objects()[1].id, rectangle);
    }

    #[test]
    fn tracks_are_sorted_for_runtime_access() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::circle(1.0));
        scene
            .animate_position(
                object,
                Vec2::new(5.0, 0.0),
                Vec2::new(6.0, 0.0),
                TrackTiming::new(5.0, 1.0, Easing::Linear),
            )
            .expect("valid track");
        scene
            .animate_position(
                object,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(1.0, 1.0, Easing::Linear),
            )
            .expect("valid track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let starts: Vec<f64> = compiled
            .tracks()
            .iter()
            .map(|track| track.timing.start_time)
            .collect();

        assert_eq!(starts, vec![1.0, 5.0]);
    }

    #[test]
    fn only_animated_properties_are_marked_dynamic() {
        let mut scene = SceneDefinition::new();
        let animated = scene.add(GeometryRef::circle(1.0));
        let static_object = scene.add(GeometryRef::rectangle(2.0, 2.0));
        scene
            .animate_scalar(
                animated,
                Property::Opacity,
                1.0,
                0.0,
                TrackTiming::new(0.0, 1.0, Easing::Linear),
            )
            .expect("valid track");

        let compiled = CompiledScene::compile(&scene).expect("scene must compile");
        let animated_index = compiled.object_index(animated).expect("known object") as usize;
        let static_index = compiled.object_index(static_object).expect("known object") as usize;

        assert_eq!(
            compiled.objects()[animated_index].dynamic,
            DynamicProperties {
                position: false,
                rotation: false,
                opacity: true,
            }
        );
        assert!(!compiled.objects()[static_index].dynamic.any());
    }

    #[test]
    fn identical_input_compiles_identically() {
        fn build() -> SceneDefinition {
            let mut scene = SceneDefinition::new();
            let object = scene.add(GeometryRef::circle(2.0));
            scene
                .animate_position(
                    object,
                    Vec2::ZERO,
                    Vec2::new(3.0, 4.0),
                    TrackTiming::new(0.5, 2.0, Easing::EaseInOutCubic),
                )
                .expect("valid track");
            scene
        }

        assert_eq!(
            CompiledScene::compile(&build()).expect("scene must compile"),
            CompiledScene::compile(&build()).expect("scene must compile")
        );
    }
}

use std::collections::BTreeMap;

use noon_core::{
    ObjectContentRef, ObjectId, Property, RetainedObjectDefinition, SceneDefinition,
    TextResourceHandle, TrackDefinition,
};

use super::{
    compile_error, compile_track, sort_tracks, validate_presence_chains, CompileError,
    CompiledChannelKey, CompiledTrack, CompiledTracks, DynamicProperties,
};

/// Dense compiler output for a retained object payload.
///
/// Unlike the legacy `CompiledObject`, this representation is not constrained to
/// geometry. Geometry and retained text share one stable object/dense-slot domain,
/// so painter order and ordinary property tracks do not need a second text-specific
/// identity system.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedCompiledObject {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    pub base_transform: noon_core::Transform2D,
    pub base_style: noon_core::Style,
    pub dynamic: DynamicProperties,
}

impl RetainedCompiledObject {
    pub fn geometry(&self) -> Option<&noon_core::GeometryRef> {
        self.content.geometry()
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        self.content.text()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetainedCompiledScene {
    objects: Vec<RetainedCompiledObject>,
    tracks: BTreeMap<CompiledChannelKey, Vec<CompiledTrack>>,
    track_count: usize,
    object_indices: BTreeMap<ObjectId, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedCompileError {
    TooManyObjects(usize),
    DuplicateObject(ObjectId),
    UnknownObject(ObjectId),
    TextTransformTrack(noon_core::TrackId),
    Track(CompileError),
}

impl std::fmt::Display for RetainedCompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "scene contains too many retained objects: {count}")
            }
            Self::DuplicateObject(id) => {
                write!(formatter, "duplicate retained object id {}", id.get())
            }
            Self::UnknownObject(id) => {
                write!(formatter, "track references unknown retained object {}", id.get())
            }
            Self::TextTransformTrack(id) => write!(
                formatter,
                "transform track {} still carries geometry snapshots and cannot target retained text",
                id.get()
            ),
            Self::Track(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedCompileError {}

impl RetainedCompiledScene {
    /// Compile a retained-object scene without routing text through legacy
    /// geometry-only `SceneDefinition` serialization.
    pub fn compile(
        objects: &[RetainedObjectDefinition],
        tracks: &[TrackDefinition],
    ) -> Result<Self, RetainedCompileError> {
        let mut object_indices = BTreeMap::new();
        let mut compiled_objects = Vec::with_capacity(objects.len());

        for (index, object) in objects.iter().enumerate() {
            let index = u32::try_from(index)
                .map_err(|_| RetainedCompileError::TooManyObjects(objects.len()))?;
            if object_indices.insert(object.id, index).is_some() {
                return Err(RetainedCompileError::DuplicateObject(object.id));
            }
            compiled_objects.push(RetainedCompiledObject {
                id: object.id,
                content: object.content.clone(),
                base_transform: object.transform,
                base_style: object.style,
                dynamic: DynamicProperties::default(),
            });
        }

        let mut compiled_tracks = Vec::with_capacity(tracks.len());
        for track in tracks {
            let object_index = *object_indices
                .get(&track.object)
                .ok_or(RetainedCompileError::UnknownObject(track.object))?;
            let object = &mut compiled_objects[object_index as usize];
            if object.text().is_some() && track.property == Property::Transform {
                return Err(RetainedCompileError::TextTransformTrack(track.id));
            }
            object.dynamic.mark(track.property);
            compiled_tracks.push(
                compile_track(track, object_index)
                    .map_err(|error| RetainedCompileError::Track(compile_error(track.id, error)))?,
            );
        }

        sort_tracks(&mut compiled_tracks);
        validate_presence_chains(&compiled_tracks).map_err(|(previous, next)| {
            RetainedCompileError::Track(CompileError::DiscontinuousPresence { previous, next })
        })?;

        let track_count = compiled_tracks.len();
        let mut tracks_by_channel = BTreeMap::<CompiledChannelKey, Vec<CompiledTrack>>::new();
        for track in compiled_tracks {
            tracks_by_channel
                .entry(CompiledChannelKey::new(track.object_index, track.property))
                .or_default()
                .push(track);
        }

        Ok(Self {
            objects: compiled_objects,
            tracks: tracks_by_channel,
            track_count,
            object_indices,
        })
    }

    /// Compatibility bridge proving legacy geometry scenes lower to the same
    /// retained object shape without changing their public serialization.
    pub fn compile_legacy(scene: &SceneDefinition) -> Result<Self, RetainedCompileError> {
        let objects = scene
            .objects()
            .iter()
            .map(RetainedObjectDefinition::from)
            .collect::<Vec<_>>();
        Self::compile(&objects, scene.tracks())
    }

    pub fn objects(&self) -> &[RetainedCompiledObject] {
        &self.objects
    }

    pub fn object_index(&self, id: ObjectId) -> Option<u32> {
        self.object_indices.get(&id).copied()
    }

    pub fn tracks(&self) -> CompiledTracks<'_> {
        CompiledTracks {
            channels: &self.tracks,
        }
    }

    pub fn tracks_iter(&self) -> impl Iterator<Item = &CompiledTrack> {
        self.tracks.values().flat_map(|tracks| tracks.iter())
    }

    pub fn channels(&self) -> impl Iterator<Item = CompiledChannelKey> + '_ {
        self.tracks.keys().copied()
    }

    pub fn channel_tracks(&self, channel: CompiledChannelKey) -> &[CompiledTrack] {
        self.tracks.get(&channel).map_or(&[], Vec::as_slice)
    }

    pub const fn track_count(&self) -> usize {
        self.track_count
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, RateFunction, TextResourceId, TrackId, TrackTiming, TrackValues, Vec2,
    };

    use super::*;

    fn text_handle(id: u64, version: u64) -> TextResourceHandle {
        TextResourceHandle {
            id: TextResourceId::new(id),
            version,
        }
    }

    #[test]
    fn mixed_geometry_and_text_share_one_dense_object_domain() {
        let circle = RetainedObjectDefinition::geometry(
            ObjectId::new(10),
            GeometryRef::circle(1.0),
        );
        let text = RetainedObjectDefinition::text(ObjectId::new(20), text_handle(7, 3));
        let line = RetainedObjectDefinition::geometry(
            ObjectId::new(30),
            GeometryRef::line(Vec2::ZERO, Vec2::ONE),
        );

        let compiled = RetainedCompiledScene::compile(&[circle, text, line], &[]).unwrap();
        assert_eq!(compiled.object_index(ObjectId::new(10)), Some(0));
        assert_eq!(compiled.object_index(ObjectId::new(20)), Some(1));
        assert_eq!(compiled.object_index(ObjectId::new(30)), Some(2));
        assert_eq!(compiled.objects()[1].text(), Some(text_handle(7, 3)));
        assert!(compiled.objects()[1].geometry().is_none());
    }

    #[test]
    fn ordinary_property_tracks_target_retained_text_without_copying_resources() {
        let object = ObjectId::new(4);
        let handle = text_handle(99, 12);
        let text = RetainedObjectDefinition::text(object, handle);
        let tracks = [
            TrackDefinition {
                id: TrackId::new(0),
                object,
                property: Property::Position,
                values: TrackValues::Vec2 {
                    from: Vec2::ZERO,
                    to: Vec2::new(3.0, 2.0),
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: noon_core::CompositionTimeMap::identity(),
            },
            TrackDefinition {
                id: TrackId::new(1),
                object,
                property: Property::Opacity,
                values: TrackValues::Scalar { from: 1.0, to: 0.5 },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: noon_core::CompositionTimeMap::identity(),
            },
        ];

        let compiled = RetainedCompiledScene::compile(&[text], &tracks).unwrap();
        assert_eq!(compiled.objects()[0].text(), Some(handle));
        assert!(compiled.objects()[0].dynamic.position);
        assert!(compiled.objects()[0].dynamic.opacity);
        assert_eq!(compiled.track_count(), 2);
    }

    #[test]
    fn geometry_snapshot_transform_tracks_are_rejected_for_text_until_generalized() {
        let object = ObjectId::new(5);
        let text = RetainedObjectDefinition::text(object, text_handle(1, 0));
        let track = TrackDefinition {
            id: TrackId::new(9),
            object,
            property: Property::Transform,
            values: TrackValues::Object {
                from: noon_core::ObjectSnapshot::new(GeometryRef::circle(1.0)),
                to: noon_core::ObjectSnapshot::new(GeometryRef::circle(2.0)),
            },
            timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        };

        assert_eq!(
            RetainedCompiledScene::compile(&[text], &[track]),
            Err(RetainedCompileError::TextTransformTrack(TrackId::new(9)))
        );
    }

    #[test]
    fn legacy_geometry_scene_has_identical_object_and_track_order() {
        let mut legacy = SceneDefinition::new();
        let first = legacy.add(GeometryRef::circle(1.0));
        let second = legacy.add(GeometryRef::rectangle(2.0, 3.0));
        legacy
            .animate_position(
                second,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(2.0, 1.0, RateFunction::Linear),
            )
            .unwrap();
        legacy
            .animate_position(
                first,
                Vec2::ZERO,
                Vec2::ONE,
                TrackTiming::new(1.0, 1.0, RateFunction::Linear),
            )
            .unwrap();

        let retained = RetainedCompiledScene::compile_legacy(&legacy).unwrap();
        let old = super::super::CompiledScene::compile(&legacy).unwrap();
        assert_eq!(retained.objects().len(), old.objects().len());
        assert_eq!(retained.object_index(first), old.object_index(first));
        assert_eq!(retained.object_index(second), old.object_index(second));
        assert_eq!(
            retained
                .tracks()
                .iter()
                .map(|track| (track.object_index, track.timing.start_time))
                .collect::<Vec<_>>(),
            old.tracks()
                .iter()
                .map(|track| (track.object_index, track.timing.start_time))
                .collect::<Vec<_>>()
        );
    }
}

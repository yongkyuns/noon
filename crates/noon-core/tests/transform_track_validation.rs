use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectDefinition, ObjectId, ObjectSnapshot, ObjectStateField,
    PatchError, Property, RateFunction, SceneDefinition, Style, TimelineError, TrackDefinition,
    TrackId, TrackTiming, TrackValueEndpoint, TrackValues,
};

#[test]
fn bulk_construction_rejects_non_finite_transform_track_snapshot() {
    let object = ObjectDefinition::new(ObjectId::new(7), GeometryRef::circle(1.0));
    let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
    let mut to = from.clone();
    to.style = Style {
        opacity: f32::NAN,
        ..Style::default()
    };
    let track = TrackDefinition {
        id: TrackId::new(3),
        object: object.id,
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };

    assert!(matches!(
        SceneDefinition::from_parts(vec![object], vec![track]),
        Err(PatchError::InvalidTrack(TimelineError::InvalidObjectValue {
            property: Property::Transform,
            endpoint: TrackValueEndpoint::To,
            field: ObjectStateField::Style,
        }))
    ));
}

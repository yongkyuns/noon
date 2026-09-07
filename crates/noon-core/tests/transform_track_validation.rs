use noon_core::{
    validate_track_definition, CompositionTimeMap, GeometryRef, ObjectId, ObjectStateField,
    Property, RateFunction, Style, TimelineError, TrackDefinition, TrackId, TrackTiming,
    TrackValueEndpoint, TrackValues, TransformTrackEndpoint,
};

#[test]
fn transform_track_rejects_non_finite_endpoint_before_compilation() {
    let from = TransformTrackEndpoint::new(GeometryRef::circle(1.0));
    let mut to = from.clone();
    to.style = Style {
        opacity: f32::NAN,
        ..Style::default()
    };
    let track = TrackDefinition {
        id: TrackId::new(3),
        object: ObjectId::new(7),
        property: Property::Transform,
        values: TrackValues::Object { from, to },
        timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    };
    assert!(matches!(
        validate_track_definition(&track),
        Err(TimelineError::InvalidObjectValue {
            property: Property::Transform,
            endpoint: TrackValueEndpoint::To,
            field: ObjectStateField::Style,
        })
    ));
}

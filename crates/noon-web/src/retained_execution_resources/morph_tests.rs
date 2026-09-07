#![cfg(test)]

use crate::{
    CanonicalAuthoringScene, CanonicalRetainedEnginePlayer, InstalledRetainedExecutionMirror,
    RetainedExecutionDeltaEnvelope,
};
use noon_core::{
    CompositionTimeMap, ObjectContentRef, ObjectId, Property, TrackDefinition, TrackId,
    TrackValues, Vec2,
};
#[test]
fn morph_endpoint_publishes_geometry_after_clearing_render_override() {
    use noon_core::{Easing, GeometryRef, StrokeWidthMode, TrackTiming, TransformTrackEndpoint};
    let source = GeometryRef::circle(1.0);
    let target = GeometryRef::rectangle(2.0, 1.0);
    let id = ObjectId::new(1);
    let mut from = TransformTrackEndpoint::new(source);
    from.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    let mut to = TransformTrackEndpoint::new(target.clone());
    to.style = from.style;
    to.transform.rotation = 0.7;
    let store = std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
    let scene = noon::Scene::with_store(std::rc::Rc::clone(&store));
    let mut geometry = scene.circle(1.0).unwrap();
    geometry.set_stroke_width_mode("screen_space").unwrap();
    let text = scene
        .text(noon::Text::new("Endpoint").with_font_size(64.0))
        .unwrap();
    let tracks = vec![
        TrackDefinition {
            id: TrackId::new(0),
            object: id,
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        },
        TrackDefinition {
            id: TrackId::new(1),
            object: id,
            property: Property::Position,
            values: TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(2.0, 0.0),
            },
            timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
            time_map: CompositionTimeMap::identity(),
        },
    ];
    let mut context = CanonicalAuthoringScene::with_store(store);
    context.bind_mobject(id, &geometry).unwrap();
    context.bind_mobject(ObjectId::new(8), &text).unwrap();
    let exported = context.finalize(tracks, Vec::new(), None).unwrap();
    let mut engine = CanonicalRetainedEnginePlayer::new(exported, 2.0, 41).unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
            .unwrap();
    let mut local_text = None;
    for (sample_index, time) in [0.0, 0.5, 1.0, 1.2].into_iter().enumerate() {
        let encoded = if sample_index == 0 {
            engine.initial_delta_json().unwrap()
        } else {
            engine
                .seek_delta_json(time)
                .unwrap()
                .expect("seek changes morph state")
        };
        mirror
            .apply(serde_json::from_str::<RetainedExecutionDeltaEnvelope>(&encoded).unwrap())
            .unwrap();
        let actual = mirror.frame().unwrap();
        let text = actual.objects[1].content.text().unwrap();
        assert_eq!(*local_text.get_or_insert(text), text);
        if time >= 1.0 {
            assert_eq!(
                actual.objects[0].content,
                ObjectContentRef::Geometry(target.clone())
            );
            assert!(actual.render_geometries[0].is_none());
            assert!(actual.render_transforms[0].is_none());
        }
    }
}

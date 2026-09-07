#![cfg(test)]

use crate::{
    retained_scene_spec_runtime::CanonicalRetainedAuthoringScene, CanonicalAuthoringScene,
    InstalledRetainedExecutionMirror, RetainedAuthoringPlayer,
};
use noon_core::{
    CompositionTimeMap, ObjectContentRef, ObjectId, Property, TrackDefinition, TrackId,
    TrackValues, Vec2,
};
#[test]
fn morph_endpoint_publishes_geometry_after_clearing_render_override() {
    use noon_core::{Easing, GeometryRef, ObjectSnapshot, StrokeWidthMode, TrackTiming};
    let source = GeometryRef::circle(1.0);
    let target = GeometryRef::rectangle(2.0, 1.0);
    let id = ObjectId::new(1);
    let mut from = ObjectSnapshot::new(source);
    from.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    let mut to = ObjectSnapshot::new(target.clone());
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
    let mut engine = RetainedAuthoringPlayer::new(
        CanonicalRetainedAuthoringScene::from_scene_spec(exported).unwrap(),
        41,
    )
    .unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(engine.resource_bundle_bytes())
            .unwrap();
    let mut local_text = None;
    for time in [0.0, 0.5, 1.0, 1.2] {
        mirror
            .apply(engine.evaluate_delta(time).unwrap().unwrap())
            .unwrap();
        let expected = engine.frame();
        let actual = mirror.frame().unwrap();
        assert_eq!(actual.objects[0], expected.objects[0], "time {time}");
        assert_eq!(actual.render_geometry(0), expected.render_geometry(0));
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

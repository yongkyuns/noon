#![cfg(test)]

use crate::{
    InstalledRetainedExecutionMirror, RetainedAuthoringDocument, RetainedAuthoringTextObject,
    RetainedTextAuthoringSpec,
};
use noon_core::{ObjectContentRef, ObjectId, SceneDefinition, Vec2};

#[test]
fn morph_endpoint_publishes_geometry_after_clearing_render_override() {
    use noon_core::{Easing, GeometryRef, ObjectSnapshot, StrokeWidthMode, TrackTiming};
    let mut scene = SceneDefinition::new();
    let source = GeometryRef::circle(1.0);
    let target = GeometryRef::rectangle(2.0, 1.0);
    let id = scene.add(source.clone());
    let mut from = ObjectSnapshot::new(source);
    from.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    let mut to = ObjectSnapshot::new(target.clone());
    to.style = from.style;
    to.transform.rotation = 0.7;
    scene.object_mut(id).unwrap().style = from.style;
    scene
        .animate_transform(id, from, to, TrackTiming::new(0.0, 1.0, Easing::Linear))
        .unwrap();
    scene
        .animate_position(
            id,
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            TrackTiming::new(1.0, 1.0, Easing::Linear),
        )
        .unwrap();
    let document = RetainedAuthoringDocument::new(vec![RetainedAuthoringTextObject {
        object: ObjectId::new(8),
        order: 1,
        text: RetainedTextAuthoringSpec::native(
            "Endpoint",
            noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY,
            64.0,
            -1.0,
        )
        .unwrap(),
    }])
    .unwrap();
    let mut engine = crate::RetainedAuthoringPlayer::from_json(
        &noon_ir::encode_scene(&scene).unwrap(),
        &document.to_json().unwrap(),
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

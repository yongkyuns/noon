#![cfg(test)]

use crate::{
    InstalledRetainedExecutionMirror, RetainedExecutionDeltaEnvelope, SemanticExecutionPlayer,
};
use noon_core::{
    AnimationOptions, CompositionTimeMap, GeometryRef, ObjectContentRef, RateFunction,
    SemanticAnimationCompositionKind, SemanticAnimationIntent, SemanticMutationTransaction,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticVec3, TrackTiming,
};

#[test]
fn morph_endpoint_publishes_geometry_after_clearing_render_override() {
    let mut scene = noon::Scene::new();
    let mut geometry = scene.circle(1.0).unwrap();
    geometry.set_stroke_width_mode("screen_space").unwrap();
    let from = geometry.copy_handle().unwrap();
    let target_geometry = GeometryRef::rectangle(2.0, 1.0);
    let mut target = scene.rectangle(2.0, 1.0).unwrap();
    target.set_stroke_width_mode("screen_space").unwrap();
    target.set_rotation(0.7).unwrap();
    let text = scene
        .text(noon::Text::new("Endpoint").with_font_size(64.0))
        .unwrap();
    scene
        .add_many(&[(&geometry).into(), (&text).into()])
        .unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let morph = transaction.create_object_property_track(
        geometry.node_id(),
        SemanticObjectTrackProperty::Transform,
        SemanticObjectTrackValues::Object {
            from: from.node_id().into(),
            to: target.node_id().into(),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let position = transaction.create_object_property_track(
        geometry.node_id(),
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::ZERO,
            to: SemanticVec3::new(2.0, 0.0, 0.0),
        },
        TrackTiming::new(1.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let committed = transaction
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![
                    committed.resolve(morph).unwrap(),
                    committed.resolve(position).unwrap(),
                ],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let session = scene.execution_session_with_animation_root(&root).unwrap();
    let mut engine = SemanticExecutionPlayer::from_session(session, 2.0, 41).unwrap();
    let mut mirror =
        InstalledRetainedExecutionMirror::from_bundle_bytes(&engine.resource_bundle_bytes())
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
                ObjectContentRef::Geometry(target_geometry.clone())
            );
            assert!(actual.render_geometries[0].is_none());
            assert!(actual.render_transforms[0].is_none());
        }
    }
}

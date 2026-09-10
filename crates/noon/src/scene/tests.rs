use super::*;
use noon_core::{AnimationOptions, RateFunction};

#[test]
fn ordinary_transform_preflight_is_read_only_and_shares_affine_payload_validation() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut affine_target = circle.target_editor().unwrap();
    affine_target.set_translation(2.0, 0.0).unwrap();
    let options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear);
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &affine_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    for rate_func in [None, Some(RateFunction::Smooth)] {
        let mut smooth = options;
        smooth.rate_func = rate_func;
        assert!(scene
            .can_ordinary_transform_to(&circle, &affine_target, smooth)
            .unwrap());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    let mut style_target = circle.target_editor().unwrap();
    style_target.set_fill_opacity(0.5).unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );

    style_target.set_stroke_cap("round").unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert!(!scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );

    let foreign = Scene::new().circle(1.0).unwrap();
    assert!(scene
        .can_ordinary_transform_to(&circle, &foreign, options)
        .is_err());
    scene
        .integration_store()
        .borrow_mut()
        .remove_node(style_target.node_id())
        .unwrap();
    assert!(scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .is_err());
}

#[test]
fn membership_preserves_identity_isolates_roots_and_rejects_foreign_stores() {
    let store = Rc::new(RefCell::new(SemanticStore::new()));
    let mut first = Scene::with_integration_store(Rc::clone(&store));
    let mut second = Scene::with_integration_store(Rc::clone(&store));
    let object = first.circle(1.0).unwrap();
    let id = object.node_id();
    first.add(&object).unwrap();
    first.add(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    second.add(&object).unwrap();
    first.remove(&object).unwrap();
    assert!(first
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_none());
    assert!(second
        .execution_session()
        .unwrap()
        .execution_object_id(id)
        .is_some());
    first.add(&object).unwrap();
    assert_eq!(object.node_id(), id);
    let mut foreign = Scene::new();
    assert!(foreign.add(&object).is_err());
    assert!(foreign
        .execution_session()
        .unwrap()
        .frame()
        .objects
        .is_empty());
}

#[test]
fn batch_membership_uses_authoritative_root_order_and_one_revision() {
    let mut scene = Scene::new();
    let first = scene.circle(1.0).unwrap();
    let second = scene.square(1.0).unwrap();
    let replacement = scene.rectangle(2.0, 1.0).unwrap();
    let before = scene.integration_store().borrow().scene_revision();
    scene
        .add_many(&[
            MobjectFamilyMember::Mobject(&first),
            MobjectFamilyMember::Mobject(&second),
        ])
        .unwrap();
    assert_eq!(
        scene.integration_store().borrow().scene_revision().get(),
        before.get() + 1
    );
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        &[first.node_id(), second.node_id()]
    );

    scene
        .replace(
            MobjectFamilyMember::Mobject(&first),
            MobjectFamilyMember::Mobject(&replacement),
        )
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .members(),
        &[replacement.node_id(), second.node_id()]
    );
    scene.clear().unwrap();
    assert!(scene
        .integration_store()
        .borrow()
        .node(scene.root())
        .unwrap()
        .members()
        .is_empty());
}

#[test]
fn initial_animation_root_rejects_foreign_and_stale_declaration_handles() {
    fn root(scene: &Scene) -> crate::DeclaredAnimation {
        scene
            .declare_animation(
                noon_core::SemanticAnimationIntent::Wait,
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap()
    }
    let local = Scene::new();
    let foreign = Scene::new();
    let local_root = root(&local);
    let foreign_root = root(&foreign);
    assert_eq!(
        local_root.node_id(),
        foreign_root.node_id(),
        "store-local IDs may collide"
    );
    assert!(local
        .execution_session_with_animation_root(&foreign_root)
        .is_err());
    local
        .integration_store()
        .borrow_mut()
        .remove_node(local_root.node_id())
        .unwrap();
    assert!(local
        .execution_session_with_animation_root(&local_root)
        .is_err());
}

#[test]
fn exact_transform_track_preserves_constant_endpoints_that_differ_from_base() {
    use noon_core::{
        CompositionTimeMap, SemanticAnimationIntent, SemanticObjectTrackProperty,
        SemanticObjectTrackValues, TrackTiming,
    };
    let mut scene = Scene::new();
    let object = scene.circle(1.0).unwrap();
    scene.add(&object).unwrap();
    let mut endpoint = scene.circle(2.0).unwrap();
    endpoint.set_translation(3.0, 1.0).unwrap();
    endpoint.set_object_opacity(0.25).unwrap();
    let track = scene
        .declare_animation(
            SemanticAnimationIntent::ObjectPropertyTrack {
                target: object.node_id(),
                property: SemanticObjectTrackProperty::Transform,
                values: SemanticObjectTrackValues::Object {
                    from: endpoint.node_id(),
                    to: endpoint.node_id(),
                },
                timing: TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![track.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut session = scene.execution_session_with_animation_root(&root).unwrap();
    for time in [0.0, 0.5, 1.0, 0.25] {
        session.seek(time).unwrap();
        let actual = &session.frame().objects[0];
        assert_eq!(actual.transform.translation, noon_core::Vec2::new(3.0, 1.0));
        assert_eq!(actual.style.opacity, 0.25);
        assert_eq!(
            actual.content.geometry(),
            Some(&noon_core::GeometryRef::circle(2.0))
        );
    }
    assert_eq!(
        session.frame().objects.len(),
        1,
        "endpoint identities remain detached"
    );
}

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
    let revision = scene.store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &affine_target, options)
        .unwrap());
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    for rate_func in [None, Some(RateFunction::Smooth)] {
        let mut smooth = options;
        smooth.rate_func = rate_func;
        assert!(scene
            .can_ordinary_transform_to(&circle, &affine_target, smooth)
            .unwrap());
        assert_eq!(scene.store().borrow().scene_revision(), revision);
    }

    let mut style_target = circle.target_editor().unwrap();
    style_target.set_fill_opacity(0.5).unwrap();
    let revision = scene.store().borrow().scene_revision();
    assert!(scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(scene.store().borrow().scene_revision(), revision);

    style_target.set_stroke_width(3.0).unwrap();
    let revision = scene.store().borrow().scene_revision();
    assert!(!scene
        .can_ordinary_transform_to(&circle, &style_target, options)
        .unwrap());
    assert_eq!(scene.store().borrow().scene_revision(), revision);

    let foreign = Scene::new().circle(1.0).unwrap();
    assert!(scene
        .can_ordinary_transform_to(&circle, &foreign, options)
        .is_err());
    scene
        .store()
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
    let mut first = Scene::with_store(Rc::clone(&store));
    let mut second = Scene::with_store(Rc::clone(&store));
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

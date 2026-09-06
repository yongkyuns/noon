use super::*;
use crate::Scene;

#[test]
fn aliases_and_copies_share_the_arena_but_only_aliases_share_state() {
    let scene = Scene::new();
    let mut circle = scene.circle(2.0).unwrap();
    let alias = circle.clone();
    let mut copy = circle.copy_handle().unwrap();
    assert_eq!(circle.node_id(), alias.node_id());
    assert_ne!(circle.node_id(), copy.node_id());
    assert!(Rc::ptr_eq(circle.store(), copy.store()));
    circle.shift(3.0, 1.0).unwrap();
    assert_eq!(alias.center().unwrap(), (3.0, 1.0));
    assert_eq!(copy.center().unwrap(), (0.0, 0.0));
    copy.set_fill_opacity(0.25).unwrap();
    assert_eq!(circle.fill_opacity().unwrap(), 0.0);
    let state = scene
        .store()
        .borrow()
        .semantic_object_state_checked(circle.node_id())
        .unwrap()
        .clone();
    assert_eq!(
        state.content.geometry(),
        Some(StoredGeometry::Circle { radius: 2.0 })
    );
    assert_eq!(
        state.transform.translation,
        SemanticVec3::new(3.0, 1.0, 0.0)
    );
}

#[test]
fn no_op_edits_do_not_publish_and_invalid_compound_edits_roll_back() {
    let scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    let revision = scene.store().borrow().scene_revision();
    circle.shift(0.0, 0.0).unwrap();
    circle.set_fill_opacity(0.0).unwrap();
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    let before = circle.state().unwrap();
    let mut invalid = before.clone();
    invalid.transform.translation.x = 9.0;
    invalid.style.stroke_width = f64::NAN;
    assert!(circle.commit_state(invalid).is_err());
    assert_eq!(circle.state().unwrap(), before);
    assert_eq!(scene.store().borrow().scene_revision(), revision);
}

#[test]
fn foreign_operands_and_stale_handles_fail_without_mutation_or_query_panics() {
    let scene = Scene::new();
    let other_scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    let foreign = other_scene.circle(1.0).unwrap();
    assert_eq!(circle.node_id(), foreign.node_id());
    let before = circle.state().unwrap();
    assert!(circle.become_handle(&foreign).is_err());
    assert!(circle.next_to_handle(&foreign, 1.0, 0.0, 0.25).is_err());
    assert_eq!(circle.state().unwrap(), before);
    scene
        .store()
        .borrow_mut()
        .remove_node(circle.node_id())
        .unwrap();
    let replacement = scene.circle(3.0).unwrap();
    assert_eq!(replacement.node_id().slot(), circle.node_id().slot());
    assert_ne!(
        replacement.node_id().generation(),
        circle.node_id().generation()
    );
    assert!(circle.shift(1.0, 0.0).is_err());
    assert!(circle.manim_scale(2.0, 2.0).is_err());
    assert!(circle.center().is_err());
    assert!(circle.layout_bounds().is_err());
    assert!(circle.fill_opacity().is_err());
    assert!(circle.wire_translation().is_err());
}

#[test]
fn resource_geometry_is_store_owned_and_lowers_from_the_same_node() {
    let mut scene = Scene::new();
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, -2.0))
        .line_to(Vec2::new(3.0, 4.0));
    let mut object = scene.path(path, SemanticStyle::default()).unwrap();
    let content = object.state().unwrap().content;
    assert!(matches!(
        content.geometry(),
        Some(StoredGeometry::Resource(_))
    ));
    let copy = object.copy_handle().unwrap();
    assert_eq!(copy.state().unwrap().content, content);
    object.shift(2.0, 3.0).unwrap();
    assert_eq!(object.center().unwrap(), (3.0, 4.0));
    assert_eq!(object.width().unwrap(), 4.0);
    assert_eq!(object.height().unwrap(), 6.0);
    assert_eq!(object.state().unwrap().content, content);
    scene.add(&object).unwrap();
    let session = scene.execution_session().unwrap();
    assert!(session.execution_object_id(object.node_id()).is_some());
    assert_eq!(session.frame().objects.len(), 1);
    let foreign_scene = Scene::new();
    assert!(Mobject::new(Rc::clone(foreign_scene.store()), object.state().unwrap()).is_err());
}

#[test]
fn invalid_geometry_or_paint_does_not_allocate_or_publish() {
    let scene = Scene::new();
    let revision = scene.store().borrow().scene_revision();
    let nodes = scene.store().borrow().len();
    let resources = scene.store().borrow().geometry_resources().len();
    let invalid_path = VectorPath::new().move_to(Vec2::new(f32::NAN, 0.0));
    assert!(scene.path(invalid_path, SemanticStyle::default()).is_err());
    let morph = VectorPath::new()
        .move_to(Vec2::ZERO)
        .with_morph_target(VectorPath::new().line_to(Vec2::new(0.0, f32::INFINITY)));
    assert!(scene.path(morph, SemanticStyle::default()).is_err());
    for geometry in [
        GeometryRef::circle(f32::NAN),
        GeometryRef::rectangle(f32::INFINITY, 1.0),
        GeometryRef::line(Vec2::ZERO, Vec2::new(f32::NAN, 0.0)),
    ] {
        assert!(Mobject::from_geometry(
            Rc::clone(scene.store()),
            geometry,
            SemanticStyle::default()
        )
        .is_err());
    }
    for geometry in [
        StoredGeometry::Circle { radius: f32::NAN },
        StoredGeometry::Rectangle {
            size: Vec2::new(f32::INFINITY, 1.0),
        },
        StoredGeometry::Line {
            start: Vec2::ZERO,
            end: Vec2::new(f32::NAN, 0.0),
        },
    ] {
        assert!(
            Mobject::new(Rc::clone(scene.store()), SemanticObjectState::new(geometry)).is_err()
        );
    }
    let invalid_style = SemanticStyle {
        fill: Some(SemanticPaint::Solid(Color::rgba(f32::NAN, 1.0, 1.0, 1.0))),
        ..SemanticStyle::default()
    };
    assert!(scene
        .path(VectorPath::new().move_to(Vec2::ZERO), invalid_style.clone())
        .is_err());
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.style = invalid_style;
    assert!(Mobject::new(Rc::clone(scene.store()), state).is_err());
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert_eq!(scene.store().borrow().len(), nodes);
    assert_eq!(scene.store().borrow().geometry_resources().len(), resources);
}

#[test]
fn analytic_line_match_preserves_source_content_and_paint() {
    let scene = Scene::new();
    let mut source = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
    source.set_stroke_color(1.0, 0.0, 0.0, 1.0).unwrap();
    let target = scene.line((2.0, 3.0), (4.0, 5.0)).unwrap();
    let before = source.state().unwrap();

    source.match_line_handle(&target).unwrap();

    let after = source.state().unwrap();
    assert_eq!(after.content, before.content);
    assert_eq!(after.style, before.style);
    let StoredGeometry::Line { start, end } = after.content.geometry().unwrap() else {
        panic!("source remains an analytic Line")
    };
    let transform = Transform2D {
        translation: after.transform.translation.lower_xy_f32().unwrap(),
        rotation: after.transform.rotation_z as f32,
        scale: after.transform.scale.lower_xy_f32().unwrap(),
    };
    let matched_start = transform.transform_point(start);
    let matched_end = transform.transform_point(end);
    assert!((matched_start.x - 2.0).abs() < 1.0e-6);
    assert!((matched_start.y - 3.0).abs() < 1.0e-6);
    assert!((matched_end.x - 4.0).abs() < 1.0e-6);
    assert!((matched_end.y - 5.0).abs() < 1.0e-6);
    assert_eq!(transform.scale.x, transform.scale.y);
}

#[test]
fn analytic_line_match_rejects_invalid_operands_before_mutation() {
    let scene = Scene::new();
    let mut source = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
    let before = source.state().unwrap();
    let circle = scene.circle(1.0).unwrap();
    assert!(source.match_line_handle(&circle).is_err());
    assert_eq!(source.state().unwrap(), before);

    let degenerate = scene.line((2.0, 3.0), (2.0, 3.0)).unwrap();
    assert!(source.match_line_handle(&degenerate).is_err());
    assert_eq!(source.state().unwrap(), before);

    let mut nonuniform = scene.line((0.0, 0.0), (1.0, 0.0)).unwrap();
    nonuniform.set_scale(2.0, 1.0).unwrap();
    assert!(source.match_line_handle(&nonuniform).is_err());
    assert_eq!(source.state().unwrap(), before);
}

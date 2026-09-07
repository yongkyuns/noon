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
fn become_matches_dimensions_in_manim_order_and_reuses_target_content() {
    let scene = Scene::new();
    let mut source = scene.rectangle(4.0, 2.0).unwrap();
    source.set_translation(3.0, -2.0).unwrap();
    let target_path = VectorPath::new()
        .move_to(Vec2::new(-0.5, -1.5))
        .line_to(Vec2::new(0.5, -1.5))
        .line_to(Vec2::new(0.5, 1.5))
        .line_to(Vec2::new(-0.5, 1.5));
    let target = scene.path(target_path, SemanticStyle::default()).unwrap();
    let target_state = target.state().unwrap();
    let target_content = target_state.content;
    let source_id = source.node_id();
    let resources = scene.store().borrow().geometry_resources().len();

    source
        .become_handle(
            &target,
            ManimBecomeOptions {
                match_height: true,
                match_width: true,
                match_center: true,
                stretch: false,
            },
        )
        .unwrap();

    assert_eq!(source.node_id(), source_id);
    assert_eq!(source.state().unwrap().content, target_content);
    assert_eq!(target.state().unwrap(), target_state);
    assert_eq!(source.center().unwrap(), (3.0, -2.0));
    assert!((source.width().unwrap() - 4.0).abs() < 1e-9);
    // Height matching occurs first; subsequent uniform width matching scales it again.
    assert!((source.height().unwrap() - 12.0).abs() < 1e-9);
    assert_eq!(scene.store().borrow().geometry_resources().len(), resources);
}

#[test]
fn ellipse_layout_is_shared_by_queries_live_admission_and_become() {
    let mut scene = Scene::new();
    let mut ellipse = Mobject::manim_ellipse(Rc::clone(scene.store()), 4.0, 1.5).unwrap();
    ellipse.rotate(std::f64::consts::PI / 6.0).unwrap();
    let expected_width = 3.663_013_982_517_412_6;
    let expected_height = 2.464_228_071_008_26;
    assert!((ellipse.width().unwrap() - expected_width).abs() < 1.0e-12);
    assert!((ellipse.height().unwrap() - expected_height).abs() < 1.0e-12);
    assert!((ellipse.critical_point(1.0, 0.0).unwrap().0 - expected_width * 0.5).abs() < 1.0e-12);
    let SemanticObjectContent::Geometry(content) = ellipse.state().unwrap().content else {
        panic!("Ellipse remains analytic geometry")
    };
    assert_eq!(content.geometry(), StoredGeometry::Circle { radius: 1.0 });
    assert_eq!(
        content.layout(),
        SemanticGeometryLayout::ManimEllipseControlHull
    );

    let mut source = scene.rectangle(6.0, 2.0).unwrap();
    let resource_count = scene.store().borrow().geometry_resources().len();
    source
        .become_handle(
            &ellipse,
            ManimBecomeOptions {
                match_width: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!((source.width().unwrap() - 6.0).abs() < 1.0e-12);
    assert!((source.height().unwrap() - expected_height * 6.0 / expected_width).abs() < 1.0e-12);
    let SemanticObjectContent::Geometry(content) = source.state().unwrap().content else {
        panic!("become retains Ellipse geometry")
    };
    assert_eq!(
        content.layout(),
        SemanticGeometryLayout::ManimEllipseControlHull
    );
    assert_eq!(
        scene.store().borrow().geometry_resources().len(),
        resource_count
    );

    scene.add(&ellipse).unwrap();
    let mut session = scene.execution_session().unwrap();
    {
        let live = scene.live(&mut session);
        let layout = live.effective_layout(&ellipse).unwrap();
        assert!((layout.width - expected_width).abs() < 1.0e-6);
        assert!((layout.height - expected_height).abs() < 1.0e-6);
    }
    assert!(matches!(
        session.frame().objects[0].geometry(),
        Some(GeometryRef::Circle { radius: 1.0 })
    ));
}

#[test]
fn become_stretch_is_atomic_for_zero_dimension_targets() {
    let scene = Scene::new();
    let mut source = scene.rectangle(3.0, 2.0).unwrap();
    let target = scene
        .path(VectorPath::new(), SemanticStyle::default())
        .unwrap();
    let before = source.state().unwrap();
    let revision = scene.store().borrow().scene_revision();
    let resources = scene.store().borrow().geometry_resources().len();

    assert!(source
        .become_handle(
            &target,
            ManimBecomeOptions {
                stretch: true,
                ..ManimBecomeOptions::default()
            },
        )
        .is_err());
    assert_eq!(source.state().unwrap(), before);
    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert_eq!(scene.store().borrow().geometry_resources().len(), resources);
}

#[test]
fn foreign_operands_and_stale_handles_fail_without_mutation_or_query_panics() {
    let scene = Scene::new();
    let other_scene = Scene::new();
    let mut circle = scene.circle(1.0).unwrap();
    let foreign = other_scene.circle(1.0).unwrap();
    assert_eq!(circle.node_id(), foreign.node_id());
    let before = circle.state().unwrap();
    assert!(circle
        .become_handle(&foreign, ManimBecomeOptions::default())
        .is_err());
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
fn typed_manim_geometry_preserves_semantic_precision_and_matcher_defaults() {
    let scene = Scene::new();
    let before_revision = scene.store().borrow().scene_revision();
    let before_nodes = scene.store().borrow().len();
    let precise_x = 0.123_456_789_012_345;
    let mut path = ManimGeometryOptions::path(
        VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .line_to(Vec2::new(1.0, 0.0)),
    )
    .unwrap();
    path.set_translation(precise_x, -2.0).unwrap();
    path.set_fill(0.2, 0.4, 0.6, 0.75).unwrap();
    let object = Mobject::from_manim_geometry(Rc::clone(scene.store()), path).unwrap();

    assert_eq!(scene.store().borrow().len(), before_nodes + 1);
    assert_eq!(
        scene.store().borrow().scene_revision(),
        before_revision.checked_next().unwrap()
    );
    assert_eq!(object.state().unwrap().transform.translation.x, precise_x);
    assert!(matches!(
        object.state().unwrap().content.geometry(),
        Some(StoredGeometry::Resource(_))
    ));

    let point = Bounds2D64::point(3.0, -1.0);
    let surround = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::surrounding_rectangle(point, 0.25, 0.5, 0.1).unwrap(),
    )
    .unwrap();
    assert_eq!(surround.center().unwrap(), (3.0, -1.0));
    assert!((surround.width().unwrap() - 0.5).abs() < 1.0e-6);
    assert!((surround.height().unwrap() - 1.0).abs() < 1.0e-6);
    assert_eq!(surround.fill_opacity().unwrap(), 0.0);

    let background = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::background_rectangle(point, 0.25, 0.5, 0.0, 0.4).unwrap(),
    )
    .unwrap();
    let style = background.state().unwrap().style;
    assert_eq!(style.fill_opacity, 0.4);
    assert_eq!(style.stroke_width, 0.0);
    assert_eq!(style.stroke_opacity, 0.0);
}

#[test]
fn invalid_typed_geometry_and_matcher_bounds_are_inert() {
    let scene = Scene::new();
    let revision = scene.store().borrow().scene_revision();
    let nodes = scene.store().borrow().len();
    let resources = scene.store().borrow().geometry_resources().len();

    assert!(
        ManimGeometryOptions::path(VectorPath::new().move_to(Vec2::new(f32::NAN, 0.0))).is_err()
    );
    assert!(ManimGeometryOptions::rectangle(0.0, 1.0).is_err());
    assert!(ManimGeometryOptions::surrounding_rectangle(
        Bounds2D64 {
            min_x: 2.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        },
        0.1,
        0.1,
        0.0,
    )
    .is_err());
    assert!(ManimGeometryOptions::background_rectangle(
        Bounds2D64::point(0.0, 0.0),
        0.0,
        0.0,
        0.0,
        0.75,
    )
    .is_err());
    let mut invalid_style = ManimGeometryOptions::path(
        VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(1.0, 0.0)),
    )
    .unwrap();
    invalid_style.style.stroke_width = f64::NAN;
    assert!(Mobject::from_manim_geometry(Rc::clone(scene.store()), invalid_style).is_err());

    assert_eq!(scene.store().borrow().scene_revision(), revision);
    assert_eq!(scene.store().borrow().len(), nodes);
    assert_eq!(scene.store().borrow().geometry_resources().len(), resources);
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

#[test]
fn specialized_manim_geometry_uses_one_semantic_identity_per_constructor() {
    let scene = Scene::new();
    let before = scene.store().borrow().scene_revision();
    let before_nodes = scene.store().borrow().len();

    let dot = Mobject::manim_dot(Rc::clone(scene.store()), 1.25, -0.75, 0.08).unwrap();
    let triangle = Mobject::manim_triangle(Rc::clone(scene.store())).unwrap();
    let elbow =
        Mobject::manim_elbow(Rc::clone(scene.store()), 0.2, std::f64::consts::FRAC_PI_4).unwrap();
    let rounded =
        Mobject::manim_rounded_rectangle(Rc::clone(scene.store()), 4.0, 2.0, 0.5).unwrap();
    let annular = Mobject::manim_annular_sector(
        Rc::clone(scene.store()),
        1.0,
        2.0,
        std::f64::consts::FRAC_PI_2,
        0.0,
        9,
        0.0,
        0.0,
    )
    .unwrap();
    let sector = Mobject::manim_sector(
        Rc::clone(scene.store()),
        1.0,
        std::f64::consts::FRAC_PI_2,
        0.0,
        9,
        0.0,
        0.0,
    )
    .unwrap();
    let annulus = Mobject::manim_annulus(Rc::clone(scene.store()), 1.0, 2.0, 9, 0.0, 0.0).unwrap();
    let dashed =
        Mobject::manim_dashed_line(Rc::clone(scene.store()), -1.0, 0.0, 1.0, 0.0, 0.05, 0.5)
            .unwrap();
    let underline = Mobject::manim_underline(&rounded, 0.25).unwrap();

    assert_eq!(dot.center().unwrap(), (1.25, -0.75));
    assert_eq!(dot.fill_opacity().unwrap(), 1.0);
    assert_eq!(dot.state().unwrap().style.stroke_width, 0.0);
    assert_eq!(
        triangle.state().unwrap().style.stroke,
        Some(SemanticPaint::Solid(Color::BLUE))
    );
    assert!(elbow.width().unwrap() > 0.0);
    assert_eq!(underline.width().unwrap(), rounded.width().unwrap());
    assert_eq!(underline.center().unwrap().1, -1.25);
    for object in [triangle, elbow, rounded, annular, sector, annulus, dashed] {
        assert!(matches!(
            object.state().unwrap().content.geometry(),
            Some(StoredGeometry::Resource(_))
        ));
    }
    assert_eq!(scene.store().borrow().len(), before_nodes + 9);
    assert_eq!(
        scene.store().borrow().scene_revision().get(),
        before.get() + 9
    );
}

#[test]
fn specialized_options_admit_through_one_live_geometry_path_after_wait() {
    let mut scene = Scene::new();
    let anchor = scene.circle(0.1).unwrap();
    scene.add(&anchor).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let wait = live.wait_segment(0.5).unwrap();
    live.advance_segment_to(wait, wait.end_time()).unwrap();

    let options = [
        ManimGeometryOptions::dot(-4.0, 2.0, 0.3).unwrap(),
        ManimGeometryOptions::triangle().unwrap(),
        ManimGeometryOptions::elbow(0.8, 0.3).unwrap(),
        ManimGeometryOptions::rounded_rectangle(2.0, 1.0, 0.2).unwrap(),
        ManimGeometryOptions::annular_sector(0.3, 0.9, std::f64::consts::PI, 0.0, 8, 0.0, 0.0)
            .unwrap(),
        ManimGeometryOptions::sector(
            0.9,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_4,
            8,
            4.0,
            0.0,
        )
        .unwrap(),
        ManimGeometryOptions::annulus(0.5, 0.9, 8, -4.0, -2.0).unwrap(),
        ManimGeometryOptions::dashed_line(-1.0, -2.0, 1.0, -2.0, 0.2, 0.5).unwrap(),
        ManimGeometryOptions::underline(
            Bounds2D64 {
                min_x: -1.0,
                min_y: -0.5,
                max_x: 1.0,
                max_y: 0.5,
            },
            0.2,
        )
        .unwrap(),
    ];

    let mut admitted = Vec::new();
    for options in options {
        let object = live.create_manim_geometry(options).unwrap();
        assert!(!live.contains(&object).unwrap());
        live.add(&object).unwrap();
        admitted.push(object);
    }

    assert_eq!(session.frame().objects.len(), 10);
    assert_eq!(admitted[0].center().unwrap(), (-4.0, 2.0));
    assert_eq!(admitted[8].center().unwrap(), (0.0, -0.7));
    assert_eq!(admitted[8].width().unwrap(), 2.0);
}

#[test]
fn invalid_specialized_geometry_does_not_allocate_or_publish() {
    let scene = Scene::new();
    let before_revision = scene.store().borrow().scene_revision();
    let before_nodes = scene.store().borrow().len();
    let before_resources = scene.store().borrow().geometry_resources().len();

    assert!(ManimGeometryOptions::rounded_rectangle(0.0, 2.0, 0.5).is_err());
    assert!(ManimGeometryOptions::sector(1.0, 1.0, 0.0, 1, 0.0, 0.0).is_err());
    assert!(ManimGeometryOptions::dashed_line(0.0, 0.0, 1.0, 0.0, 0.0, 0.5).is_err());
    assert!(ManimGeometryOptions::underline(
        Bounds2D64 {
            min_x: 1.0,
            min_y: 0.0,
            max_x: -1.0,
            max_y: 0.0,
        },
        0.1,
    )
    .is_err());

    let store = scene.store().borrow();
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(store.len(), before_nodes);
    assert_eq!(store.geometry_resources().len(), before_resources);
}

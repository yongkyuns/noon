//! Public Rust counterparts of parity/manim-v0.21/core-examples/transform_matching_shapes.py.
use noon::{
    AnimationOptions, Color, ExecutionSegment, ExecutionSession, Mobject, MobjectFamily,
    RateFunction, Scene, SemanticStyle, Vec2, VectorPath, BLUE, GREEN, PURPLE, RED, WHITE, YELLOW,
};

struct Case {
    scene: Scene,
    session: ExecutionSession,
    segment: ExecutionSegment,
    source: MobjectFamily,
    target: MobjectFamily,
    source_leaves: Vec<Mobject>,
    target_leaves: Vec<Mobject>,
    siblings: Vec<Mobject>,
}

fn paint(mut object: Mobject, color: Color) -> Mobject {
    object
        .set_fill(color.red.into(), color.green.into(), color.blue.into(), 1.0)
        .unwrap();
    object.set_stroke_width(0.0).unwrap();
    object
}

fn polygon(scene: &Scene, points: &[(f32, f32)], x: f64, y: f64, color: Color) -> Mobject {
    let mut path = VectorPath::new().move_to(Vec2::new(points[0].0, points[0].1));
    for &(px, py) in &points[1..] {
        path = path.line_to(Vec2::new(px, py));
    }
    let mut object = paint(
        scene.path(path.close(), SemanticStyle::default()).unwrap(),
        color,
    );
    object.shift(x, y).unwrap();
    object
}

fn triangle(scene: &Scene, x: f64, color: Color) -> Mobject {
    polygon(
        scene,
        &[(-0.6, -0.5), (0.6, -0.2), (-0.15, 0.7)],
        x,
        0.0,
        color,
    )
}

fn kite(scene: &Scene, x: f64, y: f64, color: Color) -> Mobject {
    polygon(
        scene,
        &[(0.0, -0.6), (0.9, 0.0), (0.0, 0.6), (-0.3, 0.0)],
        x,
        y,
        color,
    )
}

fn case(mismatches: bool) -> Case {
    let mut scene = Scene::new();
    let left = triangle(&scene, -2.0, BLUE);
    let (source, target, source_leaves, target_leaves, siblings) = if mismatches {
        let mut square = paint(scene.square(1.0).unwrap(), YELLOW);
        square.shift(0.0, 1.0).unwrap();
        let incoming = kite(&scene, 0.0, -1.0, GREEN);
        let right = triangle(&scene, 2.0, RED);
        (
            scene.family(&[(&left).into(), (&square).into()]).unwrap(),
            scene
                .family(&[(&incoming).into(), (&right).into()])
                .unwrap(),
            vec![left, square],
            vec![incoming, right],
            vec![],
        )
    } else {
        let right = kite(&scene, 2.0, 0.0, GREEN);
        let target_left = kite(&scene, -2.0, 0.0, YELLOW);
        let target_right = triangle(&scene, 2.0, RED);
        let nested_source = scene.family(&[(&left).into()]).unwrap();
        let nested_target = scene.family(&[(&target_right).into()]).unwrap();
        let before = paint(scene.rectangle(8.0, 0.3).unwrap(), PURPLE);
        let mut after = paint(scene.rectangle(0.3, 2.0).unwrap(), WHITE);
        after.shift(-2.0, 0.0).unwrap();
        (
            scene
                .family(&[(&nested_source).into(), (&right).into()])
                .unwrap(),
            scene
                .family(&[(&target_left).into(), (&nested_target).into()])
                .unwrap(),
            vec![left, right],
            vec![target_left, target_right],
            vec![before, after],
        )
    };
    if siblings.is_empty() {
        scene.add_many(&[(&source).into()]).unwrap();
    } else {
        scene
            .add_many(&[
                (&siblings[0]).into(),
                (&source).into(),
                (&siblings[1]).into(),
            ])
            .unwrap();
    }
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .declare_and_activate_matching_family_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    Case {
        scene,
        session,
        segment,
        source,
        target,
        source_leaves,
        target_leaves,
        siblings,
    }
}

#[test]
fn public_matching_frames_and_transients_agree_with_direct_seek_and_rewind() {
    for mismatches in [false, true] {
        let mut forward = case(mismatches);
        for frame in 0..=60 {
            let time = f64::from(frame) / 30.0;
            forward
                .session
                .advance_segment_to(forward.segment, time)
                .unwrap();
            if frame % 15 != 0 {
                continue;
            }
            let mut direct = case(mismatches);
            direct.session.seek(time).unwrap();
            assert_eq!(direct.session.frame(), forward.session.frame(), "t={time}");
            assert_eq!(
                direct.session.painter_order(),
                forward.session.painter_order()
            );
            assert_eq!(
                direct
                    .session
                    .take_renderer_publication()
                    .transient_presentations(),
                forward
                    .session
                    .take_renderer_publication()
                    .transient_presentations(),
                "transients at t={time}",
            );
            let observed = forward
                .scene
                .live(&mut forward.session)
                .effective(&forward.source_leaves[0])
                .unwrap();
            assert!(
                (f64::from(observed.transform.translation.x) - (-2.0 + 2.0 * time)).abs() < 1e-5
            );
        }
        forward.session.seek(1.5).unwrap();
        forward.session.seek(0.5).unwrap();
        let mut fresh = case(mismatches);
        fresh.session.seek(0.5).unwrap();
        assert_eq!(forward.session.frame(), fresh.session.frame());
        assert_eq!(
            forward
                .session
                .take_renderer_publication()
                .transient_presentations(),
            fresh
                .session
                .take_renderer_publication()
                .transient_presentations(),
        );
    }
}

#[test]
fn public_matching_cleanup_keeps_original_target_and_stable_painter_order() {
    for mismatches in [false, true] {
        let mut c = case(mismatches);
        let original_target_states: Vec<_> = c
            .target_leaves
            .iter()
            .map(|leaf| leaf.state().unwrap())
            .collect();
        c.session.advance_segment_to(c.segment, 2.0).unwrap();
        c.scene
            .live(&mut c.session)
            .complete_segment(c.segment)
            .unwrap();
        let mut expected_root: Vec<_> = c.siblings.iter().map(Mobject::node_id).collect();
        expected_root.push(c.target.node_id());
        assert_eq!(
            c.scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(c.scene.root())
                .unwrap(),
            expected_root,
        );
        assert!(!expected_root.contains(&c.source.node_id()));
        let expected_ids: Vec<_> = c
            .siblings
            .iter()
            .chain(c.target_leaves.iter())
            .map(|leaf| c.session.execution_object_id(leaf.node_id()).unwrap())
            .collect();
        for time in [2.0, 2.1, 2.2] {
            c.session.advance_to(time).unwrap();
            let ids: Vec<_> = c
                .session
                .painter_order()
                .iter()
                .map(|&index| c.session.frame().objects[index as usize].id)
                .collect();
            assert_eq!(ids, expected_ids, "cleanup painter order at t={time}");
            assert!(c
                .session
                .take_renderer_publication()
                .transient_presentations()
                .is_empty());
            for (leaf, original) in c.target_leaves.iter().zip(&original_target_states) {
                assert_eq!(&leaf.state().unwrap(), original);
                c.scene.live(&mut c.session).effective(leaf).unwrap();
            }
        }
    }
}

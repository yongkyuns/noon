//! Endpoint coordination is ordinary Rust object animation, not automatic layout.
use noon::{AnimationOptions, GeometryRef, Scene, SemanticVec3, StrokePattern, Vec2};
use noon_core::{
    CompositionTimeMap, RateFunction, SemanticAnimationCompositionKind, SemanticAnimationIntent,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, TrackTiming,
};

#[test]
fn stroke_pattern_flow_and_a_moving_node_share_exact_attachment_coordinates() {
    let mut scene = Scene::new();
    let mut node = scene.circle(0.2).unwrap();
    node.set_translation(3.0, 0.0).unwrap();
    scene.add(&node).unwrap();
    let mut line = scene.line((-3.0, 0.0), (3.0, 0.0)).unwrap();
    line.set_stroke_pattern(Some(StrokePattern::dashed(0.25, 0.25).unwrap()))
        .unwrap();
    scene.add(&line).unwrap();
    let target = scene.line((-3.0, 0.0), (3.0, 2.0)).unwrap();
    let flow = scene.flow_stroke_to(&line, &target, 0.5, 2.0).unwrap();
    let motion = scene
        .declare_animation(
            SemanticAnimationIntent::ObjectPropertyTrack {
                target: node.node_id(),
                property: SemanticObjectTrackProperty::Position,
                values: SemanticObjectTrackValues::Vec3 {
                    from: SemanticVec3::new(3.0, 0.0, 0.0),
                    to: SemanticVec3::new(3.0, 2.0, 0.0),
                },
                timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children: vec![flow.node_id(), motion.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    scene.wait(2.0).unwrap();
    let mut execution = scene.execution_session_with_animation_root(&root).unwrap();
    for time in [0.0, 0.125, 0.5, 1.0, 1.75, 2.0, 0.5, 0.0] {
        let frame = execution.seek(time).unwrap();
        assert_eq!(frame.objects.len(), 2);
        let body = &frame.objects[0];
        let connector = &frame.objects[1];
        let GeometryRef::Line { start, end } = connector.geometry().unwrap() else {
            panic!("continuous flow must preserve the ordinary Line");
        };
        let fixed = connector.transform.transform_point(*start);
        let attached = connector.transform.transform_point(*end);
        assert_eq!(fixed, Vec2::new(-3.0, 0.0));
        assert_eq!(attached, body.transform.translation);
        assert_eq!(attached, Vec2::new(3.0, time as f32));
        assert_eq!(
            connector.style.stroke_pattern.unwrap().phase(),
            time as f32 * 0.5
        );
    }
}

#[test]
fn both_moving_endpoints_and_reverse_flow_remain_attached_at_every_seek() {
    let mut scene = Scene::new();
    let mut left = scene.circle(0.2).unwrap();
    left.set_translation(-3.0, 0.0).unwrap();
    scene.add(&left).unwrap();
    let mut right = scene.circle(0.2).unwrap();
    right.set_translation(3.0, 0.0).unwrap();
    scene.add(&right).unwrap();
    let mut connector = scene.line((-3.0, 0.0), (3.0, 0.0)).unwrap();
    connector
        .set_stroke_pattern(Some(StrokePattern::dashed(0.25, 0.25).unwrap()))
        .unwrap();
    scene.add(&connector).unwrap();

    // Both ends move independently while the dash phase runs in reverse.
    // This is one normal object track per object, not an attachment updater.
    let target = scene.line((-2.0, -1.0), (2.0, 2.0)).unwrap();
    let flow = scene.flow_stroke_to(&connector, &target, -0.75, 2.0).unwrap();
    let mut children = vec![flow.node_id()];
    for (node, from, to) in [
        (
            left.node_id(),
            SemanticVec3::new(-3.0, 0.0, 0.0),
            SemanticVec3::new(-2.0, -1.0, 0.0),
        ),
        (
            right.node_id(),
            SemanticVec3::new(3.0, 0.0, 0.0),
            SemanticVec3::new(2.0, 2.0, 0.0),
        ),
    ] {
        let motion = scene
            .declare_animation(
                SemanticAnimationIntent::ObjectPropertyTrack {
                    target: node,
                    property: SemanticObjectTrackProperty::Position,
                    values: SemanticObjectTrackValues::Vec3 { from, to },
                    timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
                    time_map: CompositionTimeMap::identity(),
                },
                AnimationOptions::new(),
            )
            .unwrap();
        children.push(motion.node_id());
    }
    let root = scene
        .declare_animation(
            SemanticAnimationIntent::Composition {
                kind: SemanticAnimationCompositionKind::Parallel,
                children,
            },
            AnimationOptions::new(),
        )
        .unwrap();
    scene.wait(2.0).unwrap();
    let mut execution = scene.execution_session_with_animation_root(&root).unwrap();
    let identities: Vec<_> = execution.frame().objects.iter().map(|row| row.id).collect();

    for time in [0.0, 0.125, 0.5, 1.0, 1.75, 2.0, 2.5, 0.5, 2.0, 0.0] {
        let frame = execution.seek(time).unwrap();
        assert_eq!(frame.objects.len(), 3, "flow creates no per-dash objects");
        assert_eq!(
            frame.objects.iter().map(|row| row.id).collect::<Vec<_>>(),
            identities,
            "seek retains semantic object identity"
        );
        let progress = (time / 2.0_f64).clamp(0.0, 1.0) as f32;
        let expected_left = Vec2::new(-3.0 + progress, -progress);
        let expected_right = Vec2::new(3.0 - progress, 2.0 * progress);
        let close = |actual: Vec2, expected: Vec2| {
            assert!((actual.x - expected.x).abs() < 1e-5, "x: {actual:?} vs {expected:?}");
            assert!((actual.y - expected.y).abs() < 1e-5, "y: {actual:?} vs {expected:?}");
        };
        close(frame.objects[0].transform.translation, expected_left);
        close(frame.objects[1].transform.translation, expected_right);
        let line = &frame.objects[2];
        let GeometryRef::Line { start, end } = line.geometry().unwrap() else {
            panic!("flow must retain analytic connection geometry");
        };
        close(line.transform.transform_point(*start), frame.objects[0].transform.translation);
        close(line.transform.transform_point(*end), frame.objects[1].transform.translation);
        let phase = line.style.stroke_pattern.unwrap().phase();
        assert!(
            (phase + 1.5 * progress).abs() < 1e-5,
            "reverse flow phase at t={time}: {phase}"
        );
    }
}

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

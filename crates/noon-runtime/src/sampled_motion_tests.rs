use super::*;
use noon_compile::CompiledObject;
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, RateFunction, SampledMotionData, SampledMotionPlan,
    SemanticVec3, TrackId, TrackTiming,
};

fn fixture() -> (SceneInstance, Arc<SampledMotionPlan>, Arc<VectorPath>) {
    let geometry = Arc::new(
        VectorPath::new()
            .move_to(Vec2::new(1.0, 0.0))
            .line_to(Vec2::new(3.0, 0.0)),
    );
    let source = Transform2D {
        translation: Vec2::new(-2.0, 3.0),
        rotation: 0.3,
        scale: Vec2::new(2.0, -1.0),
    };
    let center = source.transform_point(Vec2::new(2.0, 0.0));
    let motion = Arc::new(
        SampledMotionData::new(
            vec![0.1, 0.1000000001, 1.0],
            vec![
                SemanticVec3::new(1.0, 2.0, 0.0),
                SemanticVec3::new(3.0, -2.0, 0.0),
                SemanticVec3::new(5.0, 1.0, 0.0),
            ],
            vec![-0.5, -2.5, -12.0 * std::f64::consts::PI],
        )
        .unwrap(),
    );
    let plan = Arc::new(SampledMotionPlan::new(motion, source, center).unwrap());
    let tracks = tracks(
        &plan,
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let object = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::VectorPath(geometry.clone()),
        source,
        Style::default(),
    );
    let compiled = CompiledScene::compile_objects(vec![object], &tracks).unwrap();
    (SceneInstance::new(compiled), plan, geometry)
}

fn tracks(
    plan: &Arc<SampledMotionPlan>,
    timing: TrackTiming,
    time_map: CompositionTimeMap,
) -> [TrackDefinition; 2] {
    [
        (
            Property::Position,
            TrackValues::SampledPosition { plan: plan.clone() },
        ),
        (
            Property::Rotation,
            TrackValues::SampledRotation { plan: plan.clone() },
        ),
    ]
    .map(|(property, values)| TrackDefinition {
        id: TrackId::new(if property == Property::Position { 1 } else { 2 }),
        object: ObjectId::new(1),
        property,
        values,
        timing,
        time_map: time_map.clone(),
    })
}

#[test]
fn sampled_motion_f64_phase_forward_seek_and_static_geometry_agree() {
    let (mut forward, plan, geometry) = fixture();
    let mut seek = forward.clone();
    for time in [0.0, 0.07, 0.1, 0.10000000005, 0.1000000001, 0.25, 0.7, 1.0] {
        forward.advance_to(time).unwrap();
        seek.seek(time).unwrap();
        assert_eq!(
            forward.frame().objects[0].transform,
            seek.frame().objects[0].transform
        );
        let actual = forward.frame().objects[0].transform;
        let expected = plan.sample(time);
        assert_eq!(
            (actual.translation, actual.rotation),
            expected,
            "time {time}"
        );
        assert_eq!(actual.scale, plan.source_transform().scale);
        let Some(GeometryRef::VectorPath(current)) = forward.frame().render_geometry(0) else {
            panic!("retained path expected")
        };
        assert!(Arc::ptr_eq(current, &geometry));
    }
    forward.take_frame_changes();
    forward.advance_to(1.0).unwrap();
    assert!(forward.take_frame_changes().object_indices().is_empty());
    for time in [0.7, 0.0, 0.10000000005, 1.0] {
        seek.seek(time).unwrap();
        let actual = seek.frame().objects[0].transform;
        assert_eq!((actual.translation, actual.rotation), plan.sample(time));
    }
}

#[test]
fn sampled_motion_uses_existing_nested_map_reverse_and_returning_rates() {
    let (base, plan, _) = fixture();
    let map = CompositionTimeMap::from_steps(vec![
        CompositionTimeMapStep::new(0.1, 0.7, RateFunction::Smoothstep),
        CompositionTimeMapStep::new(0.2, 0.6, RateFunction::Linear),
    ]);
    for rate in [RateFunction::Linear, RateFunction::ThereAndBack] {
        let mut timing = TrackTiming::new(0.0, 2.3, rate);
        timing.reverse_rate_function = true;
        let definitions = tracks(&plan, timing, map.clone());
        let compiled =
            CompiledScene::compile_objects(base.compiled.objects().to_vec(), &definitions).unwrap();
        let mut forward = SceneInstance::new(compiled);
        let mut sought = forward.clone();
        for time in [0.0, 0.23, 0.46, 0.8, 1.15, 1.7, 2.3] {
            forward.advance_to(time).unwrap();
            sought.seek(time).unwrap();
            assert_eq!(
                forward.frame().objects[0].transform,
                sought.frame().objects[0].transform
            );
            if let Some(phase) = noon_core::mapped_continuous_progress_f64(timing, &map, time) {
                let pose = forward.frame().objects[0].transform;
                assert_eq!((pose.translation, pose.rotation), plan.sample(phase));
            }
        }
    }
}

#[test]
fn sampled_motion_tracks_share_one_plan_and_reject_wrong_channels_or_overshoot() {
    let (_, plan, _) = fixture();
    let mut definitions = tracks(
        &plan,
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    assert!(Arc::ptr_eq(
        definitions[0].values.sampled_motion_plan().unwrap(),
        definitions[1].values.sampled_motion_plan().unwrap()
    ));
    definitions[0].property = Property::Scale;
    assert!(noon_core::validate_track_definition(&definitions[0]).is_err());
    definitions[1].timing.easing = RateFunction::EaseOutElastic;
    assert!(noon_core::validate_track_definition(&definitions[1]).is_err());
}

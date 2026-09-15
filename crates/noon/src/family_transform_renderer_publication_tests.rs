use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSegment, ExecutionSession, LiveSession,
    MobjectFamily, RateFunction, Scene, SemanticAnimationCompositionKind,
};
use noon_runtime::TimelineWakeState;

fn smooth(run_time: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(run_time)
        .rate_func(RateFunction::Smooth)
}

fn drain(session: &mut ExecutionSession) {
    let _ = session.take_renderer_publication();
}

fn finish(live: &mut LiveSession<'_>, segment: ExecutionSegment) {
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
}

fn family_transform(
    live: &mut LiveSession<'_>,
    source: &MobjectFamily,
    target: &MobjectFamily,
    run_time: f64,
) -> ExecutionSegment {
    let request = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        children: vec![AnimationCompositionRequest::FamilyTransformTo {
            source,
            target_state: target,
            options: smooth(run_time),
        }],
        options: AnimationOptions::new().rate_func(RateFunction::Linear),
    };
    live.declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap()
}

#[test]
fn renderer_publication_drain_preserves_restored_family_appearance() {
    let mut scene = Scene::new();
    let source_objects = (0..3)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();
    let returned = source.copy_family().unwrap();
    returned.root().set_fill(None, Some(0.0)).unwrap();

    let contracted_objects = (0..2)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let contracted_members = contracted_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let contracted = scene.family(&contracted_members).unwrap();
    contracted.set_fill(None, Some(0.0)).unwrap();

    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();

    {
        let wait = scene.live(&mut execution).wait_segment(0.5).unwrap();
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);

    let outline = {
        let mut live = scene.live(&mut execution);
        let copied = live.copy_family(&source).unwrap();
        live.set_family_fill(copied.root(), None, Some(0.0))
            .unwrap();
        copied
    };
    {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, outline.root(), 0.35);
        assert_eq!(segment.end_time(), 0.85);
        finish(&mut live, segment);
    }
    drain(&mut execution);

    {
        let mut live = scene.live(&mut execution);
        let contraction = family_transform(&mut live, &source, &contracted, 1.8);
        assert_eq!(contraction.end_time(), 2.65);
        finish(&mut live, contraction);
    }
    drain(&mut execution);

    scene
        .live(&mut execution)
        .set_family_fill(&source, None, Some(1.0))
        .unwrap();
    drain(&mut execution);

    {
        let wait = scene.live(&mut execution).wait_segment(0.75).unwrap();
        assert_eq!(wait.end_time(), 3.4);
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);

    scene
        .live(&mut execution)
        .set_family_fill(&source, None, Some(0.0))
        .unwrap();
    drain(&mut execution);

    {
        let wait = scene.live(&mut execution).wait_segment(0.35).unwrap();
        assert_eq!(wait.end_time(), 3.75);
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);

    let contracted_state = scene
        .live(&mut execution)
        .effective(&source_objects[1])
        .unwrap();
    assert_eq!(
        contracted_state.appearance, 1.0,
        "contraction retained execution-only Appearance instead of releasing it"
    );
    assert_eq!(
        contracted_state.style.opacity, 0.0,
        "contracted source leaf lost its authored hidden opacity before return activation"
    );

    let restoration = {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, returned.root(), 1.8);
        assert_eq!(segment.end_time(), 5.55);
        assert_eq!(
            live.segment_state(segment).timeline(),
            TimelineWakeState::Continuous,
            "return Transform published no active execution driver"
        );
        let midpoint = segment.start_time() + segment.duration() * 0.5;
        live.advance_segment_to(segment, midpoint).unwrap();
        let midpoint_state = live.effective(&source_objects[1]).unwrap();
        assert_eq!(
            midpoint_state.appearance, 1.0,
            "return Transform reacquired the released Appearance domain"
        );
        assert!(
            midpoint_state.style.opacity > 0.0 && midpoint_state.style.opacity < 1.0,
            "source leaf 1 did not animate authored opacity at midpoint: {}",
            midpoint_state.style.opacity
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        for (index, object) in source_objects.iter().enumerate() {
            let state = live.effective(object).unwrap();
            assert_eq!(
                state.appearance, 1.0,
                "source leaf {index} did not keep neutral Appearance at the return endpoint"
            );
            assert_eq!(
                state.style.opacity, 1.0,
                "source leaf {index} was not restored at the return endpoint"
            );
        }
        segment
    };

    {
        let mut live = scene.live(&mut execution);
        live.complete_segment(restoration).unwrap();
        for (index, object) in source_objects.iter().enumerate() {
            let state = live.effective(object).unwrap();
            assert_eq!(
                state.appearance, 1.0,
                "source leaf {index} lost neutral Appearance during completion"
            );
            assert_eq!(
                state.style.opacity, 1.0,
                "source leaf {index} lost restored authored opacity during completion"
            );
        }
    }

    drain(&mut execution);

    let mut live = scene.live(&mut execution);
    for (index, object) in source_objects.iter().enumerate() {
        let state = live.effective(object).unwrap();
        assert_eq!(
            state.appearance, 1.0,
            "source leaf {index} lost neutral Appearance after renderer drain"
        );
        assert_eq!(
            state.style.opacity, 1.0,
            "source leaf {index} lost restored authored opacity after renderer drain"
        );
    }
    live.copy_family(&source).unwrap();
}

#[test]
fn complex_filled_family_round_trip_restores_padding_without_repainting() {
    use noon_core::{Color, Vec2, VectorPath};
    let mut scene = Scene::new();
    let concave = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(6.0, 8.0))
        .line_to(Vec2::new(6.0, 2.0))
        .line_to(Vec2::new(2.0, 2.0))
        .line_to(Vec2::new(2.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close();
    let rectangle = VectorPath::new()
        .move_to(Vec2::new(0.0, 0.0))
        .line_to(Vec2::new(8.0, 0.0))
        .line_to(Vec2::new(8.0, 8.0))
        .line_to(Vec2::new(0.0, 8.0))
        .close();
    let source_objects = (0..3)
        .map(|_| scene.path(concave.clone(), Default::default()).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();
    source.set_fill(Some(Color::WHITE), Some(1.0)).unwrap();
    let returned = source.copy_family().unwrap();
    let target_objects = (0..2)
        .map(|_| scene.path(rectangle.clone(), Default::default()).unwrap())
        .collect::<Vec<_>>();
    let target_members = target_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let target = scene.family(&target_members).unwrap();
    target.set_fill(Some(Color::WHITE), Some(1.0)).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, &target, 1.8);
        assert_eq!(
            live.segment_state(segment).timeline(),
            TimelineWakeState::Continuous
        );
        live.advance_segment_to(segment, 0.9).unwrap();
        finish(&mut live, segment);
    }
    drain(&mut execution);
    assert_eq!(
        scene
            .live(&mut execution)
            .effective(&source_objects[1])
            .unwrap()
            .appearance,
        0.0
    );
    {
        let wait = scene.live(&mut execution).wait_segment(0.75).unwrap();
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);
    {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, returned.root(), 1.8);
        assert_eq!(
            live.segment_state(segment).timeline(),
            TimelineWakeState::Continuous
        );
        live.advance_segment_to(segment, segment.start_time() + segment.duration() * 0.5)
            .unwrap();
        let appearance = live.effective(&source_objects[1]).unwrap().appearance;
        assert!(
            appearance > 0.0 && appearance < 1.0,
            "padding must recover continuously: {appearance}"
        );
        finish(&mut live, segment);
    }
    drain(&mut execution);
    {
        let wait = scene.live(&mut execution).wait_segment(0.85).unwrap();
        finish(&mut scene.live(&mut execution), wait);
    }
    drain(&mut execution);
    let mut live = scene.live(&mut execution);
    for object in &source_objects {
        assert_eq!(live.effective(object).unwrap().appearance, 1.0);
    }
    live.copy_family(&source)
        .expect("returned filled family remains capturable after hold/drain");
}

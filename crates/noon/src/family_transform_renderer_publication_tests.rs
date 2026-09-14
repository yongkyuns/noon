use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSegment, ExecutionSession, LiveSession,
    MobjectFamily, RateFunction, Scene, SemanticAnimationCompositionKind,
};

fn smooth(run_time: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(run_time)
        .rate_func(RateFunction::Smooth)
}

fn drain(session: &mut ExecutionSession) {
    let _ = session.take_renderer_publication();
}

fn finish(live: &mut LiveSession<'_>, segment: ExecutionSegment) {
    live.advance_segment_to(segment, segment.end_time()).unwrap();
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
        live.set_family_fill(copied.root(), None, Some(0.0)).unwrap();
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

    let restoration = {
        let mut live = scene.live(&mut execution);
        let segment = family_transform(&mut live, &source, returned.root(), 1.8);
        assert_eq!(segment.end_time(), 5.55);
        let midpoint = segment.start_time() + segment.duration() * 0.5;
        live.advance_segment_to(segment, midpoint).unwrap();
        let midpoint_appearance = live.effective(&source_objects[1]).unwrap().appearance;
        assert!(
            midpoint_appearance > 0.0 && midpoint_appearance < 1.0,
            "source leaf 1 did not animate appearance at midpoint: {midpoint_appearance}"
        );
        live.advance_segment_to(segment, segment.end_time()).unwrap();
        for (index, object) in source_objects.iter().enumerate() {
            assert_eq!(
                live.effective(object).unwrap().appearance,
                1.0,
                "source leaf {index} was not restored at the return endpoint"
            );
        }
        segment
    };

    {
        let mut live = scene.live(&mut execution);
        live.complete_segment(restoration).unwrap();
        for (index, object) in source_objects.iter().enumerate() {
            assert_eq!(
                live.effective(object).unwrap().appearance,
                1.0,
                "source leaf {index} lost restored appearance during completion"
            );
        }
    }

    drain(&mut execution);

    let mut live = scene.live(&mut execution);
    for (index, object) in source_objects.iter().enumerate() {
        assert_eq!(
            live.effective(object).unwrap().appearance,
            1.0,
            "source leaf {index} lost restored appearance after renderer drain"
        );
    }
    live.copy_family(&source).unwrap();
}

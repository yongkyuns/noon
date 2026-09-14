use crate::{AnimationOptions, ExecutionSegment, ExecutionSession, LiveSession, RateFunction, Scene};

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
        let segment = live
            .declare_and_activate_family_transform_to(&source, outline.root(), smooth(0.35))
            .unwrap();
        assert_eq!(segment.end_time(), 0.85);
        finish(&mut live, segment);
    }
    drain(&mut execution);

    {
        let mut live = scene.live(&mut execution);
        let contraction = live
            .declare_and_activate_family_transform_to(&source, &contracted, smooth(1.8))
            .unwrap();
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
        let segment = live
            .declare_and_activate_family_transform_to(&source, returned.root(), smooth(1.8))
            .unwrap();
        assert_eq!(segment.end_time(), 5.55);
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

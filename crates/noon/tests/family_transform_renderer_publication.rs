use noon::{AnimationOptions, RateFunction, Scene};

fn smooth(run_time: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(run_time)
        .rate_func(RateFunction::Smooth)
}

fn drain(session: &mut noon::ExecutionSession) {
    let _ = session.take_renderer_publication();
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
        let mut live = scene.live(&mut execution);
        let contraction = live
            .declare_and_activate_family_transform_to(&source, &contracted, smooth(1.8))
            .unwrap();
        live.advance_segment_to(contraction, contraction.end_time())
            .unwrap();
        live.complete_segment(contraction).unwrap();
    }
    drain(&mut execution);

    {
        let mut live = scene.live(&mut execution);
        live.set_family_fill(&source, None, Some(1.0)).unwrap();
    }
    drain(&mut execution);
    {
        let mut live = scene.live(&mut execution);
        live.set_family_fill(&source, None, Some(0.0)).unwrap();
    }
    drain(&mut execution);

    {
        let mut live = scene.live(&mut execution);
        let restoration = live
            .declare_and_activate_family_transform_to(&source, returned.root(), smooth(1.8))
            .unwrap();
        live.advance_segment_to(restoration, restoration.end_time())
            .unwrap();
        live.complete_segment(restoration).unwrap();
        for (index, object) in source_objects.iter().enumerate() {
            assert_eq!(
                live.effective(object).unwrap().appearance,
                1.0,
                "source leaf {index} was not restored before renderer drain"
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

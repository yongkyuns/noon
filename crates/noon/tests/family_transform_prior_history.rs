use noon::{AnimationOptions, RateFunction, Scene};

fn smooth(run_time: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(run_time)
        .rate_func(RateFunction::Smooth)
}

#[test]
fn prior_equal_family_style_transform_does_not_break_padding_restoration_capture() {
    let mut scene = Scene::new();

    let source_objects = (0..3)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();

    // Match the Python scene: save the eventual return target before any live
    // outline animation, then author both detached transform targets at fill 0.
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
    execution.advance_to(0.5).unwrap();

    // This is the previously missing piece from the browser reproduction:
    // source.animate.set_fill(opacity=0) is an equal-family Transform whose
    // completed style-track history remains in the execution plan.
    let outline = {
        let mut live = scene.live(&mut execution);
        let outline = live.copy_family(&source).unwrap();
        live.set_family_fill(outline.root(), None, Some(0.0))
            .unwrap();
        outline
    };
    {
        let mut live = scene.live(&mut execution);
        let segment = live
            .declare_and_activate_family_transform_to(&source, outline.root(), smooth(0.35))
            .unwrap();
        assert_eq!(segment.end_time(), 0.85);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }

    {
        let mut live = scene.live(&mut execution);
        let segment = live
            .declare_and_activate_family_transform_to(&source, &contracted, smooth(1.8))
            .unwrap();
        assert_eq!(segment.end_time(), 2.65);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert!(source_objects
            .iter()
            .any(|object| live.effective(object).unwrap().appearance == 0.0));
        live.set_family_fill(&source, None, Some(1.0)).unwrap();
    }

    execution.advance_to(3.4).unwrap();
    {
        let mut live = scene.live(&mut execution);
        live.set_family_fill(&source, None, Some(0.0)).unwrap();
    }
    execution.advance_to(3.75).unwrap();

    let mut live = scene.live(&mut execution);
    let restoration = live
        .declare_and_activate_family_transform_to(&source, returned.root(), smooth(1.8))
        .unwrap();
    assert_eq!(restoration.end_time(), 5.55);
    live.advance_segment_to(restoration, restoration.end_time())
        .unwrap();
    live.complete_segment(restoration).unwrap();

    for (index, object) in source_objects.iter().enumerate() {
        assert_eq!(
            live.effective(object).unwrap().appearance,
            1.0,
            "source leaf {index} did not restore appearance after prior style history"
        );
    }
    live.copy_family(&source).unwrap();
}

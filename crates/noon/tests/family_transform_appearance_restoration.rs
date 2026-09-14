use noon::{AnimationOptions, RateFunction, Scene};

fn linear_transform_options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}

#[test]
fn returning_from_unequal_family_transform_restores_padding_appearance_for_capture() {
    let mut scene = Scene::new();

    let s0 = scene.square(1.0).unwrap();
    let s1 = scene.square(1.0).unwrap();
    let s2 = scene.square(1.0).unwrap();
    let source = scene
        .family(&[(&s0).into(), (&s1).into(), (&s2).into()])
        .unwrap();

    let t0 = scene.square(1.0).unwrap();
    let t1 = scene.square(1.0).unwrap();
    let contracted = scene.family(&[(&t0).into(), (&t1).into()]).unwrap();

    let r0 = scene.square(1.0).unwrap();
    let r1 = scene.square(1.0).unwrap();
    let r2 = scene.square(1.0).unwrap();
    let restored = scene
        .family(&[(&r0).into(), (&r1).into(), (&r2).into()])
        .unwrap();

    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);

    let contraction = live
        .declare_and_activate_family_transform_to(&source, &contracted, linear_transform_options())
        .unwrap();
    live.advance_segment_to(contraction, contraction.end_time())
        .unwrap();
    live.complete_segment(contraction).unwrap();

    assert_eq!(live.effective(&s1).unwrap().appearance, 0.0);
    assert!(live.copy_family(&source).is_err());

    let restoration = live
        .declare_and_activate_family_transform_to(&source, &restored, linear_transform_options())
        .unwrap();
    live.advance_segment_to(restoration, restoration.end_time())
        .unwrap();
    assert_eq!(live.effective(&s1).unwrap().appearance, 1.0);
    live.complete_segment(restoration).unwrap();

    assert_eq!(live.effective(&s1).unwrap().appearance, 1.0);
    live.copy_family(&source).unwrap();
}

#[test]
fn large_unequal_family_round_trip_restores_every_source_appearance() {
    let mut scene = Scene::new();

    let source_objects = (0..138)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();

    let contracted_objects = (0..6)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let contracted_members = contracted_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let contracted = scene.family(&contracted_members).unwrap();

    let restored_objects = (0..138)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let restored_members = restored_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let restored = scene.family(&restored_members).unwrap();

    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);

    let contraction = live
        .declare_and_activate_family_transform_to(&source, &contracted, linear_transform_options())
        .unwrap();
    live.advance_segment_to(contraction, contraction.end_time())
        .unwrap();
    live.complete_segment(contraction).unwrap();

    assert!(source_objects
        .iter()
        .any(|object| live.effective(object).unwrap().appearance == 0.0));
    assert!(live.copy_family(&source).is_err());

    let restoration = live
        .declare_and_activate_family_transform_to(&source, &restored, linear_transform_options())
        .unwrap();
    live.advance_segment_to(restoration, restoration.end_time())
        .unwrap();
    live.complete_segment(restoration).unwrap();

    for (index, object) in source_objects.iter().enumerate() {
        assert_eq!(
            live.effective(object).unwrap().appearance,
            1.0,
            "source leaf {index} did not restore appearance"
        );
    }
    live.copy_family(&source).unwrap();
}

#[test]
fn decimal_timeline_round_trip_restores_appearance_at_5_55_boundary() {
    let mut scene = Scene::new();

    let source_objects = (0..3)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let source_members = source_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();
    let restored_copy = source.copy_family().unwrap();
    restored_copy.root().set_fill(None, Some(0.0)).unwrap();
    source.set_fill(None, Some(0.0)).unwrap();

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
    execution.seek(0.85).unwrap();

    {
        let mut live = scene.live(&mut execution);
        let contraction = live
            .declare_and_activate_family_transform_to(
                &source,
                &contracted,
                AnimationOptions::new()
                    .run_time(1.8)
                    .rate_func(RateFunction::Smooth),
            )
            .unwrap();
        assert_eq!(contraction.end_time(), 2.65);
        live.advance_segment_to(contraction, contraction.end_time())
            .unwrap();
        live.complete_segment(contraction).unwrap();
        assert!(source_objects
            .iter()
            .any(|object| live.effective(object).unwrap().appearance == 0.0));
        live.set_family_fill(&source, None, Some(1.0)).unwrap();
        assert!(source_objects
            .iter()
            .any(|object| live.effective(object).unwrap().appearance == 0.0));
    }

    execution.advance_to(3.4).unwrap();
    {
        let mut live = scene.live(&mut execution);
        live.set_family_fill(&source, None, Some(0.0)).unwrap();
        assert!(source_objects
            .iter()
            .any(|object| live.effective(object).unwrap().appearance == 0.0));
    }
    execution.advance_to(3.75).unwrap();

    let mut live = scene.live(&mut execution);
    let restoration = live
        .declare_and_activate_family_transform_to(
            &source,
            restored_copy.root(),
            AnimationOptions::new()
                .run_time(1.8)
                .rate_func(RateFunction::Smooth),
        )
        .unwrap();
    assert_eq!(restoration.end_time(), 5.55);
    live.advance_segment_to(restoration, restoration.end_time())
        .unwrap();
    for object in &source_objects {
        assert_eq!(live.effective(object).unwrap().appearance, 1.0);
    }
    live.complete_segment(restoration).unwrap();
    live.copy_family(&source).unwrap();
}

#[test]
fn nested_family_round_trip_restores_padding_appearance_for_capture() {
    let mut scene = Scene::new();

    let source_objects = (0..12)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let source_groups = source_objects
        .chunks(4)
        .map(|chunk| {
            let members = chunk.iter().map(|object| object.into()).collect::<Vec<_>>();
            scene.family(&members).unwrap()
        })
        .collect::<Vec<_>>();
    let source_group_members = source_groups
        .iter()
        .map(|group| group.into())
        .collect::<Vec<_>>();
    let source = scene.family(&source_group_members).unwrap();

    let contracted_objects = (0..3)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let contracted_members = contracted_objects
        .iter()
        .map(|object| object.into())
        .collect::<Vec<_>>();
    let contracted = scene.family(&contracted_members).unwrap();

    let restored_objects = (0..12)
        .map(|_| scene.square(1.0).unwrap())
        .collect::<Vec<_>>();
    let restored_groups = restored_objects
        .chunks(4)
        .map(|chunk| {
            let members = chunk.iter().map(|object| object.into()).collect::<Vec<_>>();
            scene.family(&members).unwrap()
        })
        .collect::<Vec<_>>();
    let restored_group_members = restored_groups
        .iter()
        .map(|group| group.into())
        .collect::<Vec<_>>();
    let restored = scene.family(&restored_group_members).unwrap();

    scene.add_many(&[(&source).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);

    let contraction = live
        .declare_and_activate_family_transform_to(&source, &contracted, linear_transform_options())
        .unwrap();
    live.advance_segment_to(contraction, contraction.end_time())
        .unwrap();
    live.complete_segment(contraction).unwrap();

    assert!(source_objects
        .iter()
        .any(|object| live.effective(object).unwrap().appearance == 0.0));
    assert!(live.copy_family(&source).is_err());

    let restoration = live
        .declare_and_activate_family_transform_to(&source, &restored, linear_transform_options())
        .unwrap();
    live.advance_segment_to(restoration, restoration.end_time())
        .unwrap();
    live.complete_segment(restoration).unwrap();

    for (index, object) in source_objects.iter().enumerate() {
        assert_eq!(
            live.effective(object).unwrap().appearance,
            1.0,
            "nested source leaf {index} did not restore appearance"
        );
    }
    live.copy_family(&source).unwrap();
}

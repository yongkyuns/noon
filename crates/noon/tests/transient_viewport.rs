use noon::{AnimationOptions, RateFunction, Rect, Scene, Vec2};

#[test]
fn renderer_viewport_keeps_offscreen_anchor_for_onscreen_transient() {
    let mut scene = Scene::new();

    let mut source_left = scene.circle(0.5).unwrap();
    source_left.shift(-20.0, 0.0).unwrap();
    let mut source_right = scene.circle(0.5).unwrap();
    source_right.shift(-18.0, 0.0).unwrap();
    let source = scene
        .family(&[(&source_left).into(), (&source_right).into()])
        .unwrap();

    let mut target_left = scene.circle(0.5).unwrap();
    target_left.shift(-20.0, 0.0).unwrap();
    let mut target_right = scene.circle(0.5).unwrap();
    target_right.shift(-18.0, 0.0).unwrap();
    let mut target_extra = scene.circle(0.5).unwrap();
    target_extra.shift(20.0, 0.0).unwrap();
    // A 2 -> 3 expansion aligns [s0, copy(s0), s1] with the target order.
    // The second target must move the copy; the third keeps s1 offscreen.
    let target = scene
        .family(&[
            (&target_left).into(),
            (&target_extra).into(),
            (&target_right).into(),
        ])
        .unwrap();

    scene.add_many(&[(&source).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = {
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_family_transform_to(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        live.advance_segment_to(segment, 0.5).unwrap();
        segment
    };
    assert!(!session.segment_state(segment).is_complete());

    let viewport = Rect::new(Vec2::new(-2.0, -2.0), Vec2::new(2.0, 2.0));
    let spatial = session.query_viewport(viewport);
    let renderer = session.renderer_viewport_query(spatial.clone());
    let publication = session.take_renderer_publication();
    let transient = publication.transient_presentations();
    assert_eq!(transient.len(), 1);

    let occurrence = &transient[0];
    let anchor = occurrence.anchor_object_index() as usize;
    assert!(
        occurrence
            .state()
            .effective_render_transform()
            .translation
            .x
            .abs()
            < 1.0e-5,
        "the derived occurrence should cross the viewport while its stable anchor remains offscreen"
    );
    assert_eq!(
        publication.frame().objects[anchor].transform.translation.x,
        -20.0
    );
    assert!(spatial.object_indices().is_empty());
    assert_eq!(renderer.object_indices(), &[anchor]);
    assert_eq!(renderer.spatial_stats(), spatial.spatial_stats());
}

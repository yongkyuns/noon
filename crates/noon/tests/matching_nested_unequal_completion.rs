use std::rc::Rc;

use noon::{
    AnimationCompositionRequest, AnimationOptions, Color, IndicateOptions, LiveSession,
    ManimGeometryOptions, Mobject, RateFunction, Scene, SemanticAnimationCompositionKind, Vec2,
    VectorPath,
};

fn triangle() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -0.5))
        .line_to(Vec2::new(-0.25, 1.0))
        .close()
}

fn shape(scene: &Scene, x: f64) -> Mobject {
    let mut object = Mobject::from_manim_geometry(
        Rc::clone(scene.integration_store()),
        ManimGeometryOptions::path(triangle()).unwrap(),
    )
    .unwrap();
    object.set_translation(x, 0.0).unwrap();
    object
        .set_fill(
            f64::from(Color::PINK.red),
            f64::from(Color::PINK.green),
            f64::from(Color::PINK.blue),
            0.8,
        )
        .unwrap();
    object.set_stroke_opacity(0.0).unwrap();
    object
}

fn run_case(source_count: usize, target_count: usize) {
    let mut scene = Scene::new();
    let source_leaves = (0..source_count)
        .map(|index| shape(&scene, index as f64 * 2.0 - 3.0))
        .collect::<Vec<_>>();
    let source_members = source_leaves
        .iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    let source = scene.family(&source_members).unwrap();
    scene.add_many(&[(&source).into()]).unwrap();

    let target_leaves = (0..target_count)
        .map(|index| shape(&scene, index as f64 * 2.0 - 2.0))
        .collect::<Vec<_>>();
    let target_members = target_leaves
        .iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    let target = scene.family(&target_members).unwrap();

    let store = Rc::clone(scene.integration_store());
    let root = scene.root();
    let mut session = scene.execution_session().unwrap();
    let mut live = LiveSession::new(&store, root, &mut session);

    let child = AnimationCompositionRequest::MatchingFamilyTransformTo {
        source: &source,
        target_state: &target,
        options: AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    };
    let request = AnimationCompositionRequest::Composition {
        kind: SemanticAnimationCompositionKind::Parallel,
        children: vec![child],
        options: AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear),
    };
    let segment = live
        .declare_and_activate_composition(&request, AnimationOptions::new())
        .unwrap();
    live.advance_segment_to(segment, 2.0).unwrap();
    live.complete_segment(segment).unwrap();

    drop(live);
    let publication = session.take_renderer_publication();
    assert!(
        publication.transient_presentations().is_empty(),
        "authored matching target owns the exact endpoint without transient duplicates"
    );
    let mut live = LiveSession::new(&store, root, &mut session);

    let indicate = live
        .declare_and_activate_family_indicate(
            &target,
            IndicateOptions::default(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::ThereAndBack),
        )
        .unwrap();
    live.advance_segment_to(indicate, 3.0).unwrap();
    live.complete_segment(indicate).unwrap();
}

#[test]
fn nested_matching_completion_handles_duplicate_growth_and_shrink() {
    run_case(2, 3);
    run_case(3, 2);
}

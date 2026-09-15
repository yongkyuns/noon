//! CPU-side native-host/renderer regressions included in the workspace library gate.

use super::*;
use noon::{AnimationOptions, RateFunction, Scene};
use noon_core::Vec2;
use noon_render_wgpu::{FramePreparer, RetainedFramePreparer};

fn viewport() -> Rect {
    Rect::new(Vec2::new(-2.0, -2.0), Vec2::new(2.0, 2.0))
}

fn expansion_source(with_second: bool) -> (Scene, StaticExecutionSource, noon::ExecutionSegment) {
    let mut scene = Scene::new();
    let mut left = scene.circle(0.5).unwrap();
    left.shift(-20.0, 0.0).unwrap();
    let mut right = scene.circle(0.5).unwrap();
    right.shift(-18.0, 0.0).unwrap();
    let source = if with_second {
        scene.family(&[(&left).into(), (&right).into()])
    } else {
        scene.family(&[(&left).into()])
    }
    .unwrap();

    let mut target_left = scene.circle(0.5).unwrap();
    target_left.shift(-20.0, 0.0).unwrap();
    let mut target_extra = scene.circle(0.5).unwrap();
    target_extra.shift(20.0, 0.0).unwrap();
    let mut target_right = scene.circle(0.5).unwrap();
    target_right
        .shift(if with_second { -18.0 } else { 22.0 }, 0.0)
        .unwrap();
    // Alignment is [s0, copy(s0), s1] or [s0, copy(s0), copy(s0)].
    let target = scene
        .family(&[
            (&target_left).into(),
            (&target_extra).into(),
            (&target_right).into(),
        ])
        .unwrap();

    let mut visible = scene.circle(0.25).unwrap();
    visible.shift(0.0, 1.25).unwrap();
    let mut unrelated = scene.circle(0.5).unwrap();
    unrelated.shift(80.0, 0.0).unwrap();
    scene
        .add_many(&[(&source).into(), (&visible).into(), (&unrelated).into()])
        .unwrap();

    let mut session = scene.execution_session().unwrap();
    let segment = {
        let mut live = scene.live(&mut session);
        live.declare_and_activate_family_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap()
    };
    (
        scene,
        StaticExecutionSource::new(session, RustHostCallbackTable::new()),
        segment,
    )
}

fn assert_canonical(query: &ExecutionViewportQuery, publication: &RendererPublication<'_>) {
    // A whole-order oracle is appropriate in a test, never in production projection.
    let expected = publication
        .painter_order()
        .iter()
        .map(|&index| index as usize)
        .filter(|index| query.object_indices().contains(index))
        .collect::<Vec<_>>();
    assert_eq!(query.object_indices(), expected.as_slice());
}

#[test]
fn native_viewport_prepares_onscreen_copy_with_offscreen_anchor() {
    let (_scene, mut source, _segment) = expansion_source(true);
    source.advance_to(0.5).unwrap();
    let stable_count = source.session.frame().objects.len();
    let spatial = source.session.query_viewport(viewport());
    let query = source.query_viewport(viewport());
    assert_eq!(source.session.frame().objects.len(), stable_count);
    assert_eq!(spatial.object_indices().len(), 1);
    assert_eq!(query.object_indices().len(), 2);
    assert_eq!(query.spatial_stats(), spatial.spatial_stats());

    let publication = source.take_renderer_publication();
    assert_canonical(&query, &publication);
    assert_eq!(publication.transient_presentations().len(), 1);
    let occurrence = &publication.transient_presentations()[0];
    let anchor = occurrence.anchor_object_index() as usize;
    let copy_x = occurrence
        .state()
        .effective_render_transform()
        .translation
        .x;
    assert!(copy_x.abs() < 1.0e-5);
    assert_eq!(
        publication.frame().objects[anchor].transform.translation.x,
        -20.0
    );
    assert!(!spatial.object_indices().contains(&anchor));
    assert!(query.object_indices().contains(&anchor));

    let mut preparer = RetainedFramePreparer::new();
    let missing = preparer
        .prepare_transient_presentations_visible(&publication, spatial.object_indices())
        .unwrap();
    assert!(
        missing.slots.is_empty(),
        "the old spatial-only route loses the copy"
    );
    let packed = preparer
        .prepare_transient_presentations_visible(&publication, query.object_indices())
        .unwrap();
    assert_eq!(packed.slots.len(), 1);
    assert_eq!(packed.slots[0].anchor_object_index as usize, anchor);
    assert_eq!(packed.stats.painter_positions_visited, 0);

    let mut geometry = FramePreparer::new();
    geometry.set_painter_order(publication.frame(), publication.painter_order());
    let stable = geometry
        .prepare_incremental_visible(
            publication.frame(),
            publication.changes(),
            query.object_indices(),
        )
        .unwrap();
    assert!(stable.observe_object(anchor).is_ok());
    let submitted: usize = stable
        .ordered_render_batches()
        .map(|item| (item.batch.instance_range.end - item.batch.instance_range.start) as usize)
        .sum();
    assert_eq!(
        submitted, 2,
        "unrelated offscreen stable rows must stay culled"
    );
}

#[test]
fn native_viewport_admits_a_shared_anchor_only_once() {
    let (_scene, mut source, _segment) = expansion_source(false);
    source.advance_to(0.5).unwrap();
    let spatial = source.session.query_viewport(viewport());
    let query = source.query_viewport(viewport());
    assert_eq!(
        query.object_indices().len(),
        spatial.object_indices().len() + 1
    );
    let publication = source.take_renderer_publication();
    assert_canonical(&query, &publication);
    let occurrences = publication.transient_presentations();
    assert_eq!(occurrences.len(), 2);
    let anchor = occurrences[0].anchor_object_index();
    assert_eq!(occurrences[1].anchor_object_index(), anchor);
    assert_eq!(
        query
            .object_indices()
            .iter()
            .filter(|&&index| index == anchor as usize)
            .count(),
        1
    );
    let mut preparer = RetainedFramePreparer::new();
    let packed = preparer
        .prepare_transient_presentations_visible(&publication, query.object_indices())
        .unwrap();
    assert_eq!(packed.slots.len(), 2);
    assert_eq!(packed.stats.painter_positions_visited, 0);
}

#[test]
fn native_viewport_leaves_visible_anchor_candidates_unchanged() {
    let (_scene, mut source, _segment) = expansion_source(true);
    source.advance_to(0.5).unwrap();
    let bounds = Rect::new(Vec2::new(-22.0, -2.0), Vec2::new(-16.0, 2.0));
    let spatial = source.session.query_viewport(bounds);
    let query = source.query_viewport(bounds);
    assert_eq!(query, spatial);
    let publication = source.take_renderer_publication();
    let anchor = publication.transient_presentations()[0].anchor_object_index() as usize;
    assert!(query.object_indices().contains(&anchor));
}

#[test]
fn native_viewport_seek_matches_forward_presentation() {
    let (_forward_scene, mut forward, _forward_segment) = expansion_source(true);
    forward.advance_to(0.25).unwrap();
    drop(forward.take_renderer_publication());
    forward.advance_to(0.5).unwrap();
    let forward_query = forward.query_viewport(viewport());
    let forward_occurrences = forward
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();

    let (_direct_scene, mut direct, _direct_segment) = expansion_source(true);
    direct.session.seek(0.5).unwrap();
    let direct_query = direct.query_viewport(viewport());
    let direct_occurrences = direct
        .take_renderer_publication()
        .transient_presentations()
        .to_vec();
    assert_eq!(
        forward_query.object_indices(),
        direct_query.object_indices()
    );
    assert_eq!(forward_occurrences, direct_occurrences);
}

#[test]
fn native_viewport_releases_transient_anchors_after_completion() {
    let (mut scene, mut source, segment) = expansion_source(true);
    source.session.advance_segment_to(segment, 1.0).unwrap();
    scene
        .live(&mut source.session)
        .complete_segment(segment)
        .unwrap();
    let spatial = source.session.query_viewport(viewport());
    let query = source.query_viewport(viewport());
    assert_eq!(query, spatial);
    assert!(source
        .take_renderer_publication()
        .transient_presentations()
        .is_empty());
}

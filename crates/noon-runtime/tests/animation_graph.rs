use noon_compile::CompiledScene;
use noon_core::{
    AnimationGraph, AnimationLowering, AnimationLoweringContext, AnimationTrackOrigin,
    AnimationTrackTemplate, GeometryRef, Property, RateFunction, SceneDefinition, ScenePatch,
    TrackTiming, TrackValues, Vec2,
};
use noon_runtime::SceneInstance;

fn position_leaf(
    graph: &mut AnimationGraph,
    object: noon_core::ObjectId,
    from: f32,
    to: f32,
) -> noon_core::AnimationNodeId {
    graph.insert_leaf(vec![AnimationTrackTemplate::new(
        object,
        Property::Position,
        TrackValues::Vec2 {
            from: Vec2::new(from, 0.0),
            to: Vec2::new(to, 0.0),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
    )])
}

#[test]
fn local_graph_relower_matches_fresh_compile_across_seek_and_advance() {
    let mut scene = SceneDefinition::new();
    let animated = scene.add(GeometryRef::circle(1.0));
    let unrelated = scene.add(GeometryRef::circle(0.5));

    let mut graph = AnimationGraph::new();
    let first = position_leaf(&mut graph, animated, 0.0, 10.0);
    let second = position_leaf(&mut graph, animated, 10.0, 20.0);
    let root = graph.insert_lagged(vec![first, second], 1.0).unwrap();
    let unrelated_leaf = position_leaf(&mut graph, unrelated, 0.0, -5.0);

    let mut lowering = AnimationLowering::new();
    lowering
        .lower_root(&graph, &mut scene, root, AnimationLoweringContext::new(0.0))
        .unwrap();
    lowering
        .lower_root(
            &graph,
            &mut scene,
            unrelated_leaf,
            AnimationLoweringContext::new(0.0),
        )
        .unwrap();

    let unrelated_origin = AnimationTrackOrigin {
        leaf: unrelated_leaf,
        track_index: 0,
    };
    let unrelated_track = lowering.track_for_origin(unrelated_origin).unwrap();
    let unrelated_before = scene
        .tracks()
        .iter()
        .find(|track| track.id == unrelated_track)
        .unwrap()
        .clone();

    let compiled = CompiledScene::compile(&scene).unwrap();
    assert_eq!(
        compiled.track_origin(unrelated_track),
        Some(unrelated_origin)
    );
    let mut live = SceneInstance::new(compiled);
    graph.set_lag_ratio(root, 0.5).unwrap();
    let relowered = lowering
        .relower_edited_subtree(&graph, &mut scene, second)
        .unwrap();

    assert_eq!(relowered.stats.tracks_added, 0);
    assert_eq!(relowered.stats.tracks_removed, 0);
    assert!(relowered.stats.tracks_replaced > 0);
    assert_eq!(
        lowering.track_for_origin(unrelated_origin),
        Some(unrelated_track)
    );
    assert_eq!(
        scene
            .tracks()
            .iter()
            .find(|track| track.id == unrelated_track),
        Some(&unrelated_before)
    );
    assert!(relowered.patches.iter().all(|patch| match patch {
        ScenePatch::ReplaceTrack(track) => track.object == animated,
        ScenePatch::AddTrack(track) => track.object == animated,
        ScenePatch::RemoveTrack(id) => *id != unrelated_track,
        _ => false,
    }));

    for patch in &relowered.patches {
        live.apply_patch(patch).unwrap();
        assert_eq!(live.last_patch_stats().full_group_rebuilds, 0);
    }

    let mut fresh = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    for time in [0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5] {
        let live_frame = live.seek(time).unwrap().clone();
        let fresh_frame = fresh.seek(time).unwrap().clone();
        assert_eq!(live_frame, fresh_frame, "seek mismatch at {time}");
    }

    live.seek(0.0).unwrap();
    fresh.seek(0.0).unwrap();
    for time in [0.1, 0.4, 0.8, 1.2, 1.5] {
        let live_frame = live.advance_to(time).unwrap().clone();
        let fresh_frame = fresh.advance_to(time).unwrap().clone();
        assert_eq!(live_frame, fresh_frame, "advance mismatch at {time}");
    }
}

#[test]
fn lifecycle_leaf_events_land_on_exact_composition_boundaries() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let mut graph = AnimationGraph::new();

    let introduce = graph.insert_leaf(vec![
        AnimationTrackTemplate::new(
            object,
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            TrackTiming::instant(0.0),
        ),
        AnimationTrackTemplate::new(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(10.0, 0.0),
            },
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        ),
    ]);
    let remove = graph.insert_leaf(vec![AnimationTrackTemplate::new(
        object,
        Property::Presence,
        TrackValues::Bool {
            from: true,
            to: false,
        },
        TrackTiming::instant(1.0),
    )]);
    let root = graph.insert_sequence(vec![introduce, remove]).unwrap();

    let mut lowering = AnimationLowering::new();
    lowering
        .lower_root(&graph, &mut scene, root, AnimationLoweringContext::new(1.0))
        .unwrap();

    let presence_times = scene
        .tracks()
        .iter()
        .filter(|track| track.property == Property::Presence)
        .map(|track| track.timing.start_time)
        .collect::<Vec<_>>();
    assert_eq!(presence_times, vec![1.0, 3.0]);

    let mut runtime = SceneInstance::new(CompiledScene::compile(&scene).unwrap());
    assert!(!runtime.seek(0.5).unwrap().is_present(0));
    assert!(runtime.seek(1.0).unwrap().is_present(0));
    assert!(runtime.seek(2.999).unwrap().is_present(0));
    assert!(!runtime.seek(3.0).unwrap().is_present(0));
}

use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, Style, TrackTiming, Transform2D, Vec2,
};
use noon_ir::encode_scene;
use noon_web::{
    normalized_frame_json, playback_snapshot_json, scene_snapshot_json, ReconcileOutcome, ScenePlayer,
};

fn build_scene() -> SceneDefinition {
    let mut scene = SceneDefinition::new();

    let circle = scene.add(GeometryRef::circle(0.75));
    scene
        .animate_position(
            circle,
            Vec2::new(-2.0, -1.0),
            Vec2::new(3.0, 1.5),
            TrackTiming::new(0.25, 2.5, Easing::Smooth),
        )
        .unwrap();
    scene
        .animate_scalar(
            circle,
            noon_core::Property::Opacity,
            1.0,
            0.2,
            TrackTiming::new(0.5, 1.75, Easing::ThereAndBack),
        )
        .unwrap();

    let rectangle = scene.add(GeometryRef::rectangle(2.0, 1.0));
    let from = ObjectSnapshot::new(GeometryRef::rectangle(2.0, 1.0));
    let to = ObjectSnapshot::new(GeometryRef::rectangle(4.0, 2.0))
        .shift(Vec2::new(-1.5, 2.0))
        .rotate_by(0.8)
        .set_opacity(0.55);
    scene
        .animate_transform(
            rectangle,
            from,
            to,
            TrackTiming::new(1.0, 2.0, Easing::EaseInOutCubic),
        )
        .unwrap();

    let line = scene.add(GeometryRef::line(
        Vec2::new(-1.0, 0.0),
        Vec2::new(1.0, 0.0),
    ));
    scene
        .animate_reveal(
            line,
            0.0,
            1.0,
            TrackTiming::new(0.0, 1.2, Easing::Linear),
        )
        .unwrap();

    scene
}

fn scene_json(scene: &SceneDefinition) -> String {
    encode_scene(scene).expect("test scene encodes")
}

#[test]
fn direct_seek_incremental_playback_and_backward_scrub_have_identical_frames() {
    let json = scene_json(&build_scene());
    let targets = [0.0, 0.25, 0.499, 0.5, 1.0, 1.2, 1.75, 2.25, 2.75, 3.0, 4.0];

    for &target in &targets {
        let direct = scene_snapshot_json(&json, target).unwrap();

        let mut forward = Vec::new();
        let steps = 37;
        for step in 0..=steps {
            forward.push(target * step as f64 / steps as f64);
        }
        let incremental = playback_snapshot_json(&json, &forward).unwrap();
        assert_eq!(
            incremental, direct,
            "incremental playback diverged at target={target}"
        );

        let rewind = playback_snapshot_json(&json, &[0.1, 1.3, 2.9, 0.7, 3.4, target]).unwrap();
        assert_eq!(rewind, direct, "backward scrub diverged at target={target}");
    }
}

#[test]
fn repeated_evaluation_is_stable_at_boundaries_and_extreme_valid_times() {
    let json = scene_json(&build_scene());
    for time in [
        0.0,
        f64::EPSILON,
        0.25,
        0.5,
        1.0,
        1.2,
        2.25,
        2.75,
        3.0,
        1.0e-9,
        1.0e6,
    ] {
        let first = scene_snapshot_json(&json, time).unwrap();
        let second = scene_snapshot_json(&json, time).unwrap();
        assert_eq!(first, second, "fresh evaluation drifted at time={time}");

        let replayed = playback_snapshot_json(&json, &[time, time, time]).unwrap();
        assert_eq!(replayed, first, "same-frame replay drifted at time={time}");
    }
}

#[test]
fn reconcile_and_full_replacement_match_fresh_compile_at_the_same_playhead() {
    let base = build_scene();
    let base_json = scene_json(&base);
    let mut desired = base.clone();

    let first = desired.objects()[0].id;
    desired.object_mut(first).unwrap().transform = Transform2D {
        translation: Vec2::new(5.0, -3.0),
        rotation: -0.2,
        scale: Vec2::new(1.2, 0.8),
    };
    desired.object_mut(first).unwrap().style = Style {
        opacity: 0.42,
        stroke_width: 2.5,
        ..Style::default()
    };

    let added = desired.add(GeometryRef::circle(0.25));
    desired
        .animate_position(
            added,
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 2.0),
            TrackTiming::new(0.4, 1.6, Easing::RushFrom),
        )
        .unwrap();
    let desired_json = scene_json(&desired);

    for playhead in [0.0, 0.5, 1.1, 2.2, 3.4] {
        let expected = scene_snapshot_json(&desired_json, playhead).unwrap();

        let mut reconciled = ScenePlayer::from_scene_json(&base_json).unwrap();
        reconciled.seek(playhead).unwrap();
        let outcome = reconciled.reconcile_scene_json(&desired_json).unwrap();
        assert!(matches!(
            outcome,
            ReconcileOutcome::Incremental { .. } | ReconcileOutcome::Rebuilt { .. }
        ));
        assert_eq!(
            normalized_frame_json(reconciled.frame()),
            expected,
            "reconcile diverged at playhead={playhead}"
        );

        let mut replaced = ScenePlayer::from_scene_json(&base_json).unwrap();
        replaced.seek(playhead).unwrap();
        replaced.replace_scene_json(&desired_json).unwrap();
        assert_eq!(
            normalized_frame_json(replaced.frame()),
            expected,
            "replacement diverged at playhead={playhead}"
        );
    }
}

#[test]
fn normalized_snapshot_omits_execution_bookkeeping_but_preserves_render_observables() {
    let json = scene_json(&build_scene());
    let snapshot: serde_json::Value = serde_json::from_str(&scene_snapshot_json(&json, 1.2).unwrap())
        .expect("snapshot is valid JSON");

    assert_eq!(snapshot["time"], 1.2);
    let objects = snapshot["objects"].as_array().expect("objects array");
    assert_eq!(objects.len(), 3);
    for object in objects {
        assert!(object.get("id").is_some());
        assert!(object.get("geometry").is_some());
        assert!(object.get("transform").is_some());
        assert!(object.get("style").is_some());
        assert!(object.get("appearance").is_some());
        assert!(object.get("present").is_some());
        assert!(object.get("reveal").is_some());
        assert!(object.get("morph").is_some());
        assert!(object.get("render_geometry").is_some());
    }
    assert!(snapshot.get("groups").is_none());
    assert!(snapshot.get("cursors").is_none());
    assert!(snapshot.get("cache").is_none());
}

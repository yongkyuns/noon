use noon_core::{GeometryRef, SceneDefinition, ScenePatch, Transform2D, Vec2};
use noon_ir::encode_scene;
use noon_web::ScenePlayer;
use serde_json::Value;

const STATIC_OBJECTS: usize = 255;
const REPLACEMENTS: u64 = 128;
const MAX_LOCAL_CANDIDATES: u64 = 64;

fn hit(player: &ScenePlayer) -> Value {
    serde_json::from_str(&player.hit_test_json(0.0, 0.0))
        .expect("browser hit-test result must be valid JSON")
}

#[test]
fn browser_hit_test_tracks_generation_safe_slot_reuse_without_full_scan() {
    let mut desired = SceneDefinition::new();
    let mut target = desired.add(GeometryRef::rectangle(0.5, 0.5));

    for index in 0..STATIC_OBJECTS {
        let object = desired.add(GeometryRef::rectangle(0.5, 0.5));
        desired.object_mut(object).unwrap().transform = Transform2D {
            translation: Vec2::new((index as f32 + 1.0) * 4.0, 0.0),
            ..Transform2D::IDENTITY
        };
    }

    let mut player = ScenePlayer::from_scene_json(&encode_scene(&desired).unwrap())
        .expect("sparse scene must initialize");
    let initial = hit(&player);
    assert_eq!(initial["slots"].as_array().unwrap().len(), 1);
    let reused_slot = initial["slots"][0]["slot"].as_u64().unwrap();
    let mut generation = initial["slots"][0]["generation"].as_u64().unwrap();
    assert_eq!(initial["stats"]["full_scan_fallbacks"], 0);

    for _ in 0..REPLACEMENTS {
        desired
            .apply_patch(ScenePatch::RemoveObject(target))
            .expect("target removal must remain valid");
        target = desired.add(GeometryRef::rectangle(0.5, 0.5));

        player
            .reconcile_scene_json(&encode_scene(&desired).unwrap())
            .expect("bounded structural replacement must reconcile incrementally");

        let result = hit(&player);
        let slots = result["slots"].as_array().unwrap();
        let candidates = result["stats"]["candidates_tested"].as_u64().unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0]["slot"].as_u64().unwrap(), reused_slot);
        let next_generation = slots[0]["generation"].as_u64().unwrap();
        assert_eq!(next_generation, generation + 1);
        generation = next_generation;
        assert_eq!(result["stats"]["full_scan_fallbacks"], 0);
        assert!(
            candidates <= MAX_LOCAL_CANDIDATES,
            "browser hit test examined {candidates} candidates after structural slot reuse"
        );
        assert_eq!(player.last_transaction_stats().runtime_rebuilds, 0);
        assert_eq!(player.last_transaction_stats().semantic_scene_clones, 0);
    }
}

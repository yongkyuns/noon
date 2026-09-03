//! Browser-facing serialization for retained spatial-index queries.
//!
//! Inputs are Noon world coordinates. Results preserve retained painter order and
//! opaque execution-slot generations so browser consumers never need a scene scan or
//! semantic-ID/frame-row remapping.

use noon_core::{Rect, Vec2};
use noon_runtime::{SlottedSceneInstance, SpatialQueryResult};
use serde_json::json;

fn spatial_query_json(result: SpatialQueryResult) -> String {
    let stats = result.stats();
    let slots = result
        .slots()
        .iter()
        .map(|slot| {
            json!({
                "slot": slot.slot(),
                "generation": slot.generation(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "slots": slots,
        "stats": {
            "cells_visited": stats.cells_visited,
            "candidates_tested": stats.candidates_tested,
            "results": stats.results,
            "full_scan_fallbacks": stats.full_scan_fallbacks,
        },
    })
    .to_string()
}

/// Serialize one retained runtime hit-test without routing through the legacy
/// browser player. Spatial identity and locality are owned by Runtime; this
/// adapter only exposes that result at the browser serialization boundary.
pub fn hit_test_json(runtime: &SlottedSceneInstance, x: f32, y: f32) -> String {
    spatial_query_json(runtime.hit_test(Vec2::new(x, y)))
}

/// Serialize one retained runtime viewport query without making `ScenePlayer` a
/// dependency of the spatial-query boundary.
pub fn query_viewport_json(
    runtime: &SlottedSceneInstance,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> String {
    spatial_query_json(
        runtime.query_viewport(Rect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))),
    )
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledScene;
    use noon_core::{GeometryRef, SceneDefinition, Transform2D};
    use noon_core::{MutationTransaction, ScenePatch};
    use noon_runtime::SlottedSceneInstance;
    use serde_json::Value;

    use super::*;

    fn query_runtime() -> SlottedSceneInstance {
        let mut scene = SceneDefinition::new();
        let back = scene.add(GeometryRef::circle(1.0));
        let front = scene.add(GeometryRef::rectangle(1.0, 1.0));
        scene.object_mut(back).unwrap().transform = Transform2D {
            translation: Vec2::new(0.0, 0.0),
            ..Transform2D::IDENTITY
        };
        scene.object_mut(front).unwrap().transform = Transform2D {
            translation: Vec2::new(0.0, 0.0),
            ..Transform2D::IDENTITY
        };
        SlottedSceneInstance::new(CompiledScene::compile(&scene).unwrap())
    }

    #[test]
    fn hit_test_json_preserves_painter_order_and_query_metrics() {
        let runtime = query_runtime();
        let result: Value = serde_json::from_str(&hit_test_json(&runtime, 0.0, 0.0)).unwrap();
        let slots = result["slots"].as_array().unwrap();

        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0]["slot"], 1);
        assert_eq!(slots[1]["slot"], 0);
        assert_eq!(slots[0]["generation"], 0);
        assert_eq!(result["stats"]["results"], 2);
        assert_eq!(result["stats"]["full_scan_fallbacks"], 0);
        assert!(result["stats"]["candidates_tested"].as_u64().unwrap() >= 2);
    }

    #[test]
    fn hit_test_json_stays_local_in_large_sparse_scene() {
        const OBJECT_COUNT: usize = 4_096;
        const MAX_LOCAL_CANDIDATES: u64 = 64;

        let mut scene = SceneDefinition::new();
        for object_index in 0..OBJECT_COUNT {
            let object = scene.add(GeometryRef::rectangle(0.5, 0.5));
            scene.object_mut(object).unwrap().transform = Transform2D {
                translation: Vec2::new(object_index as f32 * 4.0, 0.0),
                ..Transform2D::IDENTITY
            };
        }
        let runtime = SlottedSceneInstance::new(CompiledScene::compile(&scene).unwrap());
        let result: Value = serde_json::from_str(&hit_test_json(&runtime, 0.0, 0.0)).unwrap();
        let slots = result["slots"].as_array().unwrap();
        let candidates = result["stats"]["candidates_tested"].as_u64().unwrap();

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0]["slot"], 0);
        assert_eq!(slots[0]["generation"], 0);
        assert_eq!(result["stats"]["results"], 1);
        assert_eq!(result["stats"]["full_scan_fallbacks"], 0);
        assert!(
            candidates <= MAX_LOCAL_CANDIDATES,
            "browser hit-test bridge examined {candidates} candidates for {OBJECT_COUNT} sparse objects"
        );
    }

    #[test]
    fn hit_test_json_tracks_incremental_transform_without_runtime_rebuild() {
        let mut scene = SceneDefinition::new();
        let object = scene.add(GeometryRef::rectangle(0.5, 0.5));
        scene.object_mut(object).unwrap().transform = Transform2D {
            translation: Vec2::new(8.0, 0.0),
            ..Transform2D::IDENTITY
        };

        let mut runtime = SlottedSceneInstance::new(CompiledScene::compile(&scene).unwrap());
        let initial: Value = serde_json::from_str(&hit_test_json(&runtime, 0.0, 0.0)).unwrap();
        assert!(initial["slots"].as_array().unwrap().is_empty());

        let moved_in = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object,
            transform: Transform2D::IDENTITY,
        }]);
        runtime.apply_transaction(&moved_in).unwrap();
        let moved_in: Value = serde_json::from_str(&hit_test_json(&runtime, 0.0, 0.0)).unwrap();
        assert_eq!(moved_in["slots"].as_array().unwrap().len(), 1);
        assert_eq!(moved_in["slots"][0]["slot"], 0);
        assert_eq!(moved_in["slots"][0]["generation"], 0);
        assert_eq!(moved_in["stats"]["full_scan_fallbacks"], 0);

        let moved_out = MutationTransaction::from_mutations([ScenePatch::SetTransform {
            object,
            transform: Transform2D {
                translation: Vec2::new(8.0, 0.0),
                ..Transform2D::IDENTITY
            },
        }]);
        runtime.apply_transaction(&moved_out).unwrap();
        let moved_out: Value = serde_json::from_str(&hit_test_json(&runtime, 0.0, 0.0)).unwrap();
        assert!(moved_out["slots"].as_array().unwrap().is_empty());
        assert_eq!(moved_out["stats"]["full_scan_fallbacks"], 0);
    }

    #[test]
    fn viewport_json_exposes_generation_safe_slots_without_scene_scan() {
        let runtime = query_runtime();
        let result: Value =
            serde_json::from_str(&query_viewport_json(&runtime, -0.25, -0.25, 0.25, 0.25)).unwrap();

        assert_eq!(result["slots"].as_array().unwrap().len(), 2);
        assert_eq!(result["stats"]["results"], 2);
        assert_eq!(result["stats"]["full_scan_fallbacks"], 0);
        assert!(result["stats"]["cells_visited"].as_u64().unwrap() > 0);
    }
}

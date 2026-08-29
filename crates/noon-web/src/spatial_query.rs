//! Browser-facing serialization for retained spatial-index queries.
//!
//! Inputs are Noon world coordinates. Results preserve retained painter order and
//! opaque execution-slot generations so browser consumers never need a scene scan or
//! semantic-ID/frame-row remapping.

use noon_core::{Rect, Vec2};
use noon_runtime::SpatialQueryResult;
use serde_json::json;

use crate::ScenePlayer;

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

impl ScenePlayer {
    pub fn hit_test_json(&self, x: f32, y: f32) -> String {
        spatial_query_json(self.hit_test(Vec2::new(x, y)))
    }

    pub fn query_viewport_json(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> String {
        spatial_query_json(
            self.query_viewport(Rect::new(Vec2::new(min_x, min_y), Vec2::new(max_x, max_y))),
        )
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, SceneDefinition, Transform2D};
    use noon_ir::encode_scene;
    use serde_json::Value;

    use super::*;

    fn query_player() -> ScenePlayer {
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
        let json = encode_scene(&scene).unwrap();
        ScenePlayer::from_scene_json(&json).unwrap()
    }

    #[test]
    fn hit_test_json_preserves_painter_order_and_query_metrics() {
        let player = query_player();
        let result: Value = serde_json::from_str(&player.hit_test_json(0.0, 0.0)).unwrap();
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
    fn viewport_json_exposes_generation_safe_slots_without_scene_scan() {
        let player = query_player();
        let result: Value =
            serde_json::from_str(&player.query_viewport_json(-0.25, -0.25, 0.25, 0.25)).unwrap();

        assert_eq!(result["slots"].as_array().unwrap().len(), 2);
        assert_eq!(result["stats"]["results"], 2);
        assert_eq!(result["stats"]["full_scan_fallbacks"], 0);
        assert!(result["stats"]["cells_visited"].as_u64().unwrap() > 0);
    }
}

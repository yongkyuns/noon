use noon_core::{Color, ObjectSnapshot, Rect};
use noon_runtime::FrameState;
use serde_json::{json, Value};

use crate::{PlayerError, ScenePlayer};

fn paint_json(color: Option<Color>, opacity: f32) -> Value {
    match color {
        Some(color) => json!({
            "red": color.red,
            "green": color.green,
            "blue": color.blue,
            "alpha": color.alpha * opacity,
        }),
        None => Value::Null,
    }
}

fn bounds_json(bounds: Option<Rect>) -> Value {
    match bounds {
        Some(bounds) => json!({
            "min": [bounds.min.x, bounds.min.y],
            "max": [bounds.max.x, bounds.max.y],
            "width": bounds.width(),
            "height": bounds.height(),
        }),
        None => Value::Null,
    }
}

pub fn semantic_frame_value(frame: &FrameState) -> Value {
    let objects = frame
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| {
            let geometry = frame.render_geometry(index).clone();
            let snapshot = ObjectSnapshot {
                geometry: geometry.clone(),
                transform: object.transform,
                style: object.style,
            };
            let bounds = snapshot.world_bounds();
            let center = bounds
                .map(Rect::center)
                .unwrap_or(object.transform.translation);
            let effective_opacity = object.style.opacity * object.appearance;
            json!({
                "id": object.id.get(),
                "present": frame.is_present(index),
                "center": [center.x, center.y],
                "bounds": bounds_json(bounds),
                "geometry": geometry,
                "transform": object.transform,
                "fill": paint_json(object.style.fill, effective_opacity),
                "stroke": paint_json(object.style.stroke, effective_opacity),
                "stroke_width": object.style.stroke_width,
                "stroke_width_mode": object.style.stroke_width_mode,
                "stroke_join": object.style.stroke_join,
                "stroke_cap": object.style.stroke_cap,
                "style_opacity": object.style.opacity,
                "appearance": object.appearance,
                "reveal": frame.reveal(index),
                "morph": frame.morph(index),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "engine": "noon",
        "time": frame.time,
        "present_object_count": frame
            .presences
            .iter()
            .copied()
            .filter(|present| *present)
            .count(),
        "objects": objects,
    })
}

pub fn semantic_frame_json(scene_json: &str, time: f64) -> Result<String, PlayerError> {
    let mut player = ScenePlayer::from_scene_json(scene_json)?;
    player.seek(time)?;
    Ok(semantic_frame_value(player.frame()).to_string())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(js_name = semanticFrameJson)]
    pub fn wasm_semantic_frame_json(scene_json: &str, time: f64) -> Result<String, JsValue> {
        super::semantic_frame_json(scene_json, time)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{Color, GeometryRef, Transform2D, Vec2};
    use noon_ir::encode_scene;

    use super::*;

    #[test]
    fn semantic_snapshot_reports_runtime_state_used_by_rendering() {
        let mut scene = noon_core::SceneDefinition::new();
        let circle = scene.add(GeometryRef::circle(1.0));
        let object = scene.object_mut(circle).expect("circle exists");
        object.transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            scale: Vec2::new(1.5, 0.5),
            ..Transform2D::IDENTITY
        };
        object.style.fill = Some(Color::rgba(0.25, 0.5, 0.75, 0.4));
        object.style.stroke = Some(Color::rgba(1.0, 0.0, 0.0, 0.8));
        object.style.opacity = 0.5;
        object.style.stroke_width = 0.04;

        let scene_json = encode_scene(&scene).expect("scene serializes");
        let snapshot = semantic_frame_json(&scene_json, 0.0).expect("snapshot succeeds");
        let value: Value = serde_json::from_str(&snapshot).expect("snapshot is JSON");

        assert_eq!(value["engine"], "noon");
        assert_eq!(value["present_object_count"], 1);
        assert_eq!(value["objects"][0]["present"], true);
        assert_eq!(value["objects"][0]["center"][0], 2.0);
        assert_eq!(value["objects"][0]["center"][1], -1.0);
        assert_eq!(value["objects"][0]["bounds"]["width"], 3.0);
        assert_eq!(value["objects"][0]["bounds"]["height"], 1.0);
        assert!((value["objects"][0]["fill"]["alpha"].as_f64().unwrap() - 0.2).abs() < 1e-6);
        assert!((value["objects"][0]["stroke"]["alpha"].as_f64().unwrap() - 0.4).abs() < 1e-6);
        assert!(
            (value["objects"][0]["stroke_width"].as_f64().unwrap() - 0.04).abs() < 1e-6
        );
        assert_eq!(value["objects"][0]["reveal"], 1.0);
    }
}

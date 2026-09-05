use noon::legacy::{Elbow, IntoSnapshot};
use noon_core::ObjectSnapshot;

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn snapshot_json(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize Manim Elbow snapshot: {error}"))
}

pub fn manim_elbow_snapshot_json(width: f64, angle: f64) -> Result<String, String> {
    let elbow = Elbow::with_options(finite_f32("width", width)?, finite_f32("angle", angle)?)
        .map_err(|error| error.to_string())?;
    snapshot_json(elbow.into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::manim_elbow_snapshot_json;

    #[wasm_bindgen(js_name = manimElbowSnapshotJson)]
    pub fn manim_elbow_snapshot(width: f64, angle: f64) -> Result<String, JsValue> {
        manim_elbow_snapshot_json(width, angle).map_err(|error| JsValue::from_str(&error))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode, Vec2, WHITE,
    };

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid ObjectSnapshot JSON")
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn elbow_bridge_preserves_shared_path_and_style_defaults() {
        let snapshot = decode(&manim_elbow_snapshot_json(0.2, 0.0).expect("valid Elbow"));
        let GeometryRef::VectorPath(path) = &snapshot.geometry else {
            panic!("Elbow bridge must preserve retained VectorPath geometry")
        };
        assert_eq!(
            path.commands(),
            &[
                PathCommand::MoveTo {
                    to: Vec2::new(0.0, 0.2),
                },
                PathCommand::LineTo {
                    to: Vec2::new(0.2, 0.2),
                },
                PathCommand::LineTo {
                    to: Vec2::new(0.2, 0.0),
                },
            ]
        );
        assert_eq!(snapshot.style.stroke, Some(WHITE));
        assert_eq!(snapshot.style.fill.map(|color| color.alpha), Some(0.0));
        assert_eq!(snapshot.style.stroke_width, 0.04);
        assert_eq!(
            snapshot.style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
        assert_eq!(snapshot.style.stroke_join, StrokeJoin::Miter);
        assert_eq!(snapshot.style.stroke_cap, StrokeCap::Butt);
    }

    #[test]
    fn elbow_bridge_preserves_constructor_rotated_public_geometry() {
        let angle = 5.0 * std::f64::consts::PI / 4.0;
        let snapshot = decode(&manim_elbow_snapshot_json(2.0, angle).expect("valid Elbow"));
        let root_two = 2.0_f32.sqrt();

        assert_eq!(snapshot.transform.rotation, 0.0);
        assert_close(snapshot.center().x, 0.0);
        assert_close(snapshot.center().y, -1.5 * root_two);
        assert_close(snapshot.width(), 2.0 * root_two);
        assert_close(snapshot.height(), root_two);
    }

    #[test]
    fn elbow_bridge_preserves_zero_and_negative_widths() {
        assert!(manim_elbow_snapshot_json(0.0, 0.0).is_ok());
        assert!(manim_elbow_snapshot_json(-0.5, 0.0).is_ok());
    }

    #[test]
    fn elbow_bridge_rejects_non_renderable_inputs() {
        assert!(manim_elbow_snapshot_json(f64::NAN, 0.0).is_err());
        assert!(manim_elbow_snapshot_json(0.2, f64::INFINITY).is_err());
        assert!(manim_elbow_snapshot_json(f64::MAX, 0.0).is_err());
    }
}

use noon::{DashedLine, IntoSnapshot};
use noon_core::{ObjectSnapshot, Vec2};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn snapshot_json(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize Manim DashedLine snapshot: {error}"))
}

pub fn manim_dashed_line_snapshot_json(
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    dash_length: f64,
    dashed_ratio: f64,
) -> Result<String, String> {
    let start = Vec2::new(
        finite_f32("start.x", start_x)?,
        finite_f32("start.y", start_y)?,
    );
    let end = Vec2::new(finite_f32("end.x", end_x)?, finite_f32("end.y", end_y)?);
    let line = DashedLine::with_options(start, end, dash_length, dashed_ratio)
        .map_err(|error| error.to_string())?;
    snapshot_json(line.into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::manim_dashed_line_snapshot_json;

    #[wasm_bindgen(js_name = manimDashedLineSnapshotJson)]
    pub fn manim_dashed_line_snapshot(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        dash_length: f64,
        dashed_ratio: f64,
    ) -> Result<String, JsValue> {
        manim_dashed_line_snapshot_json(start_x, start_y, end_x, end_y, dash_length, dashed_ratio)
            .map_err(|error| JsValue::from_str(&error))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon::{DEFAULT_DASHED_RATIO, DEFAULT_DASH_LENGTH};
    use noon_core::{
        GeometryRef, PathCommand, StrokeCap, StrokeJoin, StrokeWidthMode, Vec2, WHITE,
    };

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid ObjectSnapshot JSON")
    }

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        let GeometryRef::VectorPath(path) = &snapshot.geometry else {
            panic!("DashedLine bridge must preserve retained VectorPath geometry")
        };
        path.commands()
    }

    #[test]
    fn bridge_preserves_shared_default_dash_geometry_and_style() {
        let snapshot = decode(
            &manim_dashed_line_snapshot_json(
                -1.0,
                0.0,
                1.0,
                0.0,
                DEFAULT_DASH_LENGTH,
                DEFAULT_DASHED_RATIO,
            )
            .expect("valid DashedLine"),
        );
        let commands = commands(&snapshot);
        assert_eq!(commands.len(), 40);
        assert_eq!(
            commands.first(),
            Some(&PathCommand::MoveTo {
                to: Vec2::new(-1.0, 0.0),
            })
        );
        assert_eq!(
            commands.last(),
            Some(&PathCommand::LineTo {
                to: Vec2::new(1.0, 0.0),
            })
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
    fn bridge_preserves_f64_dash_count_decision_before_coordinate_lowering() {
        let snapshot = decode(
            &manim_dashed_line_snapshot_json(0.0, 0.0, 2.0, 0.0, 0.3, 0.5)
                .expect("valid custom DashedLine"),
        );
        // max(2, ceil(2 / 0.3 * 0.5)) = 4 dashes, two commands each.
        assert_eq!(commands(&snapshot).len(), 8);
    }

    #[test]
    fn bridge_rejects_invalid_host_and_dash_inputs() {
        assert!(manim_dashed_line_snapshot_json(
            f64::NAN,
            0.0,
            1.0,
            0.0,
            DEFAULT_DASH_LENGTH,
            DEFAULT_DASHED_RATIO,
        )
        .is_err());
        assert!(manim_dashed_line_snapshot_json(0.0, 0.0, 1.0, 0.0, 0.0, 0.5).is_err());
        assert!(manim_dashed_line_snapshot_json(0.0, 0.0, 1.0, 0.0, 0.1, 1.5).is_err());
        assert!(manim_dashed_line_snapshot_json(
            0.0,
            0.0,
            f64::MAX,
            0.0,
            DEFAULT_DASH_LENGTH,
            DEFAULT_DASHED_RATIO,
        )
        .is_err());
    }
}

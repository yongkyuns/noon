use noon::{
    BackgroundRectangle, Cross, IntoSnapshot, SurroundingRectangle, Underline,
    BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY, DEFAULT_CROSS_SCALE_FACTOR,
    DEFAULT_CROSS_STROKE_WIDTH, DEFAULT_UNDERLINE_BUFF, SURROUNDING_RECTANGLE_DEFAULT_COLOR,
};
use noon_core::{ObjectSnapshot, Vec2, BLACK, RED};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn decode_target(snapshot_json: &str) -> Result<ObjectSnapshot, String> {
    serde_json::from_str(snapshot_json)
        .map_err(|error| format!("invalid shape matcher target snapshot: {error}"))
}

fn encode_snapshot(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize shape matcher snapshot: {error}"))
}

pub fn manim_surrounding_rectangle_snapshot_json(
    target_snapshot_json: &str,
    buff_x: f64,
    buff_y: f64,
    corner_radius: f64,
) -> Result<String, String> {
    let target = decode_target(target_snapshot_json)?;
    let matcher = SurroundingRectangle::around(
        [&target],
        Vec2::new(finite_f32("buff.x", buff_x)?, finite_f32("buff.y", buff_y)?),
        finite_f32("corner_radius", corner_radius)?,
        SURROUNDING_RECTANGLE_DEFAULT_COLOR,
    )
    .map_err(|error| error.to_string())?;
    encode_snapshot(matcher.into_snapshot())
}

pub fn manim_background_rectangle_snapshot_json(
    target_snapshot_json: &str,
    buff_x: f64,
    buff_y: f64,
    corner_radius: f64,
    fill_opacity: f64,
) -> Result<String, String> {
    let target = decode_target(target_snapshot_json)?;
    let matcher = BackgroundRectangle::around(
        [&target],
        Vec2::new(finite_f32("buff.x", buff_x)?, finite_f32("buff.y", buff_y)?),
        finite_f32("corner_radius", corner_radius)?,
        BLACK,
        finite_f32("fill_opacity", fill_opacity)?,
    )
    .map_err(|error| error.to_string())?;
    encode_snapshot(matcher.into_snapshot())
}

pub fn manim_cross_snapshot_json(
    target_snapshot_json: Option<&str>,
    stroke_width: f64,
    scale_factor: f64,
) -> Result<String, String> {
    let target = target_snapshot_json.map(decode_target).transpose()?;
    let matcher = Cross::with_options(
        target.as_ref(),
        RED,
        finite_f32("stroke_width", stroke_width)?,
        finite_f32("scale_factor", scale_factor)?,
    )
    .map_err(|error| error.to_string())?;
    encode_snapshot(matcher.into_snapshot())
}

pub fn manim_underline_snapshot_json(
    target_snapshot_json: &str,
    buff: f64,
) -> Result<String, String> {
    let target = decode_target(target_snapshot_json)?;
    let matcher = Underline::with_buff(&target, finite_f32("buff", buff)?)
        .map_err(|error| error.to_string())?;
    encode_snapshot(matcher.into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        manim_background_rectangle_snapshot_json, manim_cross_snapshot_json,
        manim_surrounding_rectangle_snapshot_json, manim_underline_snapshot_json,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen(js_name = manimSurroundingRectangleSnapshotJson)]
    pub fn manim_surrounding_rectangle_snapshot(
        target_snapshot_json: &str,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<String, JsValue> {
        manim_surrounding_rectangle_snapshot_json(
            target_snapshot_json,
            buff_x,
            buff_y,
            corner_radius,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimBackgroundRectangleSnapshotJson)]
    pub fn manim_background_rectangle_snapshot(
        target_snapshot_json: &str,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
        fill_opacity: f64,
    ) -> Result<String, JsValue> {
        manim_background_rectangle_snapshot_json(
            target_snapshot_json,
            buff_x,
            buff_y,
            corner_radius,
            fill_opacity,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimCrossSnapshotJson)]
    pub fn manim_cross_snapshot(
        target_snapshot_json: Option<String>,
        stroke_width: f64,
        scale_factor: f64,
    ) -> Result<String, JsValue> {
        manim_cross_snapshot_json(
            target_snapshot_json.as_deref(),
            stroke_width,
            scale_factor,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimUnderlineSnapshotJson)]
    pub fn manim_underline_snapshot(
        target_snapshot_json: &str,
        buff: f64,
    ) -> Result<String, JsValue> {
        manim_underline_snapshot_json(target_snapshot_json, buff).map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon::{IntoSnapshot, Rectangle};
    use noon_core::{GeometryRef, Vec2, BLACK, RED, SMALL_BUFF};

    use super::*;

    fn target_json() -> String {
        serde_json::to_string(
            &Rectangle::new(4.0, 2.0)
                .shift(Vec2::new(1.0, -2.0))
                .into_snapshot(),
        )
        .expect("valid target snapshot")
    }

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid matcher snapshot")
    }

    #[test]
    fn surrounding_rectangle_bridge_uses_shared_target_bounds() {
        let snapshot = decode(
            &manim_surrounding_rectangle_snapshot_json(&target_json(), 0.25, 0.5, 0.1)
                .expect("valid matcher"),
        );
        assert_eq!(snapshot.center(), Vec2::new(1.0, -2.0));
        assert!((snapshot.width() - 4.5).abs() <= 1e-5);
        assert!((snapshot.height() - 3.0).abs() <= 1e-5);
        assert_eq!(snapshot.style.stroke, Some(SURROUNDING_RECTANGLE_DEFAULT_COLOR));
        assert!(matches!(snapshot.geometry, GeometryRef::VectorPath(_)));
    }

    #[test]
    fn background_rectangle_bridge_preserves_shared_default_style() {
        let snapshot = decode(
            &manim_background_rectangle_snapshot_json(
                &target_json(),
                0.0,
                0.0,
                0.0,
                f64::from(BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY),
            )
            .expect("valid background"),
        );
        let fill = snapshot.style.fill.expect("background fill");
        assert_eq!((fill.red, fill.green, fill.blue), (BLACK.red, BLACK.green, BLACK.blue));
        assert!((fill.alpha - BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY).abs() <= 1e-5);
        assert_eq!(snapshot.style.stroke_width, 0.0);
    }

    #[test]
    fn cross_bridge_handles_targeted_and_default_geometry() {
        let targeted = decode(
            &manim_cross_snapshot_json(
                Some(&target_json()),
                f64::from(DEFAULT_CROSS_STROKE_WIDTH),
                1.5,
            )
            .expect("valid targeted cross"),
        );
        let bounds = targeted.world_bounds().expect("cross bounds");
        assert_eq!(bounds.center(), Vec2::new(1.0, -2.0));
        assert!((bounds.width() - 6.0).abs() <= 1e-5);
        assert!((bounds.height() - 3.0).abs() <= 1e-5);
        assert_eq!(targeted.style.stroke, Some(RED));

        let default = decode(
            &manim_cross_snapshot_json(
                None,
                f64::from(DEFAULT_CROSS_STROKE_WIDTH),
                f64::from(DEFAULT_CROSS_SCALE_FACTOR),
            )
            .expect("valid default cross"),
        );
        let bounds = default.world_bounds().expect("default cross bounds");
        assert!((bounds.width() - 2.0).abs() <= 1e-5);
        assert!((bounds.height() - 2.0).abs() <= 1e-5);
    }

    #[test]
    fn underline_bridge_uses_shared_line_matcher_semantics() {
        let snapshot = decode(
            &manim_underline_snapshot_json(&target_json(), f64::from(DEFAULT_UNDERLINE_BUFF))
                .expect("valid underline"),
        );
        let GeometryRef::Line { start, end } = snapshot.geometry else {
            panic!("underline must remain retained analytic line geometry")
        };
        assert_eq!(start, Vec2::new(-1.0, -3.0 - SMALL_BUFF));
        assert_eq!(end, Vec2::new(3.0, -3.0 - SMALL_BUFF));
    }

    #[test]
    fn matcher_bridge_rejects_malformed_and_non_finite_inputs() {
        assert!(manim_underline_snapshot_json("not json", 0.1).is_err());
        assert!(manim_surrounding_rectangle_snapshot_json(
            &target_json(),
            f64::NAN,
            0.1,
            0.0,
        )
        .is_err());
        assert!(manim_cross_snapshot_json(None, f64::INFINITY, 1.0).is_err());
    }
}

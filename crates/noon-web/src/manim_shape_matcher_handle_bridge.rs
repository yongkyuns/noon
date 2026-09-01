use noon::{
    BackgroundRectangle, IntoSnapshot, SurroundingRectangle, SURROUNDING_RECTANGLE_DEFAULT_COLOR,
};
use noon_core::{Rect, Vec2, BLACK};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn rect_from_center_size(
    center_x: f64,
    center_y: f64,
    width: f64,
    height: f64,
) -> Result<Rect, String> {
    if !width.is_finite() || width < 0.0 {
        return Err("family layout width must be finite and non-negative".to_owned());
    }
    if !height.is_finite() || height < 0.0 {
        return Err("family layout height must be finite and non-negative".to_owned());
    }
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    Ok(Rect::new(
        Vec2::new(
            finite_f32("family bounds min.x", center_x - half_width)?,
            finite_f32("family bounds min.y", center_y - half_height)?,
        ),
        Vec2::new(
            finite_f32("family bounds max.x", center_x + half_width)?,
            finite_f32("family bounds max.y", center_y + half_height)?,
        ),
    ))
}

fn encode_snapshot(snapshot: noon_core::ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize shape matcher snapshot: {error}"))
}

fn surrounding_from_bounds_json(
    bounds: Rect,
    buff_x: f64,
    buff_y: f64,
    corner_radius: f64,
) -> Result<String, String> {
    SurroundingRectangle::around_world_bounds(
        bounds,
        Vec2::new(finite_f32("buff.x", buff_x)?, finite_f32("buff.y", buff_y)?),
        finite_f32("corner_radius", corner_radius)?,
        SURROUNDING_RECTANGLE_DEFAULT_COLOR,
    )
    .map(IntoSnapshot::into_snapshot)
    .map_err(|error| error.to_string())
    .and_then(encode_snapshot)
}

fn background_from_bounds_json(
    bounds: Rect,
    buff_x: f64,
    buff_y: f64,
    corner_radius: f64,
    fill_opacity: f64,
) -> Result<String, String> {
    BackgroundRectangle::around_world_bounds(
        bounds,
        Vec2::new(finite_f32("buff.x", buff_x)?, finite_f32("buff.y", buff_y)?),
        finite_f32("corner_radius", corner_radius)?,
        BLACK,
        finite_f32("fill_opacity", fill_opacity)?,
    )
    .map(IntoSnapshot::into_snapshot)
    .map_err(|error| error.to_string())
    .and_then(encode_snapshot)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::Rect;
    use wasm_bindgen::prelude::*;

    use crate::{WasmAuthoringFamilyLayout, WasmAuthoringMobjectHandle};

    use super::{background_from_bounds_json, rect_from_center_size, surrounding_from_bounds_json};

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    fn family_bounds(layout: &WasmAuthoringFamilyLayout) -> Result<Rect, JsValue> {
        rect_from_center_size(
            layout.center_x()?,
            layout.center_y()?,
            layout.width()?,
            layout.height()?,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = surroundingRectangleSnapshotJson)]
        pub fn surrounding_rectangle_snapshot_json(
            &self,
            buff_x: f64,
            buff_y: f64,
            corner_radius: f64,
        ) -> Result<String, JsValue> {
            crate::manim_shape_matcher_bridge::manim_surrounding_rectangle_snapshot_json(
                &self.snapshot_json()?,
                buff_x,
                buff_y,
                corner_radius,
            )
            .map_err(js_error)
        }

        #[wasm_bindgen(js_name = backgroundRectangleSnapshotJson)]
        pub fn background_rectangle_snapshot_json(
            &self,
            buff_x: f64,
            buff_y: f64,
            corner_radius: f64,
            fill_opacity: f64,
        ) -> Result<String, JsValue> {
            crate::manim_shape_matcher_bridge::manim_background_rectangle_snapshot_json(
                &self.snapshot_json()?,
                buff_x,
                buff_y,
                corner_radius,
                fill_opacity,
            )
            .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
        #[wasm_bindgen(js_name = surroundingRectangleSnapshotJson)]
        pub fn surrounding_rectangle_snapshot_json(
            &self,
            buff_x: f64,
            buff_y: f64,
            corner_radius: f64,
        ) -> Result<String, JsValue> {
            surrounding_from_bounds_json(family_bounds(self)?, buff_x, buff_y, corner_radius)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = backgroundRectangleSnapshotJson)]
        pub fn background_rectangle_snapshot_json(
            &self,
            buff_x: f64,
            buff_y: f64,
            corner_radius: f64,
            fill_opacity: f64,
        ) -> Result<String, JsValue> {
            background_from_bounds_json(
                family_bounds(self)?,
                buff_x,
                buff_y,
                corner_radius,
                fill_opacity,
            )
            .map_err(js_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use noon::BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY;
    use noon_core::{ObjectSnapshot, Vec2};

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid matcher snapshot")
    }

    #[test]
    fn family_center_size_lowers_to_exact_world_bounds() {
        assert_eq!(
            rect_from_center_size(1.0, -2.0, 8.0, 6.0).unwrap(),
            Rect::new(Vec2::new(-3.0, -5.0), Vec2::new(5.0, 1.0))
        );
    }

    #[test]
    fn family_bounds_matcher_keeps_geometry_and_style_in_shared_rust() {
        let bounds = rect_from_center_size(1.0, -2.0, 8.0, 6.0).unwrap();
        let surrounding = decode(&surrounding_from_bounds_json(bounds, 0.25, 0.5, 0.1).unwrap());
        let center = surrounding.center();
        assert!((center.x - 1.0).abs() <= 1e-5);
        assert!((center.y + 2.0).abs() <= 1e-5);
        assert!((surrounding.width() - 8.5).abs() <= 1e-5);
        assert!((surrounding.height() - 7.0).abs() <= 1e-5);
        assert_eq!(
            surrounding.style.stroke,
            Some(SURROUNDING_RECTANGLE_DEFAULT_COLOR)
        );

        let background = decode(
            &background_from_bounds_json(
                bounds,
                0.0,
                0.0,
                0.0,
                f64::from(BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY),
            )
            .unwrap(),
        );
        assert_eq!(background.style.stroke_width, 0.0);
        assert!(
            (background.style.fill.unwrap().alpha - BACKGROUND_RECTANGLE_DEFAULT_FILL_OPACITY)
                .abs()
                <= 1e-5
        );
    }

    #[test]
    fn invalid_family_extent_fails_closed() {
        assert!(rect_from_center_size(0.0, 0.0, -1.0, 1.0).is_err());
        assert!(rect_from_center_size(0.0, 0.0, 1.0, f64::NAN).is_err());
        assert!(rect_from_center_size(f64::MAX, 0.0, 1.0, 1.0).is_err());
    }
}

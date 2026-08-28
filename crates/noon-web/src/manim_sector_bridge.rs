use noon::{AnnularSector, Annulus, IntoSnapshot, Sector};
use noon_core::{ObjectSnapshot, Vec2, WHITE};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn point(name: &str, x: f64, y: f64) -> Result<Vec2, String> {
    Ok(Vec2::new(
        finite_f32(&format!("{name}.x"), x)?,
        finite_f32(&format!("{name}.y"), y)?,
    ))
}

fn component_count(value: u32) -> Result<usize, String> {
    let value = value as usize;
    if value < 2 {
        return Err(format!("num_components must be at least 2, got {value}"));
    }
    Ok(value)
}

fn snapshot_json(snapshot: ObjectSnapshot) -> Result<String, String> {
    serde_json::to_string(&snapshot)
        .map_err(|error| format!("unable to serialize Manim sector snapshot: {error}"))
}

#[allow(clippy::too_many_arguments)]
pub fn manim_annular_sector_snapshot_json(
    inner_radius: f64,
    outer_radius: f64,
    angle: f64,
    start_angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<String, String> {
    let sector = AnnularSector::with_options(
        finite_f32("inner_radius", inner_radius)?,
        finite_f32("outer_radius", outer_radius)?,
        finite_f32("angle", angle)?,
        finite_f32("start_angle", start_angle)?,
        1.0,
        0.0,
        WHITE,
        component_count(num_components)?,
        point("arc_center", center_x, center_y)?,
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(sector.into_snapshot())
}

pub fn manim_sector_snapshot_json(
    radius: f64,
    angle: f64,
    start_angle: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<String, String> {
    let sector = Sector::with_options(
        finite_f32("radius", radius)?,
        finite_f32("angle", angle)?,
        finite_f32("start_angle", start_angle)?,
        1.0,
        0.0,
        WHITE,
        component_count(num_components)?,
        point("arc_center", center_x, center_y)?,
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(sector.into_snapshot())
}

pub fn manim_annulus_snapshot_json(
    inner_radius: f64,
    outer_radius: f64,
    num_components: u32,
    center_x: f64,
    center_y: f64,
) -> Result<String, String> {
    let annulus = Annulus::with_options(
        finite_f32("inner_radius", inner_radius)?,
        finite_f32("outer_radius", outer_radius)?,
        1.0,
        0.0,
        WHITE,
        component_count(num_components)?,
        point("arc_center", center_x, center_y)?,
    )
    .map_err(|error| error.to_string())?;
    snapshot_json(annulus.into_snapshot())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        manim_annular_sector_snapshot_json, manim_annulus_snapshot_json,
        manim_sector_snapshot_json,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen(js_name = manimAnnularSectorSnapshotJson)]
    #[allow(clippy::too_many_arguments)]
    pub fn manim_annular_sector_snapshot(
        inner_radius: f64,
        outer_radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<String, JsValue> {
        manim_annular_sector_snapshot_json(
            inner_radius,
            outer_radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimSectorSnapshotJson)]
    pub fn manim_sector_snapshot(
        radius: f64,
        angle: f64,
        start_angle: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<String, JsValue> {
        manim_sector_snapshot_json(
            radius,
            angle,
            start_angle,
            num_components,
            center_x,
            center_y,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = manimAnnulusSnapshotJson)]
    pub fn manim_annulus_snapshot(
        inner_radius: f64,
        outer_radius: f64,
        num_components: u32,
        center_x: f64,
        center_y: f64,
    ) -> Result<String, JsValue> {
        manim_annulus_snapshot_json(
            inner_radius,
            outer_radius,
            num_components,
            center_x,
            center_y,
        )
        .map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, PathCommand, StrokeWidthMode};

    use super::*;

    fn decode(value: &str) -> ObjectSnapshot {
        serde_json::from_str(value).expect("valid ObjectSnapshot JSON")
    }

    fn path(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        let GeometryRef::VectorPath(path) = &snapshot.geometry else {
            panic!("sector bridge must preserve retained VectorPath geometry")
        };
        path.commands()
    }

    #[test]
    fn annular_sector_bridge_keeps_shared_defaults_and_contour() {
        let snapshot = decode(
            &manim_annular_sector_snapshot_json(1.0, 2.0, std::f64::consts::FRAC_PI_2, 0.0, 9, 0.0, 0.0)
                .expect("valid annular sector"),
        );
        assert_eq!(path(&snapshot).len(), 20);
        assert_eq!(path(&snapshot).last(), Some(&PathCommand::Close));
        assert_eq!(snapshot.style.fill, Some(WHITE));
        assert_eq!(snapshot.style.stroke, Some(WHITE));
        assert_eq!(snapshot.style.stroke_width, 0.0);
        assert_eq!(snapshot.style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
    }

    #[test]
    fn sector_bridge_preserves_zero_radius_inner_contour() {
        let snapshot = decode(
            &manim_sector_snapshot_json(2.0, std::f64::consts::FRAC_PI_2, 0.0, 9, 1.0, -2.0)
                .expect("valid sector"),
        );
        let commands = path(&snapshot);
        assert_eq!(commands.len(), 20);
        assert!(matches!(commands.first(), Some(PathCommand::MoveTo { to }) if *to == Vec2::new(1.0, -2.0)));
    }

    #[test]
    fn annulus_bridge_keeps_two_oppositely_wound_closed_contours() {
        let snapshot = decode(
            &manim_annulus_snapshot_json(0.5, 2.0, 9, 0.0, 0.0).expect("valid annulus"),
        );
        let commands = path(&snapshot);
        assert_eq!(commands.iter().filter(|command| matches!(command, PathCommand::MoveTo { .. })).count(), 2);
        assert_eq!(commands.iter().filter(|command| matches!(command, PathCommand::Close)).count(), 2);
    }

    #[test]
    fn sector_bridge_rejects_non_renderable_inputs() {
        assert!(manim_sector_snapshot_json(f64::NAN, 1.0, 0.0, 9, 0.0, 0.0).is_err());
        assert!(manim_annulus_snapshot_json(1.0, 2.0, 1, 0.0, 0.0).is_err());
    }
}

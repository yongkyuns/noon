//! Read-only fields for the existing browser diagnostic/export boundary (#959).
//! No engine layer consumes this codec as an authoring or runtime authority.
use noon::Mobject;
use noon_core::{Color, GeometryRef, GeometryResource, StoredGeometry, Style, Transform2D, Vec2};
#[cfg(any(target_arch = "wasm32", test))]
use serde::Serialize;

pub(crate) fn mobject_fields(
    object: &Mobject,
) -> Result<(GeometryRef, Transform2D, Style), String> {
    let state = object.state()?;
    let geometry = match state
        .content
        .geometry()
        .ok_or("geometry mobject required")?
    {
        StoredGeometry::Circle { radius } => GeometryRef::circle(radius),
        StoredGeometry::Rectangle { size } => GeometryRef::Rectangle { size },
        StoredGeometry::Line { start, end } => GeometryRef::Line { start, end },
        StoredGeometry::Resource(handle) => match object
            .store()
            .borrow()
            .geometry_resources()
            .get(handle)
            .ok_or("unknown or stale geometry resource")?
        {
            GeometryResource::VectorPath(path) => GeometryRef::path((**path).clone()),
        },
    };
    // These synchronous observations use the existing shared scalar projections.
    // They cannot interleave with another mutation of this single-threaded store.
    let translation = object.wire_translation()?;
    let scale = object.wire_scale()?;
    let transform = Transform2D {
        translation: Vec2::new(translation.0 as f32, translation.1 as f32),
        scale: Vec2::new(scale.0 as f32, scale.1 as f32),
        rotation: object.wire_rotation()? as f32,
    };
    let color =
        |(r, g, b, a): (f64, f64, f64, f64)| Color::rgba(r as f32, g as f32, b as f32, a as f32);
    let style = Style {
        fill: object.wire_fill()?.map(color),
        stroke: object.wire_stroke()?.map(color),
        stroke_width: object.wire_stroke_width()? as f32,
        stroke_width_mode: state.style.stroke_width_mode,
        stroke_join: state.style.stroke_join,
        stroke_cap: state.style.stroke_cap,
        opacity: object.wire_object_opacity()? as f32,
    };
    Ok((geometry, transform, style))
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn mobject_json(object: &Mobject) -> Result<String, String> {
    // Borrowed serialization view only: it cannot retain or mutate object state.
    // Serialize the f32 fields directly, preserving the existing external JSON.
    #[derive(Serialize)]
    struct Export<'a> {
        geometry: &'a GeometryRef,
        transform: &'a Transform2D,
        style: &'a Style,
    }
    let (geometry, transform, style) = mobject_fields(object)?;
    serde_json::to_string(&Export {
        geometry: &geometry,
        transform: &transform,
        style: &style,
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_handle_export_is_rejected() {
        let scene = noon::Scene::new();
        let object = scene.circle(1.0).unwrap();
        scene
            .store()
            .borrow_mut()
            .remove_node(object.node_id())
            .unwrap();
        assert!(mobject_fields(&object).is_err());
        assert!(mobject_json(&object).is_err());
    }

    #[test]
    fn export_preserves_paint_layers_and_authored_state() {
        let scene = noon::Scene::new();
        let mut object = scene.circle(0.4).unwrap();
        object.set_translation(0.7, -0.3).unwrap();
        object.set_fill(0.2, 0.3, 0.4, 0.25).unwrap();
        object.set_object_opacity(0.5).unwrap();
        let before = object.state().unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&mobject_json(&object).unwrap()).unwrap();
        assert_eq!(value["style"]["fill"]["alpha"], 0.25);
        assert_eq!(value["style"]["opacity"], 0.5);
        assert_eq!(object.state().unwrap(), before);
        object.disable_fill().unwrap();
        object.disable_stroke().unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&mobject_json(&object).unwrap()).unwrap();
        assert!(value["style"]["fill"].is_null());
        assert!(value["style"]["stroke"].is_null());
    }
}

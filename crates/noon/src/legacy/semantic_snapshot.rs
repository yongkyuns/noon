//! Explicit legacy value export; deletion owned by #959.
use crate::semantic_mobject::{solid_color_with_opacity, Mobject};
use noon_core::{
    GeometryRef, GeometryResource, ObjectSnapshot, SemanticStore, SemanticStyle,
    SemanticTransform2_5D, StoredGeometry, Style, Transform2D,
};

pub fn export_mobject_snapshot(object: &Mobject) -> Result<ObjectSnapshot, String> {
    let state = object.state()?;
    Ok(ObjectSnapshot {
        geometry: export_geometry(
            &object.store().borrow(),
            state
                .content
                .geometry()
                .ok_or("geometry mobject required")?,
        )?,
        transform: export_transform(state.transform)?,
        style: export_style(&state.style),
    })
}

fn export_style(style: &SemanticStyle) -> Style {
    Style {
        fill: solid_color_with_opacity(style.fill.as_ref(), style.fill_opacity),
        stroke: solid_color_with_opacity(style.stroke.as_ref(), style.stroke_opacity),
        stroke_width: style.stroke_width as f32,
        stroke_width_mode: style.stroke_width_mode,
        stroke_join: style.stroke_join,
        stroke_cap: style.stroke_cap,
        opacity: style.object_opacity as f32,
    }
}
fn export_geometry(store: &SemanticStore, geometry: StoredGeometry) -> Result<GeometryRef, String> {
    Ok(match geometry {
        StoredGeometry::Circle { radius } => GeometryRef::circle(radius),
        StoredGeometry::Rectangle { size } => GeometryRef::Rectangle { size },
        StoredGeometry::Line { start, end } => GeometryRef::Line { start, end },
        StoredGeometry::Resource(handle) => match store
            .geometry_resources()
            .get(handle)
            .ok_or("unknown or stale geometry resource")?
        {
            GeometryResource::VectorPath(path) => GeometryRef::path((**path).clone()),
        },
    })
}

fn export_transform(transform: SemanticTransform2_5D) -> Result<Transform2D, String> {
    Ok(Transform2D {
        translation: transform
            .translation
            .lower_xy_f32()
            .map_err(|e| e.to_string())?,
        scale: transform.scale.lower_xy_f32().map_err(|e| e.to_string())?,
        rotation: crate::semantic_mobject::authoring_render_f64("rotation", transform.rotation_z)?
            as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_handle_export_is_rejected() {
        let scene = crate::Scene::new();
        let object = scene.circle(1.0).unwrap();
        scene
            .store()
            .borrow_mut()
            .remove_node(object.node_id())
            .unwrap();
        assert!(export_mobject_snapshot(&object).is_err());
    }
}

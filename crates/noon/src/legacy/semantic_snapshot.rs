//! Explicit legacy value import/export; deletion owned by #959.
use crate::semantic_mobject::{import_geometry, legacy_solid_color, Mobject};
use noon_core::{
    GeometryRef, GeometryResource, ObjectSnapshot, SemanticObjectState, SemanticPaint,
    SemanticStore, SemanticStyle, SemanticTransform2_5D, SemanticVec3, StoredGeometry, Style,
    Transform2D,
};
use std::{cell::RefCell, rc::Rc};
pub fn import_mobject_snapshot(
    store: Rc<RefCell<SemanticStore>>,
    snapshot: ObjectSnapshot,
) -> Result<Mobject, String> {
    validate_snapshot(&snapshot)?;
    let mut state =
        SemanticObjectState::new(import_geometry(&mut store.borrow_mut(), snapshot.geometry)?);
    state.transform = SemanticTransform2_5D {
        translation: SemanticVec3::from_vec2(snapshot.transform.translation),
        scale: SemanticVec3::new(
            snapshot.transform.scale.x as f64,
            snapshot.transform.scale.y as f64,
            1.0,
        ),
        rotation_z: snapshot.transform.rotation as f64,
    };
    state.style = authoring_style_from_legacy(snapshot.style);
    Mobject::new(store, state)
}
pub fn replace_mobject_snapshot(
    object: &mut Mobject,
    snapshot: ObjectSnapshot,
) -> Result<(), String> {
    validate_snapshot(&snapshot)?;
    let mut state = object.state()?;
    let same_geometry = geometry_matches(
        &object.store().borrow(),
        state.content.geometry().ok_or("geometry required")?,
        &snapshot.geometry,
    )?;
    if same_geometry
        && snapshot.transform == export_transform(state.transform)?
        && snapshot.style == export_style(&state.style)
    {
        return Ok(());
    }
    if !same_geometry {
        state.content =
            import_geometry(&mut object.store().borrow_mut(), snapshot.geometry)?.into();
    }
    state.transform = SemanticTransform2_5D {
        translation: SemanticVec3::from_vec2(snapshot.transform.translation),
        scale: SemanticVec3::new(
            snapshot.transform.scale.x as f64,
            snapshot.transform.scale.y as f64,
            1.0,
        ),
        rotation_z: snapshot.transform.rotation as f64,
    };
    state.style = authoring_style_from_legacy(snapshot.style);
    object.commit_state(state)
}
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

fn authoring_style_from_legacy(style: Style) -> SemanticStyle {
    let mut semantic = SemanticStyle::from_legacy(style);
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.fill {
        semantic.fill_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.stroke {
        semantic.stroke_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    semantic
}
fn export_style(style: &SemanticStyle) -> Style {
    Style {
        fill: legacy_solid_color(style.fill.as_ref(), style.fill_opacity),
        stroke: legacy_solid_color(style.stroke.as_ref(), style.stroke_opacity),
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

fn geometry_matches(
    store: &SemanticStore,
    current: StoredGeometry,
    incoming: &GeometryRef,
) -> Result<bool, String> {
    Ok(match (current, incoming) {
        (StoredGeometry::Circle { radius: a }, GeometryRef::Circle { radius: b }) => a == *b,
        (StoredGeometry::Rectangle { size: a }, GeometryRef::Rectangle { size: b }) => a == *b,
        (StoredGeometry::Line { start: a, end: b }, GeometryRef::Line { start: c, end: d }) => {
            a == *c && b == *d
        }
        (StoredGeometry::Resource(handle), GeometryRef::VectorPath(path)) => match store
            .geometry_resources()
            .get(handle)
            .ok_or("unknown or stale geometry resource")?
        {
            GeometryResource::VectorPath(existing) => existing.as_ref() == path,
        },
        _ => false,
    })
}

fn validate_snapshot(snapshot: &ObjectSnapshot) -> Result<(), String> {
    let t = snapshot.transform;
    if !snapshot.geometry.is_finite()
        || !authoring_style_from_legacy(snapshot.style).is_finite()
        || ![
            t.translation.x,
            t.translation.y,
            t.scale.x,
            t.scale.y,
            t.rotation,
        ]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err("snapshot geometry, transform and style must be finite".into());
    }
    Ok(())
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
    use noon_core::{Vec2, VectorPath};
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

    #[test]
    fn repeated_identical_path_import_preserves_resource_and_revision() {
        let scene = crate::Scene::new();
        let path = VectorPath::new()
            .move_to(Vec2::ZERO)
            .line_to(Vec2::new(2.0, 1.0));
        let mut object = scene.path(path, SemanticStyle::default()).unwrap();
        object.shift(0.10000000001, 0.0).unwrap();
        object.set_fill_opacity(0.10000000001).unwrap();
        let semantic_before = object.state().unwrap();
        let snapshot = export_mobject_snapshot(&object).unwrap();
        let content = object.state().unwrap().content;
        let before = scene.store().borrow().scene_revision();
        let resources = scene.store().borrow().geometry_resources().len();
        for _ in 0..8 {
            replace_mobject_snapshot(&mut object, snapshot.clone()).unwrap();
        }
        assert_eq!(object.state().unwrap(), semantic_before);
        assert_eq!(object.state().unwrap().content, content);
        assert_eq!(scene.store().borrow().scene_revision(), before);
        assert_eq!(scene.store().borrow().geometry_resources().len(), resources);
    }
}

//! Typed compact-value import at explicit external authoring boundaries.

use noon_core::{
    GeometryRef, SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle,
    SemanticTransform2_5D, SemanticVec3, Style, Transform2D,
};

/// Validate compact geometry/presentation values and convert them into one semantic state.
///
/// This conversion allocates heavy geometry in the supplied semantic resource arena, but
/// creates no semantic identity or membership. Callers remain responsible for publishing
/// the resulting state through the ordinary semantic transaction vocabulary.
pub fn semantic_object_state_from_compact(
    store: &mut SemanticStore,
    geometry: GeometryRef,
    transform: Transform2D,
    style: Style,
) -> Result<SemanticObjectState, String> {
    if !geometry.is_finite() {
        return Err("compact geometry must be finite".into());
    }
    let transform = semantic_transform_from_compact(transform)?;
    let style = semantic_style_from_compact(style)?;
    let mut state =
        SemanticObjectState::new(crate::semantic_mobject::import_geometry(store, geometry)?);
    state.transform = transform;
    state.style = style;
    Ok(state)
}

pub(crate) fn semantic_transform_from_compact(
    transform: Transform2D,
) -> Result<SemanticTransform2_5D, String> {
    let semantic = SemanticTransform2_5D {
        translation: SemanticVec3::from_vec2(transform.translation),
        scale: SemanticVec3::new(
            f64::from(transform.scale.x),
            f64::from(transform.scale.y),
            1.0,
        ),
        rotation_z: f64::from(transform.rotation),
    };
    if !semantic.translation.is_finite()
        || !semantic.scale.is_finite()
        || !semantic.rotation_z.is_finite()
    {
        return Err("compact transform must be finite".into());
    }
    Ok(semantic)
}

pub(crate) fn semantic_style_from_compact(style: Style) -> Result<SemanticStyle, String> {
    let mut semantic = SemanticStyle::from_compact(style);
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.fill {
        semantic.fill_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.stroke {
        semantic.stroke_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    if !semantic.is_finite() {
        return Err("compact style must be finite".into());
    }
    Ok(semantic)
}

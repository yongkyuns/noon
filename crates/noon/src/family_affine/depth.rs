//! Depth pivots read canonical semantic controls; no projected geometry is retained.
use super::*;

pub(super) fn resolve_pivot(
    store: &SemanticStore,
    leaves: &[SemanticNodeId],
    pivot: ManimRotationPivot,
) -> Result<f64, AuthoringError> {
    let direction = match pivot {
        ManimRotationPivot::Point(x, y) => {
            crate::semantic_mobject::authoring_xy_f64(x, y)?;
            return Ok(0.0);
        }
        ManimRotationPivot::Point3d(point) => {
            if !point.is_finite() {
                return Err(AuthoringError::NonFiniteObjectState);
            }
            return Ok(point.z);
        }
        ManimRotationPivot::Edge3d(edge) => {
            if !edge.is_finite() {
                return Err(AuthoringError::NonFiniteObjectState);
            }
            edge.z
        }
        ManimRotationPivot::Edge(x, y) => {
            crate::semantic_mobject::authoring_xy_f64(x, y)?;
            0.0
        }
        ManimRotationPivot::Center => 0.0,
    };
    let mut bounds: Option<(f64, f64)> = None;
    for &leaf in leaves {
        let state = store.semantic_object_state_checked(leaf)?;
        let transform = state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        if let Some((min, max)) =
            crate::semantic_mobject::depth_for_content(store, state.content, transform, false)?
        {
            bounds = Some(bounds.map_or((min, max), |(old_min, old_max)| {
                (old_min.min(min), old_max.max(max))
            }));
        }
    }
    Ok(bounds.map_or(0.0, |(min, max)| {
        if direction < 0.0 {
            min
        } else if direction > 0.0 {
            max
        } else {
            (min + max) * 0.5
        }
    }))
}

#[cfg(test)]
mod tests;

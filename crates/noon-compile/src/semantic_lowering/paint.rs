//! Finite renderer-independent admission for continuous local fills.
use super::{SemanticExecutionField, SemanticExecutionValueError, SemanticLoweringError};
use noon_core::{
    GeometryRef, GeometryResource, LinearGradient, SemanticNodeId, SemanticObjectContent,
    SemanticObjectRole, SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry,
};
use std::sync::Arc;

#[cfg(test)]
mod tests;

pub(crate) fn validate_definition(
    gradient: &LinearGradient,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    if gradient.stops().len() > 16 {
        return Err(invalid());
    }
    let start = gradient.start().map(|v| v as f32);
    let end = gradient.end().map(|v| v as f32);
    let extent = (end[0] - start[0]).abs().max((end[1] - start[1]).abs());
    if extent < f32::MIN_POSITIVE {
        return Err(invalid());
    }
    if gradient.stops().windows(2).any(|stops| {
        stops[0].position() != stops[1].position()
            && stops[0].position() as f32 == stops[1].position() as f32
    }) {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn lower_fill(
    node: SemanticNodeId,
    state: &SemanticObjectState,
    store: &SemanticStore,
) -> Result<Option<Arc<LinearGradient>>, SemanticLoweringError> {
    let Some(SemanticPaint::LinearGradient(gradient)) = state.style.fill.as_ref() else {
        return Ok(None);
    };
    let invalid = || SemanticLoweringError::UnsupportedPaintProfile {
        node,
        field: SemanticExecutionField::FillPaint,
    };
    validate_definition(gradient).map_err(|error| error.with_node(node))?;
    if super::projection::object_requires_spatial_lowering(state, store)
        || state.role() != SemanticObjectRole::Ordinary
    {
        return Err(invalid());
    }
    let bounds = match state.content {
        SemanticObjectContent::Geometry(StoredGeometry::Circle { radius }) => {
            GeometryRef::circle(radius).local_bounds()
        }
        SemanticObjectContent::Geometry(StoredGeometry::Rectangle { size }) => {
            GeometryRef::rectangle(size.x, size.y).local_bounds()
        }
        SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
            match store.geometry_resources().get(handle) {
                Some(GeometryResource::VectorPath(path)) => path.conservative_bounds(),
                _ => None,
            }
        }
        _ => None,
    }
    .ok_or_else(invalid)?;
    let extent = bounds.max - bounds.min;
    if !extent.x.is_finite() || !extent.y.is_finite() || extent.x <= 0.0 || extent.y <= 0.0 {
        return Err(invalid());
    }
    Ok(Some(Arc::clone(gradient)))
}

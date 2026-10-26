//! Finite renderer-independent admission for continuous local fills.
use super::{SemanticExecutionField, SemanticExecutionValueError, SemanticLoweringError};
use noon_core::{
    GeometryRef, GeometryResource, LinearGradient, SemanticNodeId, SemanticObjectContent,
    SemanticObjectRole, SemanticObjectState, SemanticPaint, SemanticStore, StoredGeometry,
};
use std::sync::Arc;

#[cfg(test)]
mod tests;

pub(crate) fn validate_gradient(
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

pub(crate) fn validate_definition(
    paint: &SemanticPaint,
) -> Result<(), SemanticExecutionValueError> {
    match paint {
        SemanticPaint::LinearGradient(gradient) => validate_gradient(gradient),
        SemanticPaint::Solid(_) | SemanticPaint::Resource(_) => {
            Err(SemanticExecutionValueError::UnsupportedPaintProfile {
                field: SemanticExecutionField::FillPaint,
            })
        }
    }
}

pub(crate) fn validate_transition(
    transition: &noon_core::PaintTransition,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    let [from, to] = transition.gradients();
    validate_gradient(from)?;
    validate_gradient(to)?;
    if from.start() != to.start() || from.end() != to.end() {
        let coordinate_scale = from
            .start()
            .into_iter()
            .chain(from.end())
            .chain(to.start())
            .chain(to.end())
            .map(f64::abs)
            .fold(0.0, f64::max);
        // Two rounded endpoints can differ from their f64 values by half an
        // ULP per coordinate. Bound the entire axis sweep, not sampled times.
        if transition.minimum_axis_length() <= 4.0 * f64::from(f32::EPSILON) * coordinate_scale {
            return Err(invalid());
        }
    }
    for (a, b) in from.stops().windows(2).zip(to.stops().windows(2)) {
        if a[0].position() == b[0].position() && a[1].position() == b[1].position() {
            continue;
        }
        let gaps = [
            a[1].position() - a[0].position(),
            b[1].position() - b[0].position(),
        ];
        if gaps == [0.0, 0.0] {
            continue;
        }
        let coordinate_scale = a[1].position().max(b[1].position());
        if gaps[0].min(gaps[1]) <= 2.0 * f64::from(f32::EPSILON) * coordinate_scale {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn lower_fill(
    node: SemanticNodeId,
    state: &SemanticObjectState,
    store: &SemanticStore,
) -> Result<Option<Arc<SemanticPaint>>, SemanticLoweringError> {
    let Some(SemanticPaint::LinearGradient(gradient)) = state.style.fill.as_ref() else {
        return Ok(None);
    };
    let invalid = || SemanticLoweringError::UnsupportedPaintProfile {
        node,
        field: SemanticExecutionField::FillPaint,
    };
    validate_gradient(gradient).map_err(|error| error.with_node(node))?;
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
    Ok(Some(Arc::new(SemanticPaint::LinearGradient(Arc::clone(
        gradient,
    )))))
}

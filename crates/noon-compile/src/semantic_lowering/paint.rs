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

pub(crate) fn validate_transition(
    transition: &noon_core::PaintTransition,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    if let Some(images) = transition.images() {
        for image in images {
            validate_image_definition(image)?;
        }
        return Ok(());
    }
    let [from, to] = transition
        .gradients()
        .expect("validated gradient transition");
    validate_definition(from)?;
    validate_definition(to)?;
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

pub(crate) fn validate_image_definition(
    image: &noon_core::ImagePaint,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    let transform = image.mapping().transform();
    if !transform
        .translation()
        .into_iter()
        .chain(transform.scale())
        .chain([transform.rotation()])
        .all(|value| (value as f32).is_finite())
        || transform.scale().into_iter().any(|value| {
            let value = value as f32;
            value == 0.0 || !value.recip().is_finite()
        })
    {
        return Err(invalid());
    }
    Ok(())
}

pub fn lower_image_fill(
    node: SemanticNodeId,
    state: &SemanticObjectState,
    store: &SemanticStore,
) -> Result<Option<Arc<noon_core::ImagePaint>>, SemanticLoweringError> {
    let Some(SemanticPaint::Image(image)) = state.style.fill.as_ref() else {
        return Ok(None);
    };
    let invalid = || SemanticLoweringError::UnsupportedPaintProfile {
        node,
        field: SemanticExecutionField::FillPaint,
    };
    validate_image_definition(image).map_err(|error| error.with_node(node))?;
    if super::projection::object_requires_spatial_lowering(state, store)
        || state.role() != SemanticObjectRole::Ordinary
        || !state.signal_bindings().is_empty()
        || store
            .node(node)
            .is_some_and(|node| matches!(node.kind(), noon_core::SemanticNodeKind::Family(_)))
        || (store.node(node).is_some()
            && !store
                .animation_effect_snapshot(node)
                .map_err(SemanticLoweringError::from)?
                .is_empty())
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
    let resource = store
        .raster_image_resources()
        .get(image.resource())
        .ok_or_else(invalid)?;
    validate_image_bounds(image, bounds, [resource.width(), resource.height()])
        .map_err(|error| error.with_node(node))?;
    Ok(Some(image.clone()))
}

pub(crate) fn validate_image_bounds(
    image: &noon_core::ImagePaint,
    bounds: noon_core::Rect,
    dimensions: [u32; 2],
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    validate_image_definition(image)?;
    let extent = bounds.max - bounds.min;
    if !bounds.min.x.is_finite()
        || !bounds.min.y.is_finite()
        || !extent.x.is_finite()
        || !extent.y.is_finite()
        || extent.x <= 0.0
        || extent.y <= 0.0
        || dimensions.contains(&0)
    {
        return Err(invalid());
    }
    let size = [f64::from(extent.x), f64::from(extent.y)];
    for point in [[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0], [0.5, 0.5]] {
        if image
            .mapping()
            .image_uv(point, size, dimensions)
            .map_err(|_| invalid())?
            .is_some_and(|uv| uv.into_iter().any(|value| !(value as f32).is_finite()))
        {
            return Err(invalid());
        }
    }
    let ratio = (dimensions[0] as f32 / dimensions[1] as f32) * (extent.y / extent.x);
    if ratio <= 0.0 || !ratio.is_finite() || !ratio.recip().is_finite() {
        return Err(invalid());
    }
    if !extent.x.recip().is_finite() || !extent.y.recip().is_finite() {
        return Err(invalid());
    }
    // Bound intermediate shader coordinates even when contain clips every
    // checked corner. Rotation cannot increase this conservative L1 bound.
    let transform = image.mapping().transform();
    let delta = 1.0 + transform.translation()[0].abs() + transform.translation()[1].abs();
    let inverse_scale = transform
        .scale()
        .into_iter()
        .map(|value| value.abs().recip())
        .fold(0.0, f64::max);
    let fit = f64::from(ratio.max(ratio.recip()).max(1.0));
    let texel_bound =
        (delta * inverse_scale + 1.0) * fit * f64::from(dimensions[0].max(dimensions[1]));
    if !texel_bound.is_finite() || texel_bound > f64::from(f32::MAX) / 8.0 {
        return Err(invalid());
    }
    Ok(())
}

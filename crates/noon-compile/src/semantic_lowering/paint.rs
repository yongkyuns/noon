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

pub(crate) fn validate_radial(
    gradient: &noon_core::RadialGradient,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    if gradient.stops().len() > 16
        || gradient
            .radii()
            .into_iter()
            .any(|radius| !(radius as f32).is_finite() || (radius as f32) < 4.0 * f32::MIN_POSITIVE)
        || gradient.stops().windows(2).any(|stops| {
            stops[0].position() != stops[1].position()
                && stops[0].position() as f32 == stops[1].position() as f32
        })
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn validate_definition(
    paint: &SemanticPaint,
) -> Result<(), SemanticExecutionValueError> {
    match paint {
        SemanticPaint::LinearGradient(gradient) => validate_gradient(gradient),
        SemanticPaint::RadialGradient(gradient) => validate_radial(gradient),
        SemanticPaint::Noise(noise) => validate_noise(noise),
        SemanticPaint::Hatch(hatch) => project_hatch_mapping(hatch).map(|_| ()),
        SemanticPaint::Image(image) => validate_image(image),
        SemanticPaint::Solid(_) => Err(SemanticExecutionValueError::UnsupportedPaintProfile {
            field: SemanticExecutionField::FillPaint,
        }),
    }
}

pub(crate) fn validate_noise(
    noise: &noon_core::NoisePaint,
) -> Result<(), SemanticExecutionValueError> {
    if noise
        .frequency()
        .into_iter()
        .any(|v| v > 256.0 || (v as f32) < f32::MIN_POSITIVE)
    {
        return Err(SemanticExecutionValueError::UnsupportedPaintProfile {
            field: SemanticExecutionField::FillPaint,
        });
    }
    Ok(())
}

/// Finite GPU projection of object-local hatch coordinates. Authored phase and
/// angle remain f64 until this derived renderer input is prepared.
/// Rows contain normal/spacing, periodic phase, then duty (row zero only).
pub fn project_hatch_mapping(
    hatch: &noon_core::HatchPaint,
) -> Result<[[f32; 4]; 2], SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    let inverse = hatch.spacing().recip();
    let duty = hatch.width() / hatch.spacing();
    if !(inverse as f32).is_finite() || (inverse as f32) < 4.0 * f32::MIN_POSITIVE {
        return Err(invalid());
    }
    let (s, c) = hatch.angle().sin_cos();
    let phase = hatch.phase_wrapped();
    let rows = [
        [-s * inverse, c * inverse, phase[0], duty],
        [c * inverse, s * inverse, phase[1], 0.0],
    ];
    if rows.iter().flatten().any(|v| !(*v as f32).is_finite()) {
        return Err(invalid());
    }
    Ok(rows.map(|row| row.map(|v| v as f32)))
}

/// A rotation-independent cell bound also covers intermediate animated angles.
/// Spacing varies convexly, so endpoint minimum spacing bounds its whole sweep.
pub fn validate_hatch_bounds(
    hatch: &noon_core::HatchPaint,
    bounds: noon_core::Rect,
) -> Result<(), SemanticExecutionValueError> {
    project_hatch_mapping(hatch)?;
    let x = f64::from(bounds.min.x.abs().max(bounds.max.x.abs()));
    let y = f64::from(bounds.min.y.abs().max(bounds.max.y.abs()));
    if [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
        .into_iter()
        .any(|value| !value.is_finite())
        || (x + y) / hatch.spacing() > 65_536.0
    {
        return Err(SemanticExecutionValueError::UnsupportedPaintProfile {
            field: SemanticExecutionField::FillPaint,
        });
    }
    Ok(())
}

pub(crate) fn validate_image(
    image: &noon_core::ImagePaint,
) -> Result<(), SemanticExecutionValueError> {
    let transform = image.mapping().transform();
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    if transform
        .translation()
        .into_iter()
        .chain(transform.scale())
        .any(|value| !(value as f32).is_finite())
        || transform.scale().into_iter().any(|value| {
            (value as f32).abs() < f32::MIN_POSITIVE || !(1.0 / value as f32).is_finite()
        })
    {
        return Err(invalid());
    }
    Ok(())
}

/// Finite execution projection of the shared f64 image mapping. The norm
/// margin covers every rotation, including an animated sweep between endpoints;
/// it also bounds translated coordinates before narrowing to shader arithmetic.
/// This does not promise alias-free minification or arbitrary f64 UV precision.
pub fn project_image_mapping(
    mapping: noon_core::ImageMapping,
    local_size: [f64; 2],
    image_size: [u32; 2],
) -> Result<[[f32; 3]; 2], SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    let rows = mapping
        .image_uv_affine(local_size, image_size)
        .map_err(|_| invalid())?;
    let translation = mapping.transform().translation();
    let displacement = 2.0 + translation[0].abs() + translation[1].abs();
    if rows.iter().enumerate().any(|(axis, row)| {
        let norm = row[0].hypot(row[1]);
        // Sampling converts UVs to texel coordinates before reconstructing the
        // filter. Bound that product too, including a rotation-independent
        // displacement margin and the fitted alignment offset.
        let texel_bound =
            (2.0 * norm * displacement + row[2].abs() + 2.0) * f64::from(image_size[axis]);
        norm < 4.0 * f64::from(f32::MIN_POSITIVE)
            || texel_bound > f64::from(f32::MAX) / 4.0
            || row.iter().any(|v| !(*v as f32).is_finite())
    }) {
        return Err(invalid());
    }
    Ok(rows.map(|row| row.map(|v| v as f32)))
}

pub(crate) fn validate_transition(
    transition: &noon_core::PaintTransition,
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    if let Some([from, to]) = transition.images() {
        validate_image(from)?;
        validate_image(to)?;
        return Ok(());
    }
    if let Some([from, to]) = transition.radials() {
        validate_radial(from)?;
        validate_radial(to)?;
        return validate_stop_sweep(from.stops(), to.stops());
    }
    if let Some([from, to]) = transition.noises() {
        validate_noise(from)?;
        validate_noise(to)?;
        return Ok(());
    }
    if let Some([from, to]) = transition.hatches() {
        project_hatch_mapping(from)?;
        project_hatch_mapping(to)?;
        return Ok(());
    }
    let [from, to] = transition
        .gradients()
        .expect("closed checked gradient/image transition");
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
        if transition
            .minimum_axis_length()
            .expect("gradient transition")
            <= 4.0 * f64::from(f32::EPSILON) * coordinate_scale
        {
            return Err(invalid());
        }
    }
    validate_stop_sweep(from.stops(), to.stops())
}

fn validate_stop_sweep(
    from: &[noon_core::GradientStop],
    to: &[noon_core::GradientStop],
) -> Result<(), SemanticExecutionValueError> {
    let invalid = || SemanticExecutionValueError::UnsupportedPaintProfile {
        field: SemanticExecutionField::FillPaint,
    };
    for (a, b) in from.windows(2).zip(to.windows(2)) {
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
    let Some(
        paint @ (SemanticPaint::LinearGradient(_)
        | SemanticPaint::RadialGradient(_)
        | SemanticPaint::Noise(_)
        | SemanticPaint::Hatch(_)
        | SemanticPaint::Image(_)),
    ) = state.style.fill.as_ref()
    else {
        return Ok(None);
    };
    let invalid = || SemanticLoweringError::UnsupportedPaintProfile {
        node,
        field: SemanticExecutionField::FillPaint,
    };
    validate_definition(paint).map_err(|error| error.with_node(node))?;
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
    if let SemanticPaint::Image(image) = paint {
        let resource = store
            .raster_image_resources()
            .get(image.resource())
            .ok_or_else(invalid)?;
        project_image_mapping(
            image.mapping(),
            [f64::from(extent.x), f64::from(extent.y)],
            [resource.width(), resource.height()],
        )
        .map_err(|_| invalid())?;
    }
    if let SemanticPaint::Hatch(hatch) = paint {
        validate_hatch_bounds(hatch, bounds).map_err(|_| invalid())?;
    }
    Ok(Some(Arc::new(paint.clone())))
}

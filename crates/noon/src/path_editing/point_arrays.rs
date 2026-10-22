//! Raw XYZ inputs share the ordinary path resource/publication transaction.
use super::*;

pub(crate) fn promote_xyz(points: &[(f64, f64)]) -> Vec<SemanticVec3> {
    points
        .iter()
        .map(|&(x, y)| SemanticVec3::new(x, y, 0.0))
        .collect()
}

pub(super) fn validate(points: &[SemanticVec3]) -> Result<(), AuthoringError> {
    if points.iter().any(|point| point.lower_xy_f32().is_err()) {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    Ok(())
}

pub(crate) fn raw_path(points: &[SemanticVec3]) -> Result<VectorPath, AuthoringError> {
    validate(points)?;
    Ok(VectorPath::from_manim_points_3d(points.to_vec()))
}

pub(super) fn points(path: &VectorPath) -> Result<Vec<SemanticVec3>, AuthoringError> {
    match path.manim_points() {
        Some(points) => Ok(points.to_vec()),
        None => crate::PathQuery::prepare(path, Default::default())?.points_3d(),
    }
}

// A command-only path has implicit zero-depth controls. Capture those controls
// before baking a nontrivial Z pose, otherwise a subsequent raw-point edit
// cannot recover the authored depth from its planar drawing commands.
pub(super) fn with_canonical_points(path: &VectorPath) -> Result<VectorPath, AuthoringError> {
    let mut result = if path.manim_points().is_some() {
        path.clone()
    } else {
        VectorPath::from_manim_points_3d(points(path)?)
    };
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(with_canonical_points(target)?);
    }
    Ok(result)
}

pub(super) fn append(
    suffix: &[SemanticVec3],
    current: impl FnOnce() -> Result<VectorPath, AuthoringError>,
) -> Result<Option<VectorPath>, AuthoringError> {
    validate(suffix)?;
    if suffix.is_empty() {
        return Ok(None);
    }
    // Capture only after caller-input validation, preserving fail-closed order.
    let path = current()?;
    let mut raw = points(&path)?;
    raw.extend_from_slice(suffix);
    let mut result = VectorPath::from_manim_points_3d(raw);
    if let Some(target) = path.morph_target() {
        result = result.with_morph_target(target.clone());
    }
    Ok(Some(result))
}

impl Mobject {
    /// Replace the complete world-coordinate array, retaining all three f64 axes.
    pub fn set_points_3d(&mut self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::RawPoints3d(points))
    }

    /// Append all three coordinates through the same atomic path publication.
    pub fn append_points_3d(&mut self, points: &[SemanticVec3]) -> Result<(), AuthoringError> {
        self.edit_path(PathEdit::AppendRawPoints3d(points))
    }
}

impl crate::Scene {
    pub fn set_points_3d(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::RawPoints3d(points),
            )
        })
    }

    pub fn append_points_3d(
        &mut self,
        object: &Mobject,
        points: &[SemanticVec3],
    ) -> Result<(), AuthoringError> {
        self.with_running_execution(|store, root, execution| {
            publish_running_object_edit(
                store,
                root,
                execution,
                object,
                PathEdit::AppendRawPoints3d(points),
            )
        })
    }
}

#[cfg(test)]
mod tests;

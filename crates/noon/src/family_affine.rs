//! Atomic affine edits over the authoritative family's unique semantic leaves.
mod depth;
#[cfg(test)]
mod scaling_tests;

use crate::AuthoringError;
use crate::{
    family_layout::bounds_critical_point,
    semantic_mobject::{authoring_render_f64, rotate_affine_about_point, stage_state_changes},
    LayoutAnchor, ManimRotationPivot, Mobject, MobjectFamily, SemanticVec3,
};
use noon_core::{Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticStore};

#[derive(Clone, Copy)]
pub(crate) enum FamilyAffine {
    Scale(f64, f64, ManimRotationPivot),
    Scale3d(SemanticVec3, ManimRotationPivot),
    StretchDepth(f64, ManimRotationPivot),
    Rotate(f64, ManimRotationPivot),
    Flip(SemanticVec3, ManimRotationPivot),
}

impl FamilyAffine {
    fn scale_factors(self) -> Option<(SemanticVec3, ManimRotationPivot)> {
        match self {
            Self::Scale(x, y, pivot) => Some((SemanticVec3::new(x, y, 1.0), pivot)),
            Self::Scale3d(factors, pivot) => Some((factors, pivot)),
            Self::StretchDepth(z, pivot) => Some((SemanticVec3::new(1.0, 1.0, z), pivot)),
            _ => None,
        }
    }

    pub(crate) fn needs_xy_bounds(self) -> bool {
        if let Some((factors, pivot)) = self.scale_factors() {
            return (matches!(self, Self::Scale(..)) || factors.x != 1.0 || factors.y != 1.0)
                && !matches!(
                    pivot,
                    ManimRotationPivot::Point(..) | ManimRotationPivot::Point3d(..)
                );
        }
        !matches!(
            self,
            Self::Rotate(
                _,
                ManimRotationPivot::Point(..) | ManimRotationPivot::Point3d(..)
            )
        )
    }

    /// Prepare one affine edit for an addressed root.  A Table's spacing is an
    /// authored declaration on its own family root, so only a scale addressed
    /// at that root changes its buffers. Scaling an enclosing family still uses
    /// ordinary leaf affine behavior.
    pub(crate) fn prepare_for_scope(
        self,
        store: &SemanticStore,
        leaves: &[SemanticNodeId],
        bounds: Option<Bounds2D64>,
        scope: Option<SemanticNodeId>,
    ) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
        let depth_pivot = if let Self::Scale3d(_, pivot) | Self::StretchDepth(_, pivot) = self {
            depth::resolve_pivot(store, leaves, pivot)?
        } else {
            0.0
        };
        let mut affine_leaves = Vec::new();
        let mut replacements = Vec::new();
        for &leaf in leaves {
            let state = store.semantic_object_state_checked(leaf)?;
            if let Some((factors, pivot)) = self.scale_factors() {
                factors.lower_xy_f32().map_err(AuthoringError::from)?;
                let (x, y) = (factors.x, factors.y);
                authoring_render_f64("scale.x", x)?;
                authoring_render_f64("scale.y", y)?;
                if crate::dimension_fit::world_scale_factors(
                    state
                        .transform
                        .planar_rotation()
                        .ok_or(AuthoringError::NonFiniteObjectState)?,
                    x,
                    y,
                )
                .is_err()
                {
                    let pivot =
                        resolve_pivot(bounds, bounds_critical_point(bounds, 0.0, 0.0), pivot)?;
                    let (baked, path) = world_scaled_replacement(store, state, x, y, pivot, pivot)?;
                    let path = path.transformed_semantic(noon_core::SemanticTransform2_5D {
                        translation: SemanticVec3::new(0.0, 0.0, depth_pivot * (1.0 - factors.z)),
                        scale: SemanticVec3::new(1.0, 1.0, factors.z),
                        rotation_z: 0.0,
                    });
                    replacements.push((leaf, baked, path));
                    continue;
                }
            }
            affine_leaves.push(leaf);
        }
        let transaction = self.transaction(store, &affine_leaves, bounds, scope, depth_pivot)?;
        Ok(
            crate::path_editing::PreparedPathEdits::prepare(store, replacements)?
                .with_transaction(transaction),
        )
    }

    fn transaction(
        self,
        store: &SemanticStore,
        leaves: &[SemanticNodeId],
        bounds: Option<Bounds2D64>,
        scope: Option<SemanticNodeId>,
        depth_pivot: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let center = bounds_critical_point(bounds, 0.0, 0.0);
        if let Self::Flip(axis, _) = self {
            for (name, value) in [
                ("flip axis.x", axis.x),
                ("flip axis.y", axis.y),
                ("flip axis.z", axis.z),
            ] {
                authoring_render_f64(name, value)?;
            }
            if (axis.x == 0.0 && axis.y == 0.0 && axis.z == 0.0)
                || (axis.z != 0.0 && (axis.x != 0.0 || axis.y != 0.0))
            {
                return Err(AuthoringError::InvalidFlipAxis);
            }
        }
        let pivot = match self {
            Self::StretchDepth(..) => (0.0, 0.0),
            Self::Scale3d(factors, _) if factors.x == 1.0 && factors.y == 1.0 => (0.0, 0.0),
            Self::Scale(_, _, pivot) | Self::Scale3d(_, pivot) => {
                let (factors, _) = self.scale_factors().expect("scale variant");
                let (x, y) = (factors.x, factors.y);
                authoring_render_f64("family scale.x", x)?;
                authoring_render_f64("family scale.y", y)?;
                resolve_pivot(bounds, center, pivot)?
            }
            Self::Rotate(_, pivot) | Self::Flip(_, pivot) => {
                if let Self::Rotate(angle, _) = self {
                    authoring_render_f64("family rotation", angle)?;
                }
                resolve_pivot(bounds, center, pivot)?
            }
        };
        let mut transaction = SemanticMutationTransaction::new();
        for &leaf in leaves {
            let previous = store
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?;
            let mut next = previous.clone();
            match self {
                Self::Scale(..) | Self::Scale3d(..) | Self::StretchDepth(..) => {
                    let (factors, _) = self.scale_factors().expect("scale variant");
                    factors.lower_xy_f32().map_err(AuthoringError::from)?;
                    let (local_x, local_y) = crate::dimension_fit::world_scale_factors(
                        previous
                            .transform
                            .planar_rotation()
                            .ok_or(AuthoringError::NonFiniteObjectState)?,
                        factors.x,
                        factors.y,
                    )?;
                    next.transform.scale.x *= local_x;
                    next.transform.scale.y *= local_y;
                    next.transform.scale.z *= factors.z;
                    next.transform.translation.x =
                        (previous.transform.translation.x - pivot.0) * factors.x + pivot.0;
                    next.transform.translation.y =
                        (previous.transform.translation.y - pivot.1) * factors.y + pivot.1;
                    next.transform.translation.z =
                        (previous.transform.translation.z - depth_pivot) * factors.z + depth_pivot;
                    next.transform
                        .scale
                        .lower_xy_f32()
                        .map_err(AuthoringError::from)?;
                    next.transform
                        .translation
                        .lower_xy_f32()
                        .map_err(AuthoringError::from)?;
                }
                Self::Flip(axis, _) if axis.z == 0.0 => {
                    let norm = axis.x.hypot(axis.y);
                    let (x, y) = (axis.x / norm, axis.y / norm);
                    let (c, s) = (x * x - y * y, 2.0 * x * y);
                    let (dx, dy) = (
                        previous.transform.translation.x - pivot.0,
                        previous.transform.translation.y - pivot.1,
                    );
                    next.transform.translation.x = pivot.0 + c * dx + s * dy;
                    next.transform.translation.y = pivot.1 + s * dx - c * dy;
                    // F(axis) R(theta) D(sx, sy) = R(2*axis-theta) D(sx, -sy).
                    next.transform.orientation = noon_core::SemanticOrientation::Planar(
                        2.0 * y.atan2(x)
                            - previous
                                .transform
                                .planar_rotation()
                                .ok_or(AuthoringError::NonFiniteObjectState)?,
                    );
                    next.transform.scale.y = -previous.transform.scale.y;
                    next.transform
                        .translation
                        .lower_xy_f32()
                        .map_err(AuthoringError::from)?;
                    authoring_render_f64(
                        "flip rotation result",
                        next.transform
                            .planar_rotation()
                            .ok_or(AuthoringError::NonFiniteObjectState)?,
                    )?;
                }
                Self::Rotate(_, _) | Self::Flip(_, _) => {
                    let angle = match self {
                        Self::Rotate(angle, _) => angle,
                        _ => std::f64::consts::PI,
                    };
                    let (translation, rotation) = rotate_affine_about_point(
                        (
                            previous.transform.translation.x,
                            previous.transform.translation.y,
                        ),
                        previous
                            .transform
                            .planar_rotation()
                            .ok_or(AuthoringError::NonFiniteObjectState)?,
                        angle,
                        pivot,
                    )?;
                    next.transform.translation.x = translation.0;
                    next.transform.translation.y = translation.1;
                    next.transform.orientation = noon_core::SemanticOrientation::Planar(rotation);
                }
            }
            stage_state_changes(&mut transaction, leaf, previous, &next);
        }
        if let (Some((factors, _)), Some(scope)) = (self.scale_factors(), scope) {
            let (x, y) = (factors.x, factors.y);
            if matches!(
                store.node(scope).map(|node| node.kind()),
                Some(noon_core::SemanticNodeKind::Family(_))
            ) {
                if let Some(layout) = store
                    .semantic_table_layout(scope)
                    .map_err(AuthoringError::from)?
                {
                    transaction.set_table_layout(scope, layout.scaled_buffers(x, y));
                }
            }
        }
        Ok(transaction)
    }
}

/// Bake a world-axis deformation only when the ordinary affine cannot express it.
/// Object-scaled stroke outlines need a full affine representation; retain the
/// explicit unsupported result instead of changing their visible thickness.
pub(crate) fn world_scaled_replacement(
    store: &SemanticStore,
    state: &noon_core::SemanticObjectState,
    x: f64,
    y: f64,
    source: (f64, f64),
    destination: (f64, f64),
) -> Result<(noon_core::SemanticObjectState, noon_core::VectorPath), AuthoringError> {
    if state.style.stroke.is_some()
        && state.style.stroke_width != 0.0
        && state.style.stroke_width_mode == noon_core::StrokeWidthMode::ScaleWithObject
    {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::RotatedDimensionStretch,
        ));
    }
    let transform = noon_core::SemanticTransform2_5D {
        translation: SemanticVec3::new(
            authoring_render_f64("stretch translation.x", destination.0 - source.0 * x)?,
            authoring_render_f64("stretch translation.y", destination.1 - source.1 * y)?,
            0.0,
        ),
        scale: SemanticVec3::new(
            authoring_render_f64("stretch scale.x", x)?,
            authoring_render_f64("stretch scale.y", y)?,
            1.0,
        ),
        rotation_z: 0.0,
    };
    // Capture semantic cubic controls before applying an f64 world transform,
    // including when the source has only planar drawing commands.
    let path =
        crate::path_editing::canonical_world_path(store, state)?.transformed_semantic(transform);
    if !path.is_finite() {
        return Err(AuthoringError::NonFiniteGeometry);
    }
    // Return the pose matching the baked XYZ resource, so no caller can
    // accidentally apply the original depth pose a second time.
    let mut replacement = crate::path_editing::path_replacement_state(state.clone())?;
    replacement.transform.translation.z = 0.0;
    replacement.transform.scale.z = 1.0;
    Ok((replacement, path))
}

fn resolve_pivot(
    bounds: Option<Bounds2D64>,
    center: (f64, f64),
    pivot: ManimRotationPivot,
) -> Result<(f64, f64), AuthoringError> {
    match pivot {
        ManimRotationPivot::Point3d(point) | ManimRotationPivot::Edge3d(point) => {
            if !point.is_finite() {
                return Err(AuthoringError::NonFiniteObjectState);
            }
            resolve_pivot(
                bounds,
                center,
                match pivot {
                    ManimRotationPivot::Point3d(_) => ManimRotationPivot::Point(point.x, point.y),
                    _ => ManimRotationPivot::Edge(point.x, point.y),
                },
            )
        }
        ManimRotationPivot::Center => Ok(center),
        ManimRotationPivot::Point(x, y) => Ok((
            authoring_render_f64("pivot.x", x)?,
            authoring_render_f64("pivot.y", y)?,
        )),
        ManimRotationPivot::Edge(x, y) => {
            authoring_render_f64("edge.x", x)?;
            authoring_render_f64("edge.y", y)?;
            Ok(bounds_critical_point(bounds, x, y))
        }
    }
}

impl MobjectFamily {
    /// Scale each unique semantic leaf about the family center in one transaction.
    pub fn scale(&self, x: f64, y: f64) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).apply_affine_with_table_layout(
            FamilyAffine::Scale(x, y, ManimRotationPivot::Center),
            Some(self.node_id()),
        )
    }

    /// Rotate each unique leaf about a shared center, edge or explicit point.
    pub fn rotate(&self, angle: f64, pivot: ManimRotationPivot) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Rotate(angle, pivot))
    }

    /// Reflect about a world-space axis through the chosen family pivot.
    /// Supports axes in the XY plane and a half-turn about the Z axis.
    pub fn flip(
        &self,
        axis: SemanticVec3,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).flip(axis, pivot)
    }

    fn apply_affine(&self, operation: FamilyAffine) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).apply_affine(operation)
    }
}

impl LayoutAnchor {
    /// Scale selected unique leaves about one shared world-space pivot.
    pub fn scale(&self, x: f64, y: f64, pivot: ManimRotationPivot) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Scale(x, y, pivot))
    }

    /// Scale all three semantic coordinates around one shared family pivot.
    /// Scalar Manim scaling uses the same factor on each axis; axis factors
    /// also reuse the ordinary atomic affine and world-baking paths.
    pub fn scale_3d(
        &self,
        factors: SemanticVec3,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Scale3d(factors, pivot))
    }

    /// Rotate all selected leaves around one shared center, edge or explicit point.
    pub fn rotate(&self, angle: f64, pivot: ManimRotationPivot) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Rotate(angle, pivot))
    }

    /// Reflect the selected leaf/family in the XY plane, retaining geometry resources.
    pub fn flip(
        &self,
        axis: SemanticVec3,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Flip(axis, pivot))
    }

    fn apply_affine(&self, operation: FamilyAffine) -> Result<(), AuthoringError> {
        let scope = self.resolve()?;
        self.apply_affine_with_table_layout(operation, Some(scope))
    }

    pub(crate) fn apply_affine_without_table_layout(
        &self,
        operation: FamilyAffine,
    ) -> Result<(), AuthoringError> {
        self.apply_affine_with_table_layout(operation, None)
    }

    /// Direct family and anchor scales update Table buffers, including with a
    /// pivot. Stretch operations explicitly opt out.
    pub(crate) fn apply_affine_with_table_layout(
        &self,
        operation: FamilyAffine,
        table_layout_scope: Option<SemanticNodeId>,
    ) -> Result<(), AuthoringError> {
        let (leaves, bounds) = if operation.needs_xy_bounds() {
            let layout = self.layout()?;
            (layout.leaves().to_vec(), layout.boundary_bounds())
        } else {
            let root = self.resolve()?;
            (
                self.integration_store().borrow().ordered_leaf_nodes(root)?,
                None,
            )
        };
        let prepared = operation.prepare_for_scope(
            &self.integration_store().borrow(),
            &leaves,
            bounds,
            table_layout_scope,
        )?;
        prepared.publish(
            &mut self.integration_store().borrow_mut(),
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }
}

impl Mobject {
    /// Scale about an explicit world point through the shared affine transaction.
    pub fn manim_scale_about_point(
        &mut self,
        x: f64,
        y: f64,
        px: f64,
        py: f64,
    ) -> Result<(), AuthoringError> {
        LayoutAnchor::from(&*self).scale(x, y, ManimRotationPivot::Point(px, py))
    }
    /// Scale about the geometry critical point selected by a world direction.
    pub fn manim_scale_about_edge(
        &mut self,
        x: f64,
        y: f64,
        ex: f64,
        ey: f64,
    ) -> Result<(), AuthoringError> {
        LayoutAnchor::from(&*self).scale(x, y, ManimRotationPivot::Edge(ex, ey))
    }

    /// Apply a Manim-compatible rotation pivot using shared semantic bounds.
    pub fn rotate_with_pivot(
        &self,
        angle: f64,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).rotate(angle, pivot)
    }

    /// Reflect around a shared center/edge/point without copying path data.
    pub fn flip(
        &self,
        axis: SemanticVec3,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).flip(axis, pivot)
    }
}

//! Atomic affine edits over the authoritative family's unique semantic leaves.
use crate::AuthoringError;
use crate::{
    family_layout::bounds_critical_point,
    semantic_mobject::{
        authoring_render_f64, rotate_affine_about_point, scale_state_about_center,
        stage_state_changes, state_center,
    },
    LayoutAnchor, ManimRotationPivot, Mobject, MobjectFamily, SemanticVec3,
};
use noon_core::{Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticStore};

#[derive(Clone, Copy)]
pub(crate) enum FamilyAffine {
    Scale(f64, f64, ManimRotationPivot),
    Rotate(f64, ManimRotationPivot),
    Flip(SemanticVec3, ManimRotationPivot),
}

impl FamilyAffine {
    pub(crate) fn transaction(
        self,
        store: &SemanticStore,
        leaves: &[SemanticNodeId],
        bounds: Option<Bounds2D64>,
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
            Self::Scale(x, y, pivot) => {
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
                Self::Scale(x, y, _) => {
                    let (local_x, local_y) = crate::dimension_fit::world_scale_factors(
                        previous.transform.rotation_z,
                        x,
                        y,
                    )?;
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        pivot.0 + (old_center.0 - pivot.0) * x,
                        pivot.1 + (old_center.1 - pivot.1) * y,
                    );
                    scale_state_about_center(store, &mut next, local_x, local_y, target_center)?;
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
                    next.transform.rotation_z = 2.0 * y.atan2(x) - previous.transform.rotation_z;
                    next.transform.scale.y = -previous.transform.scale.y;
                    next.transform
                        .translation
                        .lower_xy_f32()
                        .map_err(AuthoringError::from)?;
                    authoring_render_f64("flip rotation result", next.transform.rotation_z)?;
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
                        previous.transform.rotation_z,
                        angle,
                        pivot,
                    )?;
                    next.transform.translation.x = translation.0;
                    next.transform.translation.y = translation.1;
                    next.transform.rotation_z = rotation;
                }
            }
            stage_state_changes(&mut transaction, leaf, previous, &next);
        }
        Ok(transaction)
    }
}

fn resolve_pivot(
    bounds: Option<Bounds2D64>,
    center: (f64, f64),
    pivot: ManimRotationPivot,
) -> Result<(f64, f64), AuthoringError> {
    match pivot {
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
        self.apply_affine(FamilyAffine::Scale(x, y, ManimRotationPivot::Center))
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
        let layout = self.layout()?;
        let transaction = operation.transaction(
            &self.integration_store().borrow(),
            layout.leaves(),
            layout.boundary_bounds(),
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
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

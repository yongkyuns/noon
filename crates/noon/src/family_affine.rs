//! Atomic affine edits over the authoritative family's unique semantic leaves.
use crate::AuthoringError;
use crate::{
    family_layout::bounds_critical_point,
    semantic_mobject::{
        authoring_render_f64, rotate_affine_about_point, scale_state_about_center,
        stage_state_changes, state_center,
    },
    ManimRotationPivot, MobjectFamily,
};
use noon_core::{Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticStore};

#[derive(Clone, Copy)]
pub(crate) enum FamilyAffine {
    Scale(f64, f64),
    Rotate(f64, ManimRotationPivot),
}

impl FamilyAffine {
    pub(crate) fn transaction(
        self,
        store: &SemanticStore,
        leaves: &[SemanticNodeId],
        bounds: Option<Bounds2D64>,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        let center = bounds_critical_point(bounds, 0.0, 0.0);
        let pivot = match self {
            Self::Scale(x, y) => {
                authoring_render_f64("family scale.x", x)?;
                authoring_render_f64("family scale.y", y)?;
                center
            }
            Self::Rotate(angle, pivot) => {
                authoring_render_f64("family rotation", angle)?;
                match pivot {
                    ManimRotationPivot::Center => center,
                    ManimRotationPivot::Point(x, y) => (
                        authoring_render_f64("family pivot.x", x)?,
                        authoring_render_f64("family pivot.y", y)?,
                    ),
                    ManimRotationPivot::Edge(x, y) => {
                        authoring_render_f64("family edge.x", x)?;
                        authoring_render_f64("family edge.y", y)?;
                        bounds_critical_point(bounds, x, y)
                    }
                }
            }
        };
        let mut transaction = SemanticMutationTransaction::new();
        for &leaf in leaves {
            let previous = store
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?;
            let mut next = previous.clone();
            match self {
                Self::Scale(x, y) => {
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        center.0 + (old_center.0 - center.0) * x,
                        center.1 + (old_center.1 - center.1) * y,
                    );
                    scale_state_about_center(store, &mut next, x, y, target_center)?;
                }
                Self::Rotate(angle, _) => {
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

impl MobjectFamily {
    /// Scale each unique semantic leaf about the family center in one transaction.
    pub fn scale(&self, x: f64, y: f64) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Scale(x, y))
    }

    /// Rotate each unique leaf about a shared center, edge or explicit point.
    pub fn rotate(&self, angle: f64, pivot: ManimRotationPivot) -> Result<(), AuthoringError> {
        self.apply_affine(FamilyAffine::Rotate(angle, pivot))
    }

    fn apply_affine(&self, operation: FamilyAffine) -> Result<(), AuthoringError> {
        let layout = self.layout()?;
        let transaction = {
            let store = self.integration_store().borrow();
            let leaves = store
                .ordered_leaf_nodes(self.node_id())
                .map_err(AuthoringError::from)?;
            operation.transaction(&store, &leaves, layout.bounds())?
        };
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

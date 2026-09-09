//! Atomic affine edits over the authoritative family's unique semantic leaves.
use crate::AuthoringError;
use crate::{
    family_layout::bounds_critical_point,
    semantic_mobject::{
        authoring_render_f64, rotate_affine_about_point, scale_state_about_center,
        stage_state_changes, state_center,
    },
    ManimRotationPivot, Mobject, MobjectFamily,
};
use noon_core::{Bounds2D64, SemanticMutationTransaction, SemanticNodeId, SemanticStore};

#[derive(Clone, Copy)]
pub(crate) enum FamilyAffine {
    Scale(f64, f64),
    ScaleAbout(f64, f64, ManimRotationPivot),
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
            Self::ScaleAbout(x, y, pivot) => {
                authoring_render_f64("scale.x", x)?;
                authoring_render_f64("scale.y", y)?;
                resolve_pivot(bounds, center, pivot)?
            }
            Self::Rotate(angle, pivot) => {
                authoring_render_f64("family rotation", angle)?;
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
                Self::Scale(x, y) | Self::ScaleAbout(x, y, _) => {
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        pivot.0 + (old_center.0 - pivot.0) * x,
                        pivot.1 + (old_center.1 - pivot.1) * y,
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

impl Mobject {
    /// Scale around one explicit Manim point in a single semantic transaction.
    pub fn manim_scale_about_point(
        &mut self,
        x: f64,
        y: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), AuthoringError> {
        self.apply_manim_scale_pivot(x, y, ManimRotationPivot::Point(point_x, point_y))
    }

    /// Scale around the current Manim critical point selected by an edge vector.
    pub fn manim_scale_about_edge(
        &mut self,
        x: f64,
        y: f64,
        edge_x: f64,
        edge_y: f64,
    ) -> Result<(), AuthoringError> {
        self.apply_manim_scale_pivot(x, y, ManimRotationPivot::Edge(edge_x, edge_y))
    }

    fn apply_manim_scale_pivot(
        &mut self,
        x: f64,
        y: f64,
        pivot: ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        self.validate()?;
        let state = self.state()?;
        let bounds = self.layout_bounds()?.or_else(|| {
            Some(Bounds2D64::point(
                state.transform.translation.x,
                state.transform.translation.y,
            ))
        });
        let transaction = FamilyAffine::ScaleAbout(x, y, pivot).transaction(
            &self.integration_store().borrow(),
            &[self.node_id()],
            bounds,
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
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

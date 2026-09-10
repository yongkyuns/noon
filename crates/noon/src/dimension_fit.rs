//! Dimension fitting uses shared bounds and the existing atomic affine edits.
use crate::AuthoringError;
use std::rc::Rc;

use crate::{semantic_mobject::authoring_render_f64, Bounds2D64, LayoutAnchor};

/// The supported planar layout dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutDimension {
    Width,
    Height,
}

impl TryFrom<u32> for LayoutDimension {
    type Error = AuthoringError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Width),
            1 => Ok(Self::Height),
            _ => Err(AuthoringError::InvalidDimension(value)),
        }
    }
}

impl LayoutDimension {
    pub(crate) fn length(self, bounds: Option<Bounds2D64>) -> f64 {
        bounds.map_or(0.0, |bounds| match self {
            Self::Width => bounds.width(),
            Self::Height => bounds.height(),
        })
    }

    pub(crate) fn scale(
        self,
        bounds: Option<Bounds2D64>,
        length: f64,
        stretch: bool,
    ) -> Result<Option<(f64, f64)>, AuthoringError> {
        let length = authoring_render_f64("fit length", length)?;
        let previous = self.length(bounds);
        if previous == 0.0 {
            return Ok(None);
        }
        let factor = authoring_render_f64("fit scale", length / previous)?;
        Ok(Some(match (stretch, self) {
            (false, _) => (factor, factor),
            (true, Self::Width) => (factor, 1.0),
            (true, Self::Height) => (1.0, factor),
        }))
    }
}

/// Convert a world-axis scale to local factors when no shear is required.
pub(crate) fn world_scale_factors(
    rotation: f64,
    x: f64,
    y: f64,
) -> Result<(f64, f64), AuthoringError> {
    if x == y || rotation.sin().abs() <= 1.0e-12 {
        return Ok((x, y));
    }
    if rotation.cos().abs() <= 1.0e-12 {
        return Ok((y, x));
    }
    Err(AuthoringError::Unsupported(
        crate::UnsupportedAuthoringOperation::RotatedDimensionStretch,
    ))
}

impl LayoutAnchor {
    /// Stretch one world dimension around a shared pivot, preserving semantic identity.
    pub fn stretch(
        &self,
        factor: f64,
        dimension: LayoutDimension,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        let (x, y) = match dimension {
            LayoutDimension::Width => (factor, 1.0),
            LayoutDimension::Height => (1.0, factor),
        };
        self.scale(x, y, pivot)
    }

    /// Fit the selected object or family through its ordinary affine semantics.
    /// A zero source extent is a no-op; families edit each unique leaf once.
    pub fn rescale_to_fit(
        &self,
        length: f64,
        dimension: LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
        self.rescale_to_fit_with_pivot(
            length,
            dimension,
            stretch,
            crate::ManimRotationPivot::Center,
        )
    }

    /// Fit a dimension while holding one shared center, edge, or explicit point.
    pub fn rescale_to_fit_with_pivot(
        &self,
        length: f64,
        dimension: LayoutDimension,
        stretch: bool,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        let layout = self.layout()?;
        let Some((x, y)) = dimension.scale(layout.bounds(), length, stretch)? else {
            return Ok(());
        };
        let prepared = {
            let store = self.integration_store().borrow();
            crate::family_affine::FamilyAffine::Scale(x, y, pivot).prepare(
                &store,
                layout.leaves(),
                layout.boundary_bounds(),
            )?
        };
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

    /// Match an object's or family's dimension using a fresh shared observation.
    pub fn match_dim_size(
        &self,
        target: &LayoutAnchor,
        dimension: LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
        self.match_dim_size_with_pivot(
            target,
            dimension,
            stretch,
            crate::ManimRotationPivot::Center,
        )
    }

    /// Match a fresh target extent and fit around the selected source pivot.
    pub fn match_dim_size_with_pivot(
        &self,
        target: &LayoutAnchor,
        dimension: LayoutDimension,
        stretch: bool,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), target.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let length = dimension.length(target.layout()?.bounds());
        self.rescale_to_fit_with_pivot(length, dimension, stretch, pivot)
    }
}

/// Prepare replacement from one coherent observation of the source and target.
/// Target leaves shared with the source observe the staged scale before the
/// final center shift, matching ordinary sequential replace semantics without
/// publishing an intermediate revision.
pub(crate) fn replacement_transaction(
    store: &noon_core::SemanticStore,
    leaves: &[crate::SemanticNodeId],
    source: (Option<Bounds2D64>, Option<Bounds2D64>),
    target_leaves: &[(
        crate::SemanticNodeId,
        Option<Bounds2D64>,
        Option<Bounds2D64>,
    )],
    dimension: LayoutDimension,
    stretch: bool,
) -> Result<crate::path_editing::PreparedPathEdits, AuthoringError> {
    use crate::semantic_mobject::{scale_state_about_center, stage_state_changes, state_center};
    use std::collections::BTreeSet;

    fn union(bounds: impl Iterator<Item = Bounds2D64>) -> Option<Bounds2D64> {
        bounds.reduce(|mut total, next| {
            total.include(next.min_x, next.min_y);
            total.include(next.max_x, next.max_y);
            total
        })
    }
    let (source_bounds, source_boundary) = source;
    let target_bounds = union(
        target_leaves
            .iter()
            .filter_map(|(_, dimensions, _)| *dimensions),
    );
    let (x, y) = if stretch {
        let x = LayoutDimension::Width
            .scale(
                source_bounds,
                LayoutDimension::Width.length(target_bounds),
                true,
            )?
            .map_or(1.0, |scale| scale.0);
        let y = LayoutDimension::Height
            .scale(
                source_bounds,
                LayoutDimension::Height.length(target_bounds),
                true,
            )?
            .map_or(1.0, |scale| scale.1);
        (x, y)
    } else {
        dimension
            .scale(source_bounds, dimension.length(target_bounds), false)?
            .unwrap_or((1.0, 1.0))
    };
    let center = crate::family_layout::bounds_critical_point(source_boundary, 0.0, 0.0);
    let sources: BTreeSet<_> = leaves.iter().copied().collect();
    let target_after_scale = union(target_leaves.iter().filter_map(|(leaf, _, bounds)| {
        bounds.map(|mut bounds| {
            if sources.contains(leaf) {
                bounds.min_x = center.0 + (bounds.min_x - center.0) * x;
                bounds.max_x = center.0 + (bounds.max_x - center.0) * x;
                bounds.min_y = center.1 + (bounds.min_y - center.1) * y;
                bounds.max_y = center.1 + (bounds.max_y - center.1) * y;
            }
            bounds
        })
    }));
    let destination = crate::family_layout::bounds_critical_point(target_after_scale, 0.0, 0.0);
    let mut transaction = noon_core::SemanticMutationTransaction::new();
    let mut replacements = Vec::new();
    for &leaf in leaves {
        let previous = store
            .semantic_object_state_checked(leaf)
            .map_err(AuthoringError::from)?;
        let Ok((local_x, local_y)) = world_scale_factors(previous.transform.rotation_z, x, y)
        else {
            let path = crate::family_affine::world_scaled_path(
                store,
                previous,
                x,
                y,
                center,
                destination,
            )?;
            replacements.push((leaf, previous.clone(), path));
            continue;
        };
        let old_center = state_center(store, previous)?;
        let next_center = (
            destination.0 + (old_center.0 - center.0) * x,
            destination.1 + (old_center.1 - center.1) * y,
        );
        let mut next = previous.clone();
        scale_state_about_center(store, &mut next, local_x, local_y, next_center)?;
        stage_state_changes(&mut transaction, leaf, previous, &next);
    }
    Ok(
        crate::path_editing::PreparedPathEdits::prepare(store, replacements)?
            .with_transaction(transaction),
    )
}

impl LayoutAnchor {
    /// Match another object's or family's size and center atomically.
    ///
    /// Reads authored bounds. A zero source extent keeps that dimension's scale;
    /// an empty source is a no-op. Shared target leaves observe the staged scale.
    /// Use `LiveSession::replace_layout` after execution starts to consume the
    /// coherent current target bounds and publish through the live session.
    pub fn replace_layout(
        &self,
        target: &LayoutAnchor,
        dimension: LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), target.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let source = self.layout()?;
        let target_layout = target.layout()?;
        if target_layout.bounds().is_none() {
            return Err(AuthoringError::MissingLayoutBounds(target.resolve()?));
        }
        let target_leaves = target_layout
            .leaves()
            .iter()
            .map(|&node| {
                let object = crate::Mobject::from_node(Rc::clone(self.integration_store()), node)?;
                Ok((node, object.layout_bounds()?, object.boundary_bounds()?))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let prepared = replacement_transaction(
            &self.integration_store().borrow(),
            source.leaves(),
            (source.bounds(), source.boundary_bounds()),
            &target_leaves,
            dimension,
            stretch,
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

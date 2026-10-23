//! Dimension fitting uses shared bounds and the existing atomic affine edits.
use crate::AuthoringError;
use std::rc::Rc;

use crate::{semantic_mobject::authoring_render_f64, Bounds2D64, LayoutAnchor};

/// World dimensions accepted by semantic layout and stretch operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutDimension {
    Width,
    Height,
    Depth,
}

impl TryFrom<u32> for LayoutDimension {
    type Error = AuthoringError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Width),
            1 => Ok(Self::Height),
            2 => Ok(Self::Depth),
            _ => Err(AuthoringError::InvalidDimension(value)),
        }
    }
}

impl LayoutDimension {
    pub(crate) fn length(self, bounds: Option<Bounds2D64>) -> f64 {
        bounds.map_or(0.0, |bounds| match self {
            Self::Width => bounds.width(),
            Self::Height => bounds.height(),
            Self::Depth => 0.0,
        })
    }

    pub(crate) fn scale(
        self,
        bounds: Option<Bounds2D64>,
        length: f64,
        stretch: bool,
    ) -> Result<Option<(f64, f64)>, AuthoringError> {
        let previous = self.length(bounds);
        if previous == 0.0 {
            return Ok(None);
        }
        let length = authoring_render_f64("fit length", length)?;
        let factor = authoring_render_f64("fit scale", length / previous)?;
        Ok(Some(match (stretch, self) {
            (true, Self::Width) => (factor, 1.0),
            (true, Self::Height) => (1.0, factor),
            _ => (factor, factor),
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
    /// Read the same layout extent used by fitting, including resource objects.
    pub fn dimension_length(&self, dimension: LayoutDimension) -> Result<f64, AuthoringError> {
        if dimension == LayoutDimension::Depth {
            let root = self.resolve()?;
            let store = self.integration_store().borrow();
            let leaves = store.ordered_leaf_nodes(root)?;
            return depth_length_for_leaves(&store, &leaves);
        }
        Ok(dimension.length(self.dimension_bounds()?))
    }

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
            LayoutDimension::Depth => {
                return self.apply_affine_without_table_layout(
                    crate::family_affine::FamilyAffine::StretchDepth(factor, pivot),
                );
            }
        };
        self.apply_affine_without_table_layout(crate::family_affine::FamilyAffine::Scale(
            x, y, pivot,
        ))
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
        let previous = self.dimension_length(dimension)?;
        let Some(factor) = fitting_factor(previous, length)? else {
            return Ok(());
        };
        let factors = fitting_factors(factor, dimension, stretch);
        let table_layout_scope = (!stretch).then(|| self.resolve()).transpose()?;
        self.apply_affine_with_table_layout(
            crate::family_affine::FamilyAffine::Scale3d(factors, pivot),
            table_layout_scope,
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
        let length = target.dimension_length(dimension)?;
        self.rescale_to_fit_with_pivot(length, dimension, stretch, pivot)
    }
}

/// Prepare replacement from one coherent observation of the source and target.
/// Target leaves shared with the source observe the staged scale before the
/// final center shift, matching ordinary sequential replace semantics without
/// publishing an intermediate revision.
/// An optional circle buffer adds the pinned diagonal fit after remeasuring
/// the replaced target.
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
    circle_buffer_factor: Option<f64>,
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
    if let Some(buffer) = circle_buffer_factor {
        authoring_render_f64("circle surround buffer factor", buffer)?;
    }
    let (source_bounds, source_boundary) = source;
    let target_bounds = union(
        target_leaves
            .iter()
            .filter_map(|(_, dimensions, _)| *dimensions),
    );
    let (mut x, mut y) = if stretch {
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
    if let Some(buffer) = circle_buffer_factor {
        // Circle.surround re-reads its target after replace. This matters when
        // the target contains the source; both its size and center may change.
        let target_after_replace = union(target_leaves.iter().filter_map(|(leaf, bounds, _)| {
            bounds.map(|mut bounds| {
                if sources.contains(leaf) {
                    bounds.min_x = destination.0 + (bounds.min_x - center.0) * x;
                    bounds.max_x = destination.0 + (bounds.max_x - center.0) * x;
                    bounds.min_y = destination.1 + (bounds.min_y - center.1) * y;
                    bounds.max_y = destination.1 + (bounds.max_y - center.1) * y;
                }
                bounds
            })
        }));
        let diagonal = LayoutDimension::Width
            .length(target_after_replace)
            .hypot(LayoutDimension::Height.length(target_after_replace));
        let source_width = LayoutDimension::Width.length(source_bounds) * x;
        // The width setter is a no-op for a collapsed source, then scale still
        // applies the buffer. Preserve stretched ellipses rather than forcing
        // a circle: the pinned implementation keeps their current aspect ratio.
        let fit = if source_width == 0.0 {
            1.0
        } else {
            authoring_render_f64("circle surround width fit", diagonal / source_width)?
        };
        let scale = authoring_render_f64("circle surround scale", fit * buffer)?;
        x = authoring_render_f64("circle surround x scale", x * scale)?;
        y = authoring_render_f64("circle surround y scale", y * scale)?;
    }
    let mut transaction = noon_core::SemanticMutationTransaction::new();
    let mut replacements = Vec::new();
    for &leaf in leaves {
        let previous = store
            .semantic_object_state_checked(leaf)
            .map_err(AuthoringError::from)?;
        let planar = previous
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        let Ok((local_x, local_y)) = world_scale_factors(planar.rotation_z, x, y) else {
            let (baked, path) = crate::family_affine::world_scaled_replacement(
                store,
                previous,
                x,
                y,
                center,
                destination,
            )?;
            replacements.push((leaf, baked, path));
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
        self.replace_layout_with_circle_buffer(target, dimension, stretch, None)
    }

    /// Apply ManimCE Circle.surround sizing through one atomic affine edit.
    ///
    /// The planar dimensions are 0 (width), 1 (height), and 2 (zero depth).
    /// Stretch ignores the dimension and retains the fitted aspect ratio.
    /// The target is remeasured after replacement, including shared source leaves.
    pub fn surround_circle(
        &self,
        target: &LayoutAnchor,
        dimension: u32,
        stretch: bool,
        buffer_factor: f64,
    ) -> Result<(), AuthoringError> {
        let dimension = circle_surround_dimension(dimension, stretch)?;
        self.replace_layout_with_circle_buffer(target, dimension, stretch, Some(buffer_factor))
    }

    fn replace_layout_with_circle_buffer(
        &self,
        target: &LayoutAnchor,
        dimension: LayoutDimension,
        stretch: bool,
        circle_buffer_factor: Option<f64>,
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
                Ok((
                    node,
                    object.content_layout_bounds()?,
                    object.content_boundary_bounds()?,
                ))
            })
            .collect::<Result<Vec<_>, AuthoringError>>()?;
        let prepared = replacement_transaction(
            &self.integration_store().borrow(),
            source.leaves(),
            (source.bounds(), source.boundary_bounds()),
            &target_leaves,
            dimension,
            stretch,
            circle_buffer_factor,
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

pub(crate) fn circle_surround_dimension(
    dimension: u32,
    stretch: bool,
) -> Result<LayoutDimension, AuthoringError> {
    if stretch {
        Ok(LayoutDimension::Width)
    } else {
        dimension.try_into()
    }
}

/// The extent is measured before pivot resolution; unfinished arrays still
/// contribute all their controls, and an empty source remains a no-op.
pub(crate) fn depth_length_for_leaves(
    store: &noon_core::SemanticStore,
    leaves: &[crate::SemanticNodeId],
) -> Result<f64, AuthoringError> {
    let mut bounds: Option<(f64, f64)> = None;
    for &leaf in leaves {
        let state = store.semantic_object_state_checked(leaf)?;
        let transform = state
            .transform
            .as_planar()
            .ok_or(AuthoringError::NonFiniteObjectState)?;
        if let Some((min, max)) =
            crate::semantic_mobject::depth_for_content(store, state.content, transform, true)?
        {
            bounds = Some(bounds.map_or((min, max), |(old_min, old_max)| {
                (old_min.min(min), old_max.max(max))
            }));
        }
    }
    Ok(bounds.map_or(0.0, |(min, max)| max - min))
}

pub(crate) fn fitting_factor(previous: f64, length: f64) -> Result<Option<f64>, AuthoringError> {
    if previous == 0.0 {
        return Ok(None);
    }
    for (name, value) in [("fit length", length), ("fit scale", length / previous)] {
        if !value.is_finite() {
            return Err(AuthoringError::InvalidRenderNumber {
                name: name.into(),
                value,
            });
        }
    }
    Ok(Some(length / previous))
}

pub(crate) fn fitting_factors(
    factor: f64,
    dimension: LayoutDimension,
    stretch: bool,
) -> crate::SemanticVec3 {
    if !stretch {
        return crate::SemanticVec3::new(factor, factor, factor);
    }
    match dimension {
        LayoutDimension::Width => crate::SemanticVec3::new(factor, 1.0, 1.0),
        LayoutDimension::Height => crate::SemanticVec3::new(1.0, factor, 1.0),
        LayoutDimension::Depth => crate::SemanticVec3::new(1.0, 1.0, factor),
    }
}

#[cfg(test)]
mod xyz_tests;

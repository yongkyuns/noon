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

pub(crate) fn validate_fit_stretch(rotation: f64, stretch: bool) -> Result<(), AuthoringError> {
    // The planar affine representation has local scale plus rotation, not shear.
    // A world-axis stretch of a rotated shape cannot generally use local scale.
    if stretch && rotation.sin().abs() > 1.0e-12 {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::RotatedDimensionStretch,
        ));
    }
    Ok(())
}

impl LayoutAnchor {
    /// Fit the selected object or family through its ordinary affine semantics.
    /// A zero source extent is a no-op; families edit each unique leaf once.
    pub fn rescale_to_fit(
        &self,
        length: f64,
        dimension: LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
        let layout = self.layout()?;
        let Some((x, y)) = dimension.scale(layout.bounds(), length, stretch)? else {
            return Ok(());
        };
        let transaction = {
            let store = self.integration_store().borrow();
            for &leaf in layout.leaves() {
                let state = store
                    .semantic_object_state_checked(leaf)
                    .map_err(AuthoringError::from)?;
                validate_fit_stretch(state.transform.rotation_z, stretch)?;
            }
            crate::family_affine::FamilyAffine::Scale(x, y).transaction(
                &store,
                layout.leaves(),
                layout.bounds(),
            )?
        };
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Match an object's or family's dimension using a fresh shared observation.
    pub fn match_dim_size(
        &self,
        target: &LayoutAnchor,
        dimension: LayoutDimension,
        stretch: bool,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), target.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let length = dimension.length(target.layout()?.bounds());
        self.rescale_to_fit(length, dimension, stretch)
    }
}

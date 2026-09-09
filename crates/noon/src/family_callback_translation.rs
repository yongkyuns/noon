//! All-before-write translation of a revision-pinned, unique semantic family.

use crate::{FamilyCallbackPaintError, MobjectFamily, SemanticNodeId, Transform2D};
use noon_core::{Rect, SceneRevision, SemanticLoweringError, SemanticVec3};

/// Existing family-read or shared numeric-lowering rejection. No write is exposed
/// when selection, a late read, or a translated coordinate fails.
#[derive(Debug)]
pub enum FamilyCallbackTranslationError {
    Family(FamilyCallbackPaintError),
    Translation(SemanticLoweringError),
}

impl std::fmt::Display for FamilyCallbackTranslationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Family(error) => error.fmt(f),
            Self::Translation(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for FamilyCallbackTranslationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Family(error) => Some(error),
            Self::Translation(error) => Some(error),
        }
    }
}
impl From<FamilyCallbackPaintError> for FamilyCallbackTranslationError {
    fn from(error: FamilyCallbackPaintError) -> Self {
        Self::Family(error)
    }
}
impl From<SemanticLoweringError> for FamilyCallbackTranslationError {
    fn from(error: SemanticLoweringError) -> Self {
        Self::Translation(error)
    }
}

/// Prepared effective transform and its optional translated observation. This is
/// one operation's result, not retained family state or another mutation protocol.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CallbackFamilyTranslation {
    pub node: SemanticNodeId,
    pub transform: Transform2D,
    pub bounds: Option<Rect>,
}

impl MobjectFamily {
    /// Translate unique semantic leaves using the exact phase's effective rows,
    /// including preceding overlay writes. No authored state or read row changes
    /// until the caller has the entire successful result. Work is family-local;
    /// geometry and unrelated objects are never read.
    pub fn prepare_callback_translation(
        &self,
        revision: SceneRevision,
        x: f64,
        y: f64,
        mut read: impl FnMut(
            SemanticNodeId,
        ) -> Result<
            (Transform2D, Option<Rect>),
            crate::ExecutionSessionCallbackError,
        >,
    ) -> Result<Vec<CallbackFamilyTranslation>, FamilyCallbackTranslationError> {
        let leaves = self.callback_leaf_nodes(revision)?;
        // Same high-precision input and explicit f32 lowering as authored shift.
        let delta = SemanticVec3::new(x, y, 0.0);
        delta.lower_xy_f32()?;
        let mut changes = Vec::with_capacity(leaves.len());
        for node in leaves {
            let (previous, bounds) = read(node).map_err(FamilyCallbackPaintError::Callback)?;
            // Retain shared numeric causes even when an earlier overlay operation
            // supplied invalid values. Never partially append this operation.
            SemanticVec3::new(
                previous.scale.x.into(),
                previous.scale.y.into(),
                previous.rotation.into(),
            )
            .lower_xy_f32()?;
            let translation = SemanticVec3::new(
                f64::from(previous.translation.x) + delta.x,
                f64::from(previous.translation.y) + delta.y,
                0.0,
            )
            .lower_xy_f32()?;
            let transform = Transform2D {
                translation,
                ..previous
            };
            if transform != previous {
                let offset = translation - previous.translation;
                // Translation-only bounds propagation matches the existing
                // EffectiveObjectProperties::set_transform fallback exactly.
                let bounds = bounds.map(|b| Rect::new(b.min + offset, b.max + offset));
                changes.push(CallbackFamilyTranslation {
                    node,
                    transform,
                    bounds,
                });
            }
        }
        Ok(changes)
    }
}

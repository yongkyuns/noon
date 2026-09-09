//! Atomic family paint preparation over a revision-pinned effective read view.
use crate::{
    AuthoringError, Color, ExecutionSessionCallbackError, MobjectFamily, SemanticNodeId, Style,
};
use noon_core::SceneRevision;

/// The same paint semantics used by authored families and callback driver writes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FamilyPaint {
    Color(Color),
    Fill {
        color: Option<Color>,
        opacity: Option<f64>,
    },
    Stroke {
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    },
    Opacity(f64),
}

impl FamilyPaint {
    fn apply(self, style: &mut Style) -> Result<(), AuthoringError> {
        match self {
            Self::Color(c) => crate::semantic_mobject::edit_color(
                style,
                c.red.into(),
                c.green.into(),
                c.blue.into(),
                c.alpha.into(),
            ),
            Self::Fill { color, opacity } => crate::family_style::fill(style, color, opacity),
            Self::Stroke {
                color,
                width,
                opacity,
            } => crate::family_style::stroke(style, color, width, opacity),
            Self::Opacity(value) => crate::semantic_mobject::edit_manim_opacity(style, value),
        }
    }
}

#[derive(Debug)]
pub enum FamilyCallbackPaintError {
    Authoring(AuthoringError),
    Store(noon_core::SemanticStoreError),
    Callback(ExecutionSessionCallbackError),
    StaleRevision {
        expected: SceneRevision,
        actual: SceneRevision,
    },
    InvalidPaint(AuthoringError),
}

impl std::fmt::Display for FamilyCallbackPaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Callback(e) => e.fmt(f),
            Self::StaleRevision { expected, actual } => write!(
                f,
                "family callback expected scene revision {}, found {}",
                expected.get(),
                actual.get()
            ),
            Self::InvalidPaint(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for FamilyCallbackPaintError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(e) | Self::InvalidPaint(e) => Some(e),
            Self::Store(e) => Some(e),
            Self::Callback(e) => Some(e),
            _ => None,
        }
    }
}

impl MobjectFamily {
    /// Resolve the affected family only while its authored membership matches the phase.
    pub fn callback_leaf_nodes(
        &self,
        revision: SceneRevision,
    ) -> Result<Vec<SemanticNodeId>, FamilyCallbackPaintError> {
        self.validate()
            .map_err(FamilyCallbackPaintError::Authoring)?;
        let store = self.integration_store().borrow();
        if store.scene_revision() != revision {
            return Err(FamilyCallbackPaintError::StaleRevision {
                expected: revision,
                actual: store.scene_revision(),
            });
        }
        store
            .ordered_leaf_nodes(self.node_id())
            .map_err(FamilyCallbackPaintError::Store)
    }

    /// Prepare one complete effective operation without mutating the store or read view.
    /// The read callback must observe the exact phase, including preceding overlay writes.
    /// Returned changes use unique semantic leaves in first-occurrence order. A failed
    /// final read or paint edit returns no changes; work is proportional to this family.
    pub fn prepare_callback_paint(
        &self,
        revision: SceneRevision,
        operation: FamilyPaint,
        mut read: impl FnMut(SemanticNodeId) -> Result<Style, ExecutionSessionCallbackError>,
    ) -> Result<Vec<(SemanticNodeId, Style)>, FamilyCallbackPaintError> {
        let leaves = self.callback_leaf_nodes(revision)?;
        operation
            .apply(&mut Style::default())
            .map_err(FamilyCallbackPaintError::InvalidPaint)?;
        let mut changes = Vec::with_capacity(leaves.len());
        for node in leaves {
            let previous = read(node).map_err(FamilyCallbackPaintError::Callback)?;
            let mut next = previous;
            operation
                .apply(&mut next)
                .map_err(FamilyCallbackPaintError::InvalidPaint)?;
            if next != previous {
                changes.push((node, next));
            }
        }
        Ok(changes)
    }
}

//! Shared authored text-part queries for semantic text Mobjects.
//!
//! Selection is resolved against the immutable retained text resource owned by the
//! semantic store. Python/WASM wrappers consume these same values; they do not
//! maintain a second source-to-glyph map.

use noon_core::{SemanticNodeId, TextPart, TextPartQueryError, TextSourceSpan};

use crate::{AuthoringError, Mobject};

/// Failure to select one authored text part from a semantic Mobject.
#[derive(Clone, Debug, PartialEq)]
pub enum TextPartAuthoringError {
    /// The semantic Mobject handle or retained resource is invalid.
    Authoring(AuthoringError),
    /// The requested semantic object is not text-backed.
    NotText(SemanticNodeId),
    /// The authored source selection cannot be projected onto normalized text.
    Query(TextPartQueryError),
}

impl std::fmt::Display for TextPartAuthoringError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Authoring(error) => error.fmt(formatter),
            Self::NotText(node) => write!(formatter, "semantic object {node:?} is not text-backed"),
            Self::Query(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextPartAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::Query(error) => Some(error),
            Self::NotText(_) => None,
        }
    }
}

impl From<AuthoringError> for TextPartAuthoringError {
    fn from(error: AuthoringError) -> Self {
        Self::Authoring(error)
    }
}

impl From<TextPartQueryError> for TextPartAuthoringError {
    fn from(error: TextPartQueryError) -> Self {
        Self::Query(error)
    }
}

impl Mobject {
    /// Project one authored UTF-8 source span through this text object's retained resource.
    pub fn text_source_part(
        &self,
        source_span: TextSourceSpan,
    ) -> Result<TextPart, TextPartAuthoringError> {
        let state = self.state()?;
        let handle = state
            .content
            .text()
            .ok_or(TextPartAuthoringError::NotText(self.node_id()))?;
        let store = self.integration_store().borrow();
        let resource = store.text_resources().get(handle).ok_or(
            TextPartAuthoringError::Authoring(AuthoringError::MissingTextResource(handle)),
        )?;
        Ok(resource.source_part(source_span)?)
    }

    /// Select every non-overlapping occurrence of `needle` in authored UTF-8 source order.
    pub fn text_source_parts_for(
        &self,
        needle: &str,
    ) -> Result<Vec<TextPart>, TextPartAuthoringError> {
        let state = self.state()?;
        let handle = state
            .content
            .text()
            .ok_or(TextPartAuthoringError::NotText(self.node_id()))?;
        let store = self.integration_store().borrow();
        let resource = store.text_resources().get(handle).ok_or(
            TextPartAuthoringError::Authoring(AuthoringError::MissingTextResource(handle)),
        )?;
        Ok(resource.source_parts_for(needle)?)
    }
}

#[cfg(all(test, feature = "native-text", feature = "bundled-fonts"))]
mod tests {
    use super::*;
    use crate::{Scene, Text};

    #[test]
    fn semantic_text_parts_reuse_resource_identity_across_object_presentation_edits() {
        let scene = Scene::new();
        let mut label = scene.text(Text::new("Noon Noon")).unwrap();
        let resource = label.state().unwrap().content.text().unwrap();
        let before = label.text_source_parts_for("Noon").unwrap();

        label.shift(2.0, -1.0).unwrap();
        label.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
        let after = label.text_source_parts_for("Noon").unwrap();

        assert_eq!(before, after);
        assert_eq!(label.state().unwrap().content.text(), Some(resource));
        assert_eq!(before.len(), 2);
        assert_eq!(before[0].source_span, TextSourceSpan::new(0, 4));
        assert_eq!(before[1].source_span, TextSourceSpan::new(5, 9));
    }

    #[test]
    fn non_text_objects_are_rejected_without_mutation() {
        let scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let before = circle.state().unwrap();
        assert!(matches!(
            circle.text_source_parts_for("x"),
            Err(TextPartAuthoringError::NotText(node)) if node == circle.node_id()
        ));
        assert_eq!(circle.state().unwrap(), before);
    }
}

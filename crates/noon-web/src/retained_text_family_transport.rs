use noon_core::{ObjectContentRef, TextFamilyAnimationError, TextFamilyAnimationState};
use serde::{Deserialize, Serialize};

/// Per-object transport state for retained Text family animations.
///
/// This state is embedded in the existing retained object delta. Snapshots therefore
/// carry one optional value per object, while incrementals remain sparse and carry
/// family state only for objects that are already dirty. No glyph IDs cross the wire;
/// renderers derive members from the immutable `TextResource`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RetainedTextFamilyTransportState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_family_animation: Option<TextFamilyAnimationState>,
}

impl RetainedTextFamilyTransportState {
    pub const fn empty() -> Self {
        Self {
            text_family_animation: None,
        }
    }

    pub fn new(
        content: &ObjectContentRef,
        text_family_animation: Option<TextFamilyAnimationState>,
    ) -> Result<Self, RetainedTextFamilyTransportError> {
        let state = Self {
            text_family_animation,
        };
        state.validate(content)?;
        Ok(state)
    }

    /// Revalidate the optional animation after any serialization boundary.
    pub fn validate(
        self,
        content: &ObjectContentRef,
    ) -> Result<(), RetainedTextFamilyTransportError> {
        let Some(animation) = self.text_family_animation else {
            return Ok(());
        };
        if !matches!(content, ObjectContentRef::Text(_)) {
            return Err(RetainedTextFamilyTransportError::NonTextObject);
        }
        animation
            .validate()
            .map_err(RetainedTextFamilyTransportError::InvalidState)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedTextFamilyTransportError {
    NonTextObject,
    InvalidState(TextFamilyAnimationError),
}

impl std::fmt::Display for RetainedTextFamilyTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonTextObject => {
                formatter.write_str("retained Text family transport state targets non-text content")
            }
            Self::InvalidState(error) => {
                write!(
                    formatter,
                    "invalid retained Text family transport state: {error}"
                )
            }
        }
    }
}

impl std::error::Error for RetainedTextFamilyTransportError {}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, ObjectId, RateFunction, RetainedObjectDefinition, TextFamilyAnimationMode,
        TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(17),
            version: 0,
        }
    }

    fn geometry_content() -> ObjectContentRef {
        RetainedObjectDefinition::geometry(ObjectId::new(1), GeometryRef::circle(1.0)).content
    }

    fn text_content() -> ObjectContentRef {
        RetainedObjectDefinition::text(ObjectId::new(2), text_handle()).content
    }

    fn state() -> TextFamilyAnimationState {
        TextFamilyAnimationState {
            mode: TextFamilyAnimationMode::Reveal,
            overall_progress: 0.5,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    #[test]
    fn text_state_round_trips_without_glyph_identity() {
        let content = text_content();
        let transport = RetainedTextFamilyTransportState::new(&content, Some(state())).unwrap();
        assert_eq!(transport.text_family_animation, Some(state()));

        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("text_family_animation"));
        assert!(!json.contains("glyph"));
        let decoded: RetainedTextFamilyTransportState = serde_json::from_str(&json).unwrap();
        decoded.validate(&content).unwrap();
        assert_eq!(decoded, transport);
    }

    #[test]
    fn empty_state_is_valid_for_any_object_and_serializes_compactly() {
        let transport = RetainedTextFamilyTransportState::empty();
        transport.validate(&geometry_content()).unwrap();
        transport.validate(&text_content()).unwrap();
        assert_eq!(serde_json::to_string(&transport).unwrap(), "{}");
    }

    #[test]
    fn family_state_on_geometry_is_rejected() {
        assert_eq!(
            RetainedTextFamilyTransportState::new(&geometry_content(), Some(state())).unwrap_err(),
            RetainedTextFamilyTransportError::NonTextObject
        );
    }

    #[test]
    fn malformed_serialized_state_is_revalidated() {
        let mut invalid = state();
        invalid.overall_progress = 1.5;
        assert_eq!(
            RetainedTextFamilyTransportState::new(&text_content(), Some(invalid)).unwrap_err(),
            RetainedTextFamilyTransportError::InvalidState(
                TextFamilyAnimationError::InvalidOverallProgress(1.5)
            )
        );
    }
}

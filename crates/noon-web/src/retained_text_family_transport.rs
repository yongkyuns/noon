use noon_core::{ObjectContentRef, TextFamilyAnimationError, TextFamilyAnimationState};
use noon_runtime::RetainedFrameState;
use serde::{Deserialize, Serialize};

/// Object-aligned renderer transport state for retained Text family animations.
///
/// The vector deliberately contains no glyph IDs. Renderers derive family members
/// from each immutable `TextResource`, preserving one semantic retained Text object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTextFamilyTransportState {
    pub objects: Vec<Option<TextFamilyAnimationState>>,
}

impl RetainedTextFamilyTransportState {
    pub fn empty(object_count: usize) -> Self {
        Self {
            objects: vec![None; object_count],
        }
    }

    pub fn new(
        frame: &RetainedFrameState,
        objects: Vec<Option<TextFamilyAnimationState>>,
    ) -> Result<Self, RetainedTextFamilyTransportError> {
        let state = Self { objects };
        state.validate(frame)?;
        Ok(state)
    }

    /// Validate transport state after any serialization boundary.
    pub fn validate(
        &self,
        frame: &RetainedFrameState,
    ) -> Result<(), RetainedTextFamilyTransportError> {
        if self.objects.len() != frame.objects.len() {
            return Err(RetainedTextFamilyTransportError::FrameShapeMismatch {
                expected: frame.objects.len(),
                actual: self.objects.len(),
            });
        }

        for (index, state) in self.objects.iter().copied().enumerate() {
            let Some(state) = state else {
                continue;
            };
            if !matches!(frame.objects[index].content, ObjectContentRef::Text(_)) {
                return Err(RetainedTextFamilyTransportError::NonTextObject(index));
            }
            state
                .validate()
                .map_err(|error| RetainedTextFamilyTransportError::InvalidState { index, error })?;
        }
        Ok(())
    }

    pub fn state(&self, object_index: usize) -> Option<TextFamilyAnimationState> {
        self.objects.get(object_index).copied().flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedTextFamilyTransportError {
    FrameShapeMismatch {
        expected: usize,
        actual: usize,
    },
    NonTextObject(usize),
    InvalidState {
        index: usize,
        error: TextFamilyAnimationError,
    },
}

impl std::fmt::Display for RetainedTextFamilyTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameShapeMismatch { expected, actual } => write!(
                formatter,
                "retained Text family transport state has {actual} slots; expected {expected}"
            ),
            Self::NonTextObject(index) => write!(
                formatter,
                "retained Text family transport state targets non-text object index {index}"
            ),
            Self::InvalidState { index, error } => {
                write!(formatter, "invalid retained Text family state at object index {index}: {error}")
            }
        }
    }
}

impl std::error::Error for RetainedTextFamilyTransportError {}

#[cfg(test)]
mod tests {
    use noon_compile::RetainedCompiledScene;
    use noon_core::{
        GeometryRef, ObjectId, RateFunction, RetainedObjectDefinition, TextFamilyAnimationMode,
        TextResourceHandle, TextResourceId,
    };
    use noon_runtime::RetainedSceneInstance;

    use super::*;

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            id: TextResourceId::new(17),
            version: 0,
        }
    }

    fn frame() -> RetainedFrameState {
        let compiled = RetainedCompiledScene::compile(
            &[
                RetainedObjectDefinition::geometry(ObjectId::new(1), GeometryRef::circle(1.0)),
                RetainedObjectDefinition::text(ObjectId::new(2), text_handle()),
            ],
            &[],
        )
        .unwrap();
        RetainedSceneInstance::new(compiled).frame().clone()
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
    fn text_only_object_aligned_state_round_trips() {
        let frame = frame();
        let transport =
            RetainedTextFamilyTransportState::new(&frame, vec![None, Some(state())]).unwrap();
        assert_eq!(transport.state(0), None);
        assert_eq!(transport.state(1), Some(state()));

        let json = serde_json::to_string(&transport).unwrap();
        let decoded: RetainedTextFamilyTransportState = serde_json::from_str(&json).unwrap();
        decoded.validate(&frame).unwrap();
        assert_eq!(decoded, transport);
    }

    #[test]
    fn frame_shape_mismatch_fails_closed() {
        let frame = frame();
        assert_eq!(
            RetainedTextFamilyTransportState::new(&frame, vec![None]).unwrap_err(),
            RetainedTextFamilyTransportError::FrameShapeMismatch {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn family_state_on_geometry_is_rejected() {
        let frame = frame();
        assert_eq!(
            RetainedTextFamilyTransportState::new(&frame, vec![Some(state()), None]).unwrap_err(),
            RetainedTextFamilyTransportError::NonTextObject(0)
        );
    }

    #[test]
    fn malformed_serialized_state_is_revalidated() {
        let frame = frame();
        let mut invalid = state();
        invalid.overall_progress = 1.5;
        assert!(matches!(
            RetainedTextFamilyTransportState::new(&frame, vec![None, Some(invalid)]),
            Err(RetainedTextFamilyTransportError::InvalidState {
                index: 1,
                error: TextFamilyAnimationError::InvalidOverallProgress(1.5),
            })
        ));
    }

    #[test]
    fn empty_state_matches_frame_shape() {
        let frame = frame();
        let transport = RetainedTextFamilyTransportState::empty(frame.objects.len());
        transport.validate(&frame).unwrap();
        assert_eq!(transport.objects, vec![None, None]);
    }
}

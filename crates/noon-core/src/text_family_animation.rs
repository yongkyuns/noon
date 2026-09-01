use crate::{
    FamilyAnimationDefinition, FamilyAnimationError, FamilyAnimationMode, FamilyAnimationState,
};

/// Backward-compatible retained Text names.
///
/// Text is the first consumer of the generic family animation model. Keeping these
/// aliases avoids churn at current compiler/runtime/renderer call sites while the
/// shared abstraction is adopted by additional content types.
pub type TextFamilyAnimationMode = FamilyAnimationMode;
pub type TextFamilyAnimationDefinition = FamilyAnimationDefinition;
pub type TextFamilyAnimationState = FamilyAnimationState;
pub type TextFamilyAnimationError = FamilyAnimationError;

#[cfg(test)]
mod tests {
    use crate::{ObjectId, RateFunction};

    use super::*;

    #[test]
    fn retained_text_aliases_preserve_the_existing_typed_api() {
        let animation = TextFamilyAnimationDefinition::new(
            ObjectId::new(9),
            TextFamilyAnimationMode::DrawBorderThenFill,
            0.0,
            2.0,
            0.2,
            RateFunction::Smooth,
            false,
            true,
        )
        .unwrap();
        let state: TextFamilyAnimationState = animation.state_at(1.0).unwrap();
        assert_eq!(state.mode, TextFamilyAnimationMode::DrawBorderThenFill);
        assert_eq!(state.overall_progress, 0.5);
        assert!(matches!(
            TextFamilyAnimationDefinition::new(
                ObjectId::new(9),
                TextFamilyAnimationMode::Reveal,
                0.0,
                0.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            ),
            Err(TextFamilyAnimationError::InvalidDuration(0.0))
        ));
    }
}

use noon_compile::RetainedCompiledScene;
use noon_core::{
    ObjectId, TextFamilyAnimationDefinition, TextFamilyAnimationError, TextFamilyAnimationState,
};

use crate::{EvaluationError, FrameChanges, RetainedFrameState, RetainedSceneInstance};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedTextFamilyFrame<'a> {
    pub retained: &'a RetainedFrameState,
    pub text_family_animations: &'a [Option<TextFamilyAnimationState>],
}

impl RetainedTextFamilyFrame<'_> {
    pub fn text_family_animation(&self, object_index: usize) -> Option<TextFamilyAnimationState> {
        self.text_family_animations
            .get(object_index)
            .copied()
            .flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedTextFamilyRuntimeError {
    Animation(TextFamilyAnimationError),
    UnknownObject(ObjectId),
    NonTextObject(ObjectId),
    OverlappingAnimations {
        object: ObjectId,
        previous_start: f64,
        previous_end: f64,
        next_start: f64,
    },
    Evaluation(EvaluationError),
}

impl std::fmt::Display for RetainedTextFamilyRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "Text family animation references unknown retained object {}",
                object.get()
            ),
            Self::NonTextObject(object) => write!(
                formatter,
                "Text family animation targets non-text retained object {}",
                object.get()
            ),
            Self::OverlappingAnimations {
                object,
                previous_start,
                previous_end,
                next_start,
            } => write!(
                formatter,
                "Text family animations overlap for object {}: previous [{previous_start}, {previous_end}], next starts at {next_start}",
                object.get()
            ),
            Self::Evaluation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedTextFamilyRuntimeError {}

impl From<TextFamilyAnimationError> for RetainedTextFamilyRuntimeError {
    fn from(value: TextFamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<EvaluationError> for RetainedTextFamilyRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Additive retained runtime for Text family animations.
///
/// Ordinary retained object properties remain owned by [`RetainedSceneInstance`].
/// This wrapper evaluates only content-specific Text family state, preserving raw
/// animation progress until the renderer combines it with shaped resource members.
/// No glyph identity or Python callback enters runtime state.
#[derive(Clone, Debug)]
pub struct RetainedTextFamilySceneInstance {
    inner: RetainedSceneInstance,
    animations_by_object: Vec<Vec<TextFamilyAnimationDefinition>>,
    states: Vec<Option<TextFamilyAnimationState>>,
    family_changed_indices: Vec<usize>,
}

impl RetainedTextFamilySceneInstance {
    pub fn new(
        compiled: RetainedCompiledScene,
        animations: Vec<TextFamilyAnimationDefinition>,
    ) -> Result<Self, RetainedTextFamilyRuntimeError> {
        let object_count = compiled.objects().len();
        let mut animations_by_object = vec![Vec::new(); object_count];

        for animation in animations {
            animation.validate()?;
            let object_index = compiled.object_index(animation.object).ok_or(
                RetainedTextFamilyRuntimeError::UnknownObject(animation.object),
            )? as usize;
            if compiled.objects()[object_index].text().is_none() {
                return Err(RetainedTextFamilyRuntimeError::NonTextObject(
                    animation.object,
                ));
            }
            animations_by_object[object_index].push(animation);
        }

        for object_animations in &mut animations_by_object {
            object_animations.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
            for pair in object_animations.windows(2) {
                let previous = pair[0];
                let next = pair[1];
                if next.start_time < previous.end_time() {
                    return Err(RetainedTextFamilyRuntimeError::OverlappingAnimations {
                        object: previous.object,
                        previous_start: previous.start_time,
                        previous_end: previous.end_time(),
                        next_start: next.start_time,
                    });
                }
            }
        }

        let inner = RetainedSceneInstance::new(compiled);
        let states = evaluate_family_states(&animations_by_object, 0.0)?;
        Ok(Self {
            inner,
            animations_by_object,
            states,
            family_changed_indices: Vec::new(),
        })
    }

    pub fn frame(&self) -> RetainedTextFamilyFrame<'_> {
        RetainedTextFamilyFrame {
            retained: self.inner.frame(),
            text_family_animations: &self.states,
        }
    }

    pub fn evaluate(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner.evaluate(time)?;
        self.update_family_states(time)?;
        Ok(self.frame())
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner.seek(time)?;
        self.update_family_states(time)?;
        Ok(self.frame())
    }

    pub fn advance_to(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner.advance_to(time)?;
        self.update_family_states(time)?;
        Ok(self.frame())
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        let base = self.inner.take_frame_changes();
        if base.is_all() {
            self.family_changed_indices.clear();
            return FrameChanges::all();
        }

        let mut object_indices = base.object_indices().to_vec();
        object_indices.append(&mut self.family_changed_indices);
        object_indices.sort_unstable();
        object_indices.dedup();
        FrameChanges::with_structure(
            object_indices,
            base.added_indices().to_vec(),
            base.removed_indices().to_vec(),
        )
    }

    pub fn inner(&self) -> &RetainedSceneInstance {
        &self.inner
    }

    fn update_family_states(&mut self, time: f64) -> Result<(), RetainedTextFamilyRuntimeError> {
        let next = evaluate_family_states(&self.animations_by_object, time)?;
        for (index, (previous, current)) in self.states.iter().zip(&next).enumerate() {
            if previous != current {
                self.family_changed_indices.push(index);
            }
        }
        self.states = next;
        Ok(())
    }
}

fn evaluate_family_states(
    animations_by_object: &[Vec<TextFamilyAnimationDefinition>],
    time: f64,
) -> Result<Vec<Option<TextFamilyAnimationState>>, RetainedTextFamilyRuntimeError> {
    if !time.is_finite() {
        return Err(RetainedTextFamilyRuntimeError::Evaluation(
            EvaluationError::InvalidTime(time),
        ));
    }

    animations_by_object
        .iter()
        .map(|animations| {
            animations
                .iter()
                .rev()
                .find(|animation| animation.start_time <= time && time <= animation.end_time())
                .copied()
                .map(|animation| animation.state_at(time))
                .transpose()
                .map_err(Into::into)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use noon_core::{
        GeometryRef, RateFunction, RetainedObjectDefinition, TextFamilyAnimationMode,
        TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            id: TextResourceId::new(11),
            version: 0,
        }
    }

    fn compiled_text(object: ObjectId) -> RetainedCompiledScene {
        RetainedCompiledScene::compile(
            &[RetainedObjectDefinition::text(object, text_handle())],
            &[],
        )
        .unwrap()
    }

    fn animation(
        object: ObjectId,
        start_time: f64,
        duration: f64,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> TextFamilyAnimationDefinition {
        TextFamilyAnimationDefinition::new(
            object,
            TextFamilyAnimationMode::Reveal,
            start_time,
            duration,
            1.0,
            RateFunction::Linear,
            reverse_rate_function,
            reverse_member_order,
        )
        .unwrap()
    }

    #[test]
    fn direct_seek_and_incremental_advance_produce_identical_family_state() {
        let object = ObjectId::new(7);
        let definitions = vec![animation(object, 1.0, 2.0, false, false)];
        let mut incremental =
            RetainedTextFamilySceneInstance::new(compiled_text(object), definitions.clone())
                .unwrap();
        incremental.take_frame_changes();
        incremental.advance_to(1.25).unwrap();
        incremental.advance_to(2.0).unwrap();
        let incremental_state = incremental.frame().text_family_animation(0).unwrap();

        let mut direct =
            RetainedTextFamilySceneInstance::new(compiled_text(object), definitions).unwrap();
        direct.seek(2.0).unwrap();
        let direct_state = direct.frame().text_family_animation(0).unwrap();

        assert_eq!(incremental_state, direct_state);
        assert_eq!(direct_state.overall_progress, 0.5);
        assert_eq!(
            (0..5)
                .map(|index| direct_state.member_progress(index, 5).unwrap())
                .collect::<Vec<_>>(),
            vec![1.0, 1.0, 0.5, 0.0, 0.0]
        );
    }

    #[test]
    fn family_state_changes_mark_only_the_target_object_dirty() {
        let first = ObjectId::new(7);
        let second = ObjectId::new(8);
        let compiled = RetainedCompiledScene::compile(
            &[
                RetainedObjectDefinition::text(first, text_handle()),
                RetainedObjectDefinition::text(second, text_handle()),
            ],
            &[],
        )
        .unwrap();
        let mut instance = RetainedTextFamilySceneInstance::new(
            compiled,
            vec![animation(second, 1.0, 1.0, false, false)],
        )
        .unwrap();
        assert!(instance.take_frame_changes().is_all());

        instance.evaluate(0.5).unwrap();
        assert!(instance.take_frame_changes().is_empty());

        instance.evaluate(1.0).unwrap();
        assert_eq!(instance.take_frame_changes().object_indices(), &[1]);
        instance.evaluate(1.5).unwrap();
        assert_eq!(instance.take_frame_changes().object_indices(), &[1]);
        instance.evaluate(2.1).unwrap();
        assert_eq!(instance.take_frame_changes().object_indices(), &[1]);
    }

    #[test]
    fn adjacent_animations_are_allowed_and_later_start_wins_at_boundary() {
        let object = ObjectId::new(7);
        let first = animation(object, 0.0, 1.0, false, false);
        let second = animation(object, 1.0, 1.0, true, true);
        let mut instance =
            RetainedTextFamilySceneInstance::new(compiled_text(object), vec![first, second])
                .unwrap();
        instance.seek(1.0).unwrap();
        let state = instance.frame().text_family_animation(0).unwrap();
        assert_eq!(state.overall_progress, 0.0);
        assert!(state.reverse_rate_function);
        assert!(state.reverse_member_order);
    }

    #[test]
    fn overlapping_animations_on_one_text_object_fail_before_runtime_start() {
        let object = ObjectId::new(7);
        let error = RetainedTextFamilySceneInstance::new(
            compiled_text(object),
            vec![
                animation(object, 0.0, 2.0, false, false),
                animation(object, 1.0, 1.0, false, false),
            ],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RetainedTextFamilyRuntimeError::OverlappingAnimations {
                object: actual,
                previous_start: 0.0,
                previous_end: 2.0,
                next_start: 1.0,
            } if actual == object
        ));
    }

    #[test]
    fn unknown_and_non_text_targets_fail_closed() {
        let text = ObjectId::new(7);
        assert_eq!(
            RetainedTextFamilySceneInstance::new(
                compiled_text(text),
                vec![animation(ObjectId::new(99), 0.0, 1.0, false, false)],
            )
            .unwrap_err(),
            RetainedTextFamilyRuntimeError::UnknownObject(ObjectId::new(99))
        );

        let geometry = ObjectId::new(8);
        let compiled = RetainedCompiledScene::compile(
            &[RetainedObjectDefinition::geometry(
                geometry,
                GeometryRef::circle(1.0),
            )],
            &[],
        )
        .unwrap();
        assert_eq!(
            RetainedTextFamilySceneInstance::new(
                compiled,
                vec![animation(geometry, 0.0, 1.0, false, false)],
            )
            .unwrap_err(),
            RetainedTextFamilyRuntimeError::NonTextObject(geometry)
        );
    }
}

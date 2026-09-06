use noon_compile::CompiledScene;
use noon_core::{
    FamilyAnimationDefinition, FamilyAnimationError, FamilyAnimationState, ObjectId,
    TextFamilyAnimationDefinition, TextFamilyAnimationError, TextFamilyAnimationState,
};

use crate::{EvaluationError, FrameChanges, FrameState, SceneInstance};

/// Evaluated retained frame plus content-independent family animation state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedFamilyFrame<'a> {
    pub retained: &'a FrameState,
    pub family_animations: &'a [Option<FamilyAnimationState>],
}

impl RetainedFamilyFrame<'_> {
    pub fn family_animation(&self, object_index: usize) -> Option<FamilyAnimationState> {
        self.family_animations.get(object_index).copied().flatten()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyRuntimeError {
    Animation(FamilyAnimationError),
    UnknownObject(ObjectId),
    OverlappingAnimations {
        object: ObjectId,
        previous_start: f64,
        previous_end: f64,
        next_start: f64,
    },
    Evaluation(EvaluationError),
}

impl std::fmt::Display for RetainedFamilyRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "family animation references unknown retained object {}",
                object.get()
            ),
            Self::OverlappingAnimations {
                object,
                previous_start,
                previous_end,
                next_start,
            } => write!(
                formatter,
                "family animations overlap for object {}: previous [{previous_start}, {previous_end}], next starts at {next_start}",
                object.get()
            ),
            Self::Evaluation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyRuntimeError {}

impl From<FamilyAnimationError> for RetainedFamilyRuntimeError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<EvaluationError> for RetainedFamilyRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Additive retained runtime for content-independent family animations.
///
/// Ordinary retained object properties remain owned by [`SceneInstance`].
/// This wrapper evaluates only family scheduling state. Concrete content/resource
/// layers decide how a member realizes `Reveal`, `DrawBorderThenFill`, or future
/// family operations; no content identity or host callback enters this scheduler.
#[derive(Clone, Debug)]
pub struct RetainedFamilySceneInstance {
    inner: SceneInstance,
    animations_by_object: Vec<Vec<FamilyAnimationDefinition>>,
    states: Vec<Option<FamilyAnimationState>>,
    family_changed_indices: Vec<usize>,
}

impl RetainedFamilySceneInstance {
    pub fn new(
        compiled: CompiledScene,
        animations: Vec<FamilyAnimationDefinition>,
    ) -> Result<Self, RetainedFamilyRuntimeError> {
        let object_count = compiled.objects().len();
        let mut animations_by_object = vec![Vec::new(); object_count];

        for animation in animations {
            animation.validate()?;
            let object_index = compiled
                .object_index(animation.object)
                .ok_or(RetainedFamilyRuntimeError::UnknownObject(animation.object))?
                as usize;
            animations_by_object[object_index].push(animation);
        }

        for object_animations in &mut animations_by_object {
            object_animations.sort_by(|left, right| left.start_time.total_cmp(&right.start_time));
            for pair in object_animations.windows(2) {
                let previous = pair[0];
                let next = pair[1];
                if next.start_time < previous.end_time() {
                    return Err(RetainedFamilyRuntimeError::OverlappingAnimations {
                        object: previous.object,
                        previous_start: previous.start_time,
                        previous_end: previous.end_time(),
                        next_start: next.start_time,
                    });
                }
            }
        }

        let inner = SceneInstance::new(compiled);
        let states = evaluate_family_states(&animations_by_object, 0.0)?;
        Ok(Self {
            inner,
            animations_by_object,
            states,
            family_changed_indices: Vec::new(),
        })
    }

    pub fn frame(&self) -> RetainedFamilyFrame<'_> {
        RetainedFamilyFrame {
            retained: self.inner.frame(),
            family_animations: &self.states,
        }
    }

    pub fn evaluate(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyRuntimeError> {
        self.inner.evaluate(time)?;
        self.update_family_states(time)?;
        Ok(self.frame())
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyRuntimeError> {
        self.inner.seek(time)?;
        self.update_family_states(time)?;
        Ok(self.frame())
    }

    pub fn advance_to(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyRuntimeError> {
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

    pub fn inner(&self) -> &SceneInstance {
        &self.inner
    }

    fn update_family_states(&mut self, time: f64) -> Result<(), RetainedFamilyRuntimeError> {
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
    animations_by_object: &[Vec<FamilyAnimationDefinition>],
    time: f64,
) -> Result<Vec<Option<FamilyAnimationState>>, RetainedFamilyRuntimeError> {
    if !time.is_finite() {
        return Err(RetainedFamilyRuntimeError::Evaluation(
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
                .map_err(RetainedFamilyRuntimeError::from)
        })
        .collect()
}

/// Compatibility frame for the retained Text consumer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedTextFamilyFrame<'a> {
    pub retained: &'a FrameState,
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

impl From<RetainedFamilyRuntimeError> for RetainedTextFamilyRuntimeError {
    fn from(value: RetainedFamilyRuntimeError) -> Self {
        match value {
            RetainedFamilyRuntimeError::Animation(error) => Self::Animation(error),
            RetainedFamilyRuntimeError::UnknownObject(object) => Self::UnknownObject(object),
            RetainedFamilyRuntimeError::OverlappingAnimations {
                object,
                previous_start,
                previous_end,
                next_start,
            } => Self::OverlappingAnimations {
                object,
                previous_start,
                previous_end,
                next_start,
            },
            RetainedFamilyRuntimeError::Evaluation(error) => Self::Evaluation(error),
        }
    }
}

/// Retained Text compatibility adapter over the generic family scheduler.
///
/// Text-specific responsibility is now limited to validating that targets are
/// retained Text objects and exposing the historical Text-named frame/error API.
#[derive(Clone, Debug)]
pub struct RetainedTextFamilySceneInstance {
    inner: RetainedFamilySceneInstance,
}

impl RetainedTextFamilySceneInstance {
    pub fn new(
        compiled: CompiledScene,
        animations: Vec<TextFamilyAnimationDefinition>,
    ) -> Result<Self, RetainedTextFamilyRuntimeError> {
        for animation in &animations {
            animation
                .validate()
                .map_err(RetainedTextFamilyRuntimeError::Animation)?;
            let object_index = compiled.object_index(animation.object).ok_or(
                RetainedTextFamilyRuntimeError::UnknownObject(animation.object),
            )? as usize;
            if compiled.objects()[object_index].text().is_none() {
                return Err(RetainedTextFamilyRuntimeError::NonTextObject(
                    animation.object,
                ));
            }
        }

        Ok(Self {
            inner: RetainedFamilySceneInstance::new(compiled, animations)
                .map_err(RetainedTextFamilyRuntimeError::from)?,
        })
    }

    pub fn frame(&self) -> RetainedTextFamilyFrame<'_> {
        let frame = self.inner.frame();
        RetainedTextFamilyFrame {
            retained: frame.retained,
            text_family_animations: frame.family_animations,
        }
    }

    pub fn evaluate(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner
            .evaluate(time)
            .map_err(RetainedTextFamilyRuntimeError::from)?;
        Ok(self.frame())
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner
            .seek(time)
            .map_err(RetainedTextFamilyRuntimeError::from)?;
        Ok(self.frame())
    }

    pub fn advance_to(
        &mut self,
        time: f64,
    ) -> Result<RetainedTextFamilyFrame<'_>, RetainedTextFamilyRuntimeError> {
        self.inner
            .advance_to(time)
            .map_err(RetainedTextFamilyRuntimeError::from)?;
        Ok(self.frame())
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        self.inner.take_frame_changes()
    }

    pub fn inner(&self) -> &SceneInstance {
        self.inner.inner()
    }
}

#[cfg(test)]
mod tests {
    use noon_compile::CompiledObject;
    use noon_core::{
        FamilyAnimationMode, GeometryRef, RateFunction, TextFamilyAnimationMode,
        TextResourceHandle, TextResourceId,
    };

    use super::*;

    fn text_handle() -> TextResourceHandle {
        TextResourceHandle {
            arena: 0,
            id: TextResourceId::new(11),
            version: 0,
        }
    }

    fn compiled_text(object: ObjectId) -> CompiledScene {
        CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                text_handle(),
                noon_core::Transform2D::IDENTITY,
                noon_core::Style::default(),
            )],
            &[],
        )
        .unwrap()
    }

    fn compiled_geometry(object: ObjectId) -> CompiledScene {
        CompiledScene::compile_objects(
            vec![CompiledObject::new(
                object,
                GeometryRef::circle(1.0),
                noon_core::Transform2D::IDENTITY,
                noon_core::Style::default(),
            )],
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
    fn generic_family_runtime_accepts_non_text_retained_objects() {
        let object = ObjectId::new(5);
        let definition = FamilyAnimationDefinition::new(
            object,
            FamilyAnimationMode::Reveal,
            0.0,
            2.0,
            0.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        let mut instance =
            RetainedFamilySceneInstance::new(compiled_geometry(object), vec![definition]).unwrap();
        instance.seek(1.0).unwrap();
        assert_eq!(
            instance
                .frame()
                .family_animation(0)
                .unwrap()
                .overall_progress,
            0.5
        );
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
        let compiled = CompiledScene::compile_objects(
            vec![
                CompiledObject::new(
                    first,
                    text_handle(),
                    noon_core::Transform2D::IDENTITY,
                    noon_core::Style::default(),
                ),
                CompiledObject::new(
                    second,
                    text_handle(),
                    noon_core::Transform2D::IDENTITY,
                    noon_core::Style::default(),
                ),
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
    fn overlapping_animations_on_one_text_object_keep_compatibility_error() {
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
    fn unknown_and_non_text_text_targets_fail_closed() {
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
        assert_eq!(
            RetainedTextFamilySceneInstance::new(
                compiled_geometry(geometry),
                vec![animation(geometry, 0.0, 1.0, false, false)],
            )
            .unwrap_err(),
            RetainedTextFamilyRuntimeError::NonTextObject(geometry)
        );
    }
}

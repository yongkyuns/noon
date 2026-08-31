use noon_core::{
    ObjectId, TextFamilyAnimationError, TextFamilyAnimationState,
};
use noon_runtime::{
    FrameChanges, RetainedFrameState, RetainedTextFamilyFrame,
};
use serde::{Deserialize, Serialize};

use crate::{
    RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope,
    RetainedExecutionFrameMirror, RetainedExecutionTransportError,
    RetainedTransportApplyOutcome, TransportObjectContent,
    RETAINED_EXECUTION_TRANSPORT_CHANNEL, RETAINED_EXECUTION_TRANSPORT_VERSION,
};

/// Retained execution protocol carrying object-local Text family animation state.
///
/// Version 2 deliberately reuses the v1 object/session/sequence protocol internally,
/// but old consumers must reject it rather than silently dropping animation state.
pub const RETAINED_TEXT_FAMILY_TRANSPORT_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTextFamilyExecutionDeltaEnvelope {
    #[serde(flatten)]
    pub retained: RetainedExecutionDeltaEnvelope,
    pub text_family_animations: Vec<Option<TextFamilyAnimationState>>,
}

#[derive(Debug)]
pub enum RetainedTextFamilyTransportError {
    Base(RetainedExecutionTransportError),
    UnsupportedVersion(u32),
    StateShapeMismatch {
        objects: usize,
        states: usize,
    },
    InvalidObjectOrder(u32),
    NonTextAnimation(ObjectId),
    Animation(TextFamilyAnimationError),
    MissingFrameSnapshot,
}

impl std::fmt::Display for RetainedTextFamilyTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Base(error) => error.fmt(formatter),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported retained Text family transport version {version}"
            ),
            Self::StateShapeMismatch { objects, states } => write!(
                formatter,
                "retained Text family transport carries {states} animation states for {objects} objects"
            ),
            Self::InvalidObjectOrder(order) => {
                write!(formatter, "invalid retained Text family object order {order}")
            }
            Self::NonTextAnimation(object) => write!(
                formatter,
                "retained Text family animation targets non-text object {}",
                object.get()
            ),
            Self::Animation(error) => error.fmt(formatter),
            Self::MissingFrameSnapshot => formatter
                .write_str("retained Text family transport has no installed frame snapshot"),
        }
    }
}

impl std::error::Error for RetainedTextFamilyTransportError {}

impl From<RetainedExecutionTransportError> for RetainedTextFamilyTransportError {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Base(value)
    }
}

impl From<TextFamilyAnimationError> for RetainedTextFamilyTransportError {
    fn from(value: TextFamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

/// Version-2 encoder that delegates all base retained sequencing to the existing
/// v1 encoder, then attaches the animation state for exactly the transmitted slots.
#[derive(Clone, Debug)]
pub struct RetainedTextFamilyDeltaEncoder {
    base: RetainedExecutionDeltaEncoder,
}

impl RetainedTextFamilyDeltaEncoder {
    pub const fn new(session: u32) -> Self {
        Self {
            base: RetainedExecutionDeltaEncoder::new(session),
        }
    }

    pub fn encode_snapshot(
        &mut self,
        frame: RetainedTextFamilyFrame<'_>,
        camera: noon_core::Camera2DState,
    ) -> Result<RetainedTextFamilyExecutionDeltaEnvelope, RetainedTextFamilyTransportError> {
        let delta = self.base.encode_snapshot(frame.retained, camera)?;
        upgrade_delta(frame, delta)
    }

    pub fn encode_incremental(
        &mut self,
        frame: RetainedTextFamilyFrame<'_>,
        changes: &FrameChanges,
        camera: noon_core::Camera2DState,
    ) -> Result<Option<RetainedTextFamilyExecutionDeltaEnvelope>, RetainedTextFamilyTransportError>
    {
        self.base
            .encode_incremental(frame.retained, changes, camera)?
            .map(|delta| upgrade_delta(frame, delta))
            .transpose()
    }
}

fn upgrade_delta(
    frame: RetainedTextFamilyFrame<'_>,
    mut retained: RetainedExecutionDeltaEnvelope,
) -> Result<RetainedTextFamilyExecutionDeltaEnvelope, RetainedTextFamilyTransportError> {
    let mut states = Vec::with_capacity(retained.objects.len());
    for object in &retained.objects {
        let index = usize::try_from(object.order)
            .map_err(|_| RetainedTextFamilyTransportError::InvalidObjectOrder(object.order))?;
        let state = frame
            .text_family_animations
            .get(index)
            .copied()
            .ok_or(RetainedTextFamilyTransportError::InvalidObjectOrder(
                object.order,
            ))?;
        validate_object_animation(object.object, &object.content, state)?;
        states.push(state);
    }
    retained.protocol_version = RETAINED_TEXT_FAMILY_TRANSPORT_VERSION;
    Ok(RetainedTextFamilyExecutionDeltaEnvelope {
        retained,
        text_family_animations: states,
    })
}

fn validate_object_animation(
    object: ObjectId,
    content: &TransportObjectContent,
    state: Option<TextFamilyAnimationState>,
) -> Result<(), RetainedTextFamilyTransportError> {
    let Some(state) = state else {
        return Ok(());
    };
    state.validate()?;
    if !matches!(content, TransportObjectContent::Text { .. }) {
        return Err(RetainedTextFamilyTransportError::NonTextAnimation(object));
    }
    Ok(())
}

/// Transactional v2 mirror. The v1 mirror remains the authority for session,
/// sequence, slot identity, content identity, camera, and retained frame state.
/// Animation state is committed only after both layers validate successfully.
#[derive(Clone, Debug, Default)]
pub struct RetainedTextFamilyExecutionFrameMirror {
    base: RetainedExecutionFrameMirror,
    text_family_animations: Vec<Option<TextFamilyAnimationState>>,
}

impl RetainedTextFamilyExecutionFrameMirror {
    pub fn frame(&self) -> Option<&RetainedFrameState> {
        self.base.frame()
    }

    pub const fn camera(&self) -> noon_core::Camera2DState {
        self.base.camera()
    }

    pub fn text_family_animations(&self) -> &[Option<TextFamilyAnimationState>] {
        &self.text_family_animations
    }

    pub fn text_family_animation(
        &self,
        object_index: usize,
    ) -> Option<TextFamilyAnimationState> {
        self.text_family_animations
            .get(object_index)
            .copied()
            .flatten()
    }

    pub fn apply(
        &mut self,
        delta: RetainedTextFamilyExecutionDeltaEnvelope,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), RetainedTextFamilyTransportError>
    {
        if delta.retained.channel != RETAINED_EXECUTION_TRANSPORT_CHANNEL {
            let mut base = delta.retained.clone();
            base.protocol_version = RETAINED_EXECUTION_TRANSPORT_VERSION;
            return Err(RetainedTextFamilyTransportError::Base(
                RetainedExecutionFrameMirror::default()
                    .apply(base)
                    .expect_err("invalid channel must be rejected by base transport"),
            ));
        }
        if delta.retained.protocol_version != RETAINED_TEXT_FAMILY_TRANSPORT_VERSION {
            return Err(RetainedTextFamilyTransportError::UnsupportedVersion(
                delta.retained.protocol_version,
            ));
        }
        if delta.retained.objects.len() != delta.text_family_animations.len() {
            return Err(RetainedTextFamilyTransportError::StateShapeMismatch {
                objects: delta.retained.objects.len(),
                states: delta.text_family_animations.len(),
            });
        }

        let mut candidate_base = self.base.clone();
        let mut base_delta = delta.retained.clone();
        base_delta.protocol_version = RETAINED_EXECUTION_TRANSPORT_VERSION;
        let (outcome, changes) = candidate_base.apply(base_delta)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }

        for (object, state) in delta
            .retained
            .objects
            .iter()
            .zip(&delta.text_family_animations)
        {
            validate_object_animation(object.object, &object.content, *state)?;
        }

        let frame = candidate_base
            .frame()
            .ok_or(RetainedTextFamilyTransportError::MissingFrameSnapshot)?;
        let mut candidate_states = if delta.retained.snapshot {
            vec![None; frame.objects.len()]
        } else {
            if self.text_family_animations.len() != frame.objects.len() {
                return Err(RetainedTextFamilyTransportError::StateShapeMismatch {
                    objects: frame.objects.len(),
                    states: self.text_family_animations.len(),
                });
            }
            self.text_family_animations.clone()
        };

        for (object, state) in delta
            .retained
            .objects
            .iter()
            .zip(delta.text_family_animations.iter().copied())
        {
            let index = usize::try_from(object.order)
                .map_err(|_| RetainedTextFamilyTransportError::InvalidObjectOrder(object.order))?;
            let destination = candidate_states
                .get_mut(index)
                .ok_or(RetainedTextFamilyTransportError::InvalidObjectOrder(
                    object.order,
                ))?;
            *destination = state;
        }

        self.base = candidate_base;
        self.text_family_animations = candidate_states;
        Ok((outcome, changes))
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Camera2DState, GeometryRef, ObjectContentRef, RateFunction, Style,
        TextFamilyAnimationDefinition, TextFamilyAnimationMode, TextResourceHandle,
        TextResourceId, Transform2D,
    };
    use noon_runtime::RetainedFrameObjectState;

    use super::*;

    fn mixed_frame(time: f64) -> RetainedFrameState {
        let text = TextResourceHandle {
            id: TextResourceId::new(7),
            version: 3,
        };
        RetainedFrameState {
            time,
            objects: vec![
                RetainedFrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                RetainedFrameObjectState {
                    id: ObjectId::new(12),
                    content: ObjectContentRef::Text(text),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
        }
    }

    fn family_state(progress_time: f64) -> TextFamilyAnimationState {
        TextFamilyAnimationDefinition::new(
            ObjectId::new(12),
            TextFamilyAnimationMode::Reveal,
            0.0,
            1.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap()
        .state_at(progress_time)
        .unwrap()
    }

    #[test]
    fn v2_snapshot_round_trips_text_family_state() {
        let frame = mixed_frame(0.5);
        let states = vec![None, Some(family_state(0.5))];
        let family_frame = RetainedTextFamilyFrame {
            retained: &frame,
            text_family_animations: &states,
        };
        let mut encoder = RetainedTextFamilyDeltaEncoder::new(4);
        let delta = encoder
            .encode_snapshot(family_frame, Camera2DState::default())
            .unwrap();
        assert_eq!(
            delta.retained.protocol_version,
            RETAINED_TEXT_FAMILY_TRANSPORT_VERSION
        );

        let json = serde_json::to_string(&delta).unwrap();
        let decoded: RetainedTextFamilyExecutionDeltaEnvelope =
            serde_json::from_str(&json).unwrap();
        let mut mirror = RetainedTextFamilyExecutionFrameMirror::default();
        let (outcome, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.frame().unwrap(), &frame);
        assert_eq!(mirror.text_family_animation(0), None);
        assert_eq!(mirror.text_family_animation(1), Some(family_state(0.5)));
    }

    #[test]
    fn incremental_updates_only_transmitted_family_slot() {
        let initial = mixed_frame(0.0);
        let initial_states = vec![None, Some(family_state(0.0))];
        let mut encoder = RetainedTextFamilyDeltaEncoder::new(7);
        let snapshot = encoder
            .encode_snapshot(
                RetainedTextFamilyFrame {
                    retained: &initial,
                    text_family_animations: &initial_states,
                },
                Camera2DState::default(),
            )
            .unwrap();
        let mut mirror = RetainedTextFamilyExecutionFrameMirror::default();
        mirror.apply(snapshot).unwrap();

        let updated = mixed_frame(0.5);
        let updated_states = vec![None, Some(family_state(0.5))];
        let delta = encoder
            .encode_incremental(
                RetainedTextFamilyFrame {
                    retained: &updated,
                    text_family_animations: &updated_states,
                },
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.retained.objects.len(), 1);
        assert_eq!(delta.text_family_animations, vec![Some(family_state(0.5))]);

        let (_, changes) = mirror.apply(delta).unwrap();
        assert_eq!(changes.object_indices(), &[1]);
        assert_eq!(mirror.frame().unwrap().time, 0.5);
        assert_eq!(mirror.text_family_animation(1), Some(family_state(0.5)));
    }

    #[test]
    fn encoder_rejects_family_animation_on_geometry() {
        let frame = mixed_frame(0.5);
        let states = vec![Some(family_state(0.5)), None];
        let mut encoder = RetainedTextFamilyDeltaEncoder::new(11);
        let error = encoder
            .encode_snapshot(
                RetainedTextFamilyFrame {
                    retained: &frame,
                    text_family_animations: &states,
                },
                Camera2DState::default(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RetainedTextFamilyTransportError::NonTextAnimation(ObjectId::new(11))
        ));
    }

    #[test]
    fn invalid_family_state_does_not_partially_mutate_mirror() {
        let frame = mixed_frame(0.0);
        let states = vec![None, Some(family_state(0.0))];
        let mut encoder = RetainedTextFamilyDeltaEncoder::new(13);
        let snapshot = encoder
            .encode_snapshot(
                RetainedTextFamilyFrame {
                    retained: &frame,
                    text_family_animations: &states,
                },
                Camera2DState::default(),
            )
            .unwrap();
        let mut mirror = RetainedTextFamilyExecutionFrameMirror::default();
        mirror.apply(snapshot).unwrap();

        let updated = mixed_frame(0.5);
        let updated_states = vec![None, Some(family_state(0.5))];
        let mut delta = encoder
            .encode_incremental(
                RetainedTextFamilyFrame {
                    retained: &updated,
                    text_family_animations: &updated_states,
                },
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        delta.text_family_animations[0]
            .as_mut()
            .unwrap()
            .overall_progress = 2.0;

        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedTextFamilyTransportError::Animation(
                TextFamilyAnimationError::InvalidOverallProgress(2.0)
            ))
        ));
        assert_eq!(mirror.frame().unwrap().time, 0.0);
        assert_eq!(mirror.text_family_animation(1), Some(family_state(0.0)));
    }

    #[test]
    fn v1_payload_is_rejected_by_v2_mirror() {
        let frame = mixed_frame(0.0);
        let states = vec![None, Some(family_state(0.0))];
        let mut encoder = RetainedTextFamilyDeltaEncoder::new(17);
        let mut delta = encoder
            .encode_snapshot(
                RetainedTextFamilyFrame {
                    retained: &frame,
                    text_family_animations: &states,
                },
                Camera2DState::default(),
            )
            .unwrap();
        delta.retained.protocol_version = RETAINED_EXECUTION_TRANSPORT_VERSION;

        let mut mirror = RetainedTextFamilyExecutionFrameMirror::default();
        assert!(matches!(
            mirror.apply(delta),
            Err(RetainedTextFamilyTransportError::UnsupportedVersion(
                RETAINED_EXECUTION_TRANSPORT_VERSION
            ))
        ));
    }
}

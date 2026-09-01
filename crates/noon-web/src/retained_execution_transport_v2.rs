use noon_core::{Camera2DState, ObjectContentRef, TextFamilyAnimationState};
use noon_runtime::{FrameChanges, RetainedFrameState};
use serde::{Deserialize, Serialize};

use crate::{
    RetainedExecutionDeltaEncoder, RetainedExecutionDeltaEnvelope, RetainedExecutionFrameMirror,
    RetainedExecutionTransportError, RetainedTextFamilyTransportError,
    RetainedTextFamilyTransportState, RetainedTransportApplyOutcome, RetainedTransportObjectState,
};

/// First retained execution protocol version that carries Text family animation state.
pub const RETAINED_EXECUTION_TRANSPORT_VERSION_V2: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedTransportObjectStateV2 {
    #[serde(flatten)]
    pub retained: RetainedTransportObjectState,
    #[serde(flatten)]
    pub text_family: RetainedTextFamilyTransportState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedExecutionDeltaEnvelopeV2 {
    pub channel: String,
    pub protocol_version: u32,
    pub session: u32,
    pub sequence: u64,
    pub snapshot: bool,
    pub time: f64,
    #[serde(default)]
    pub camera: Camera2DState,
    pub objects: Vec<RetainedTransportObjectStateV2>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedExecutionTransportV2Error {
    Base(RetainedExecutionTransportError),
    UnsupportedVersion(u32),
    InvalidFamilyState {
        object_index: usize,
        error: RetainedTextFamilyTransportError,
    },
    MissingFamilyState {
        object_index: usize,
        available: usize,
    },
}

impl std::fmt::Display for RetainedExecutionTransportV2Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Base(error) => error.fmt(formatter),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported retained execution v2 transport version {version}"
            ),
            Self::InvalidFamilyState {
                object_index,
                error,
            } => write!(
                formatter,
                "invalid retained Text family state for object index {object_index}: {error}"
            ),
            Self::MissingFamilyState {
                object_index,
                available,
            } => write!(
                formatter,
                "retained Text family state is missing object index {object_index}; only {available} states are available"
            ),
        }
    }
}

impl std::error::Error for RetainedExecutionTransportV2Error {}

impl From<RetainedExecutionTransportError> for RetainedExecutionTransportV2Error {
    fn from(value: RetainedExecutionTransportError) -> Self {
        Self::Base(value)
    }
}

/// Protocol-v2 adapter over the proven retained v1 delta encoder.
///
/// Sequence numbers, snapshot requirements, structural changes, slot identity, and
/// sparse object selection remain owned by v1. This adapter only attaches validated
/// Text-family state to the objects v1 already selected for transport.
#[derive(Clone, Debug)]
pub struct RetainedExecutionDeltaEncoderV2 {
    base: RetainedExecutionDeltaEncoder,
}

impl RetainedExecutionDeltaEncoderV2 {
    pub const fn new(session: u32) -> Self {
        Self {
            base: RetainedExecutionDeltaEncoder::new(session),
        }
    }

    pub fn encode_snapshot(
        &mut self,
        frame: &RetainedFrameState,
        text_family_animations: &[Option<TextFamilyAnimationState>],
        camera: Camera2DState,
    ) -> Result<RetainedExecutionDeltaEnvelopeV2, RetainedExecutionTransportV2Error> {
        let mut next_base = self.base.clone();
        let base = next_base.encode_snapshot(frame, camera)?;
        let delta = Self::attach_family_state(base, frame, text_family_animations)?;
        self.base = next_base;
        Ok(delta)
    }

    pub fn encode_incremental(
        &mut self,
        frame: &RetainedFrameState,
        text_family_animations: &[Option<TextFamilyAnimationState>],
        changes: &FrameChanges,
        camera: Camera2DState,
    ) -> Result<Option<RetainedExecutionDeltaEnvelopeV2>, RetainedExecutionTransportV2Error> {
        let mut next_base = self.base.clone();
        let Some(base) = next_base.encode_incremental(frame, changes, camera)? else {
            self.base = next_base;
            return Ok(None);
        };
        let delta = Self::attach_family_state(base, frame, text_family_animations)?;
        self.base = next_base;
        Ok(Some(delta))
    }

    fn attach_family_state(
        base: RetainedExecutionDeltaEnvelope,
        frame: &RetainedFrameState,
        text_family_animations: &[Option<TextFamilyAnimationState>],
    ) -> Result<RetainedExecutionDeltaEnvelopeV2, RetainedExecutionTransportV2Error> {
        let objects = base
            .objects
            .into_iter()
            .map(|retained| {
                let object_index = retained.order as usize;
                let animation = text_family_animations.get(object_index).copied().ok_or(
                    RetainedExecutionTransportV2Error::MissingFamilyState {
                        object_index,
                        available: text_family_animations.len(),
                    },
                )?;
                let content = frame.objects.get(object_index).ok_or(
                    RetainedExecutionTransportV2Error::MissingFamilyState {
                        object_index,
                        available: frame.objects.len(),
                    },
                )?;
                let text_family =
                    RetainedTextFamilyTransportState::new(&content.content, animation).map_err(
                        |error| RetainedExecutionTransportV2Error::InvalidFamilyState {
                            object_index,
                            error,
                        },
                    )?;
                Ok::<RetainedTransportObjectStateV2, RetainedExecutionTransportV2Error>(
                    RetainedTransportObjectStateV2 {
                        retained,
                        text_family,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RetainedExecutionDeltaEnvelopeV2 {
            channel: base.channel,
            protocol_version: RETAINED_EXECUTION_TRANSPORT_VERSION_V2,
            session: base.session,
            sequence: base.sequence,
            snapshot: base.snapshot,
            time: base.time,
            camera: base.camera,
            objects,
        })
    }
}

/// Protocol-v2 mirror that delegates all retained-frame mutation rules to v1 while
/// maintaining the renderer-visible Text-family state in the same object order.
#[derive(Clone, Debug, Default)]
pub struct RetainedExecutionFrameMirrorV2 {
    base: RetainedExecutionFrameMirror,
    text_family_animations: Vec<Option<TextFamilyAnimationState>>,
}

impl RetainedExecutionFrameMirrorV2 {
    pub fn frame(&self) -> Option<&RetainedFrameState> {
        self.base.frame()
    }

    pub fn text_family_animations(&self) -> &[Option<TextFamilyAnimationState>] {
        &self.text_family_animations
    }

    pub fn text_family_animation(&self, object_index: usize) -> Option<TextFamilyAnimationState> {
        self.text_family_animations
            .get(object_index)
            .copied()
            .flatten()
    }

    pub const fn camera(&self) -> Camera2DState {
        self.base.camera()
    }

    pub fn apply(
        &mut self,
        delta: RetainedExecutionDeltaEnvelopeV2,
    ) -> Result<(RetainedTransportApplyOutcome, FrameChanges), RetainedExecutionTransportV2Error>
    {
        if delta.protocol_version != RETAINED_EXECUTION_TRANSPORT_VERSION_V2 {
            return Err(RetainedExecutionTransportV2Error::UnsupportedVersion(
                delta.protocol_version,
            ));
        }

        let snapshot = delta.snapshot;
        let family_updates = delta
            .objects
            .iter()
            .map(|object| {
                (
                    object.retained.order as usize,
                    object.text_family,
                    object.retained.content.clone(),
                )
            })
            .collect::<Vec<_>>();
        let base_delta = RetainedExecutionDeltaEnvelope {
            channel: delta.channel,
            protocol_version: 1,
            session: delta.session,
            sequence: delta.sequence,
            snapshot,
            time: delta.time,
            camera: delta.camera,
            objects: delta
                .objects
                .into_iter()
                .map(|object| object.retained)
                .collect(),
        };

        let mut next_base = self.base.clone();
        let (outcome, changes) = next_base.apply(base_delta)?;
        if outcome == RetainedTransportApplyOutcome::DroppedStale {
            return Ok((outcome, changes));
        }

        let mut next_family = if snapshot {
            vec![None; next_base.frame().map_or(0, |frame| frame.objects.len())]
        } else {
            self.text_family_animations.clone()
        };
        for (index, text_family, transport_content) in family_updates {
            let content: ObjectContentRef = transport_content.into();
            text_family.validate(&content).map_err(|error| {
                RetainedExecutionTransportV2Error::InvalidFamilyState {
                    object_index: index,
                    error,
                }
            })?;
            let available = next_family.len();
            let slot = next_family.get_mut(index).ok_or(
                RetainedExecutionTransportV2Error::MissingFamilyState {
                    object_index: index,
                    available,
                },
            )?;
            *slot = text_family.text_family_animation;
        }

        self.base = next_base;
        self.text_family_animations = next_family;
        Ok((outcome, changes))
    }
}

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

    fn state(progress: f64) -> TextFamilyAnimationState {
        TextFamilyAnimationState {
            mode: TextFamilyAnimationMode::Reveal,
            overall_progress: progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
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

    #[test]
    fn snapshot_round_trips_family_state_on_same_object_stream() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(4);
        let delta = encoder
            .encode_snapshot(&frame, &[None, Some(state(0.25))], Camera2DState::default())
            .unwrap();
        assert_eq!(delta.protocol_version, 2);
        assert_eq!(delta.objects.len(), 2);
        assert_eq!(
            delta.objects[1].text_family.text_family_animation,
            Some(state(0.25))
        );

        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"text_family_animation\""));
        assert!(!json.contains("glyph"));

        let decoded: RetainedExecutionDeltaEnvelopeV2 = serde_json::from_str(&json).unwrap();
        let mut mirror = RetainedExecutionFrameMirrorV2::default();
        let (outcome, changes) = mirror.apply(decoded).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());
        assert_eq!(mirror.text_family_animation(0), None);
        assert_eq!(mirror.text_family_animation(1), Some(state(0.25)));
    }

    #[test]
    fn incremental_remains_sparse_and_can_clear_family_state() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(5);
        let snapshot = encoder
            .encode_snapshot(&frame, &[None, Some(state(0.25))], Camera2DState::default())
            .unwrap();
        let mut mirror = RetainedExecutionFrameMirrorV2::default();
        mirror.apply(snapshot).unwrap();

        let delta = encoder
            .encode_incremental(
                &frame,
                &[None, None],
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(delta.objects.len(), 1);
        assert_eq!(delta.objects[0].retained.order, 1);
        assert_eq!(delta.objects[0].text_family.text_family_animation, None);

        mirror.apply(delta).unwrap();
        assert_eq!(mirror.text_family_animation(1), None);
    }

    #[test]
    fn stale_delta_does_not_roll_back_family_state() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(6);
        let snapshot = encoder
            .encode_snapshot(&frame, &[None, Some(state(0.2))], Camera2DState::default())
            .unwrap();
        let stale = snapshot.clone();
        let incremental = encoder
            .encode_incremental(
                &frame,
                &[None, Some(state(0.8))],
                &FrameChanges::objects(vec![1]),
                Camera2DState::default(),
            )
            .unwrap()
            .unwrap();

        let mut mirror = RetainedExecutionFrameMirrorV2::default();
        mirror.apply(snapshot).unwrap();
        mirror.apply(incremental).unwrap();
        assert_eq!(mirror.text_family_animation(1), Some(state(0.8)));

        let (outcome, changes) = mirror.apply(stale).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::DroppedStale);
        assert!(changes.is_empty());
        assert_eq!(mirror.text_family_animation(1), Some(state(0.8)));
    }

    #[test]
    fn rejected_family_state_does_not_advance_encoder_sequence() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(7);
        assert!(encoder
            .encode_snapshot(&frame, &[Some(state(0.5)), None], Camera2DState::default(),)
            .is_err());
        let delta = encoder
            .encode_snapshot(&frame, &[None, None], Camera2DState::default())
            .unwrap();
        assert_eq!(delta.sequence, 0);
    }

    #[test]
    fn v1_payload_is_rejected_at_v2_boundary() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(8);
        let mut delta = encoder
            .encode_snapshot(&frame, &[None, None], Camera2DState::default())
            .unwrap();
        delta.protocol_version = 1;
        assert_eq!(
            RetainedExecutionFrameMirrorV2::default()
                .apply(delta)
                .unwrap_err(),
            RetainedExecutionTransportV2Error::UnsupportedVersion(1)
        );
    }

    #[test]
    fn geometry_family_state_is_rejected_before_transport() {
        let frame = frame();
        let mut encoder = RetainedExecutionDeltaEncoderV2::new(9);
        assert!(matches!(
            encoder.encode_snapshot(&frame, &[Some(state(0.5)), None], Camera2DState::default(),),
            Err(RetainedExecutionTransportV2Error::InvalidFamilyState {
                object_index: 0,
                error: RetainedTextFamilyTransportError::NonTextObject,
            })
        ));
    }
}

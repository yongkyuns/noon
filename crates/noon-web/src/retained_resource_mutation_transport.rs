use noon_core::ObjectId;
use serde::{Deserialize, Serialize};

use crate::{RetainedResourceBundle, TransportTextResourceHandle};

pub const RETAINED_RESOURCE_MUTATION_CHANNEL: &str = "noon.execution.retained.resource_mutation";
pub const RETAINED_RESOURCE_MUTATION_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetainedResourceMutationOperation {
    Replace {
        replacement: TransportTextResourceHandle,
        resources: RetainedResourceBundle,
    },
    Remove,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedResourceMutationEnvelope {
    pub channel: String,
    pub protocol_version: u32,
    pub session: u32,
    pub sequence: u64,
    pub object: ObjectId,
    pub expected: TransportTextResourceHandle,
    pub operation: RetainedResourceMutationOperation,
}

impl RetainedResourceMutationEnvelope {
    pub fn replace(
        session: u32,
        sequence: u64,
        object: ObjectId,
        expected: TransportTextResourceHandle,
        replacement: TransportTextResourceHandle,
        resources: RetainedResourceBundle,
    ) -> Self {
        Self {
            channel: RETAINED_RESOURCE_MUTATION_CHANNEL.to_owned(),
            protocol_version: RETAINED_RESOURCE_MUTATION_VERSION,
            session,
            sequence,
            object,
            expected,
            operation: RetainedResourceMutationOperation::Replace {
                replacement,
                resources,
            },
        }
    }

    pub fn remove(
        session: u32,
        sequence: u64,
        object: ObjectId,
        expected: TransportTextResourceHandle,
    ) -> Self {
        Self {
            channel: RETAINED_RESOURCE_MUTATION_CHANNEL.to_owned(),
            protocol_version: RETAINED_RESOURCE_MUTATION_VERSION,
            session,
            sequence,
            object,
            expected,
            operation: RetainedResourceMutationOperation::Remove,
        }
    }

    pub fn encode_binary(&self) -> Result<Vec<u8>, RetainedResourceMutationTransportError> {
        self.validate_protocol()?;
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(self, &mut bytes)
            .map_err(|error| RetainedResourceMutationTransportError::Encode(error.to_string()))?;
        Ok(bytes)
    }

    pub fn decode_binary(bytes: &[u8]) -> Result<Self, RetainedResourceMutationTransportError> {
        let envelope: Self = ciborium::de::from_reader(bytes)
            .map_err(|error| RetainedResourceMutationTransportError::Decode(error.to_string()))?;
        envelope.validate_protocol()?;
        Ok(envelope)
    }

    pub fn validate_protocol(&self) -> Result<(), RetainedResourceMutationTransportError> {
        if self.channel != RETAINED_RESOURCE_MUTATION_CHANNEL {
            return Err(RetainedResourceMutationTransportError::InvalidChannel(
                self.channel.clone(),
            ));
        }
        if self.protocol_version != RETAINED_RESOURCE_MUTATION_VERSION {
            return Err(RetainedResourceMutationTransportError::UnsupportedVersion(
                self.protocol_version,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedResourceMutationSequenceOutcome {
    Accepted,
    DroppedStale,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetainedResourceMutationSequenceGuard {
    session: Option<u32>,
    next_sequence: u64,
}

impl RetainedResourceMutationSequenceGuard {
    pub fn accept(
        &mut self,
        envelope: &RetainedResourceMutationEnvelope,
    ) -> Result<RetainedResourceMutationSequenceOutcome, RetainedResourceMutationTransportError>
    {
        envelope.validate_protocol()?;

        match self.session {
            None => {
                if envelope.sequence != 0 {
                    return Err(
                        RetainedResourceMutationTransportError::SessionRequiresSequenceZero {
                            session: envelope.session,
                            sequence: envelope.sequence,
                        },
                    );
                }
            }
            Some(session) if session != envelope.session => {
                if envelope.sequence != 0 {
                    return Err(
                        RetainedResourceMutationTransportError::SessionRequiresSequenceZero {
                            session: envelope.session,
                            sequence: envelope.sequence,
                        },
                    );
                }
            }
            Some(_) if envelope.sequence < self.next_sequence => {
                return Ok(RetainedResourceMutationSequenceOutcome::DroppedStale);
            }
            Some(_) if envelope.sequence != self.next_sequence => {
                return Err(RetainedResourceMutationTransportError::SequenceGap {
                    expected: self.next_sequence,
                    actual: envelope.sequence,
                });
            }
            Some(_) => {}
        }

        let next_sequence = envelope
            .sequence
            .checked_add(1)
            .ok_or(RetainedResourceMutationTransportError::SequenceExhausted)?;
        self.session = Some(envelope.session);
        self.next_sequence = next_sequence;
        Ok(RetainedResourceMutationSequenceOutcome::Accepted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedResourceMutationTransportError {
    InvalidChannel(String),
    UnsupportedVersion(u32),
    SessionRequiresSequenceZero { session: u32, sequence: u64 },
    SequenceGap { expected: u64, actual: u64 },
    SequenceExhausted,
    Encode(String),
    Decode(String),
}

impl std::fmt::Display for RetainedResourceMutationTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid retained resource mutation channel {channel:?}")
            }
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported retained resource mutation version {version}"
            ),
            Self::SessionRequiresSequenceZero { session, sequence } => write!(
                formatter,
                "retained resource mutation session {session} must begin at sequence 0, got {sequence}"
            ),
            Self::SequenceGap { expected, actual } => write!(
                formatter,
                "retained resource mutation sequence gap: expected {expected}, got {actual}"
            ),
            Self::SequenceExhausted => {
                formatter.write_str("retained resource mutation sequence exhausted")
            }
            Self::Encode(message) => {
                write!(formatter, "retained resource mutation encode failed: {message}")
            }
            Self::Decode(message) => {
                write!(formatter, "retained resource mutation decode failed: {message}")
            }
        }
    }
}

impl std::error::Error for RetainedResourceMutationTransportError {}

#[cfg(test)]
mod tests {
    use noon_core::{
        FontResourceArena, GeometryResourceArena, Rect, TextResource, TextResourceArena,
        TextSourceKind, Vec2,
    };
    use std::sync::Arc;

    use super::*;

    fn text(source: &str) -> TextResource {
        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    fn replacement_bundle() -> (TransportTextResourceHandle, RetainedResourceBundle) {
        let mut texts = TextResourceArena::new();
        let handle = texts.insert(text("replacement")).unwrap();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let bundle =
            RetainedResourceBundle::capture([handle], &texts, &geometries, &fonts).unwrap();
        (
            TransportTextResourceHandle::from_source_handle(handle),
            bundle,
        )
    }

    #[test]
    fn binary_round_trip_preserves_opaque_ids_versions_and_object_identity() {
        let expected = TransportTextResourceHandle {
            id: 0xfedc_ba98_7654_3210,
            version: u64::MAX - 7,
        };
        let (mut replacement, bundle) = replacement_bundle();
        replacement.id = 0x8123_4567_89ab_cdef;
        replacement.version = u64::MAX - 3;
        let envelope = RetainedResourceMutationEnvelope::replace(
            17,
            42,
            ObjectId::new(0x1234_5678),
            expected,
            replacement,
            bundle,
        );

        let bytes = envelope.encode_binary().unwrap();
        let decoded = RetainedResourceMutationEnvelope::decode_binary(&bytes).unwrap();
        assert_eq!(decoded, envelope);
        assert_eq!(decoded.expected, expected);
        assert_eq!(decoded.object, ObjectId::new(0x1234_5678));
        match decoded.operation {
            RetainedResourceMutationOperation::Replace { replacement, .. } => {
                assert_eq!(replacement.id, 0x8123_4567_89ab_cdef);
                assert_eq!(replacement.version, u64::MAX - 3);
            }
            RetainedResourceMutationOperation::Remove => panic!("expected replacement"),
        }
    }

    #[test]
    fn invalid_protocol_header_is_rejected_before_encoding() {
        let mut envelope = RetainedResourceMutationEnvelope::remove(
            1,
            0,
            ObjectId::new(7),
            TransportTextResourceHandle { id: 9, version: 2 },
        );
        envelope.channel = "wrong.channel".to_owned();
        assert_eq!(
            envelope.encode_binary(),
            Err(RetainedResourceMutationTransportError::InvalidChannel(
                "wrong.channel".to_owned()
            ))
        );
    }

    #[test]
    fn sequence_guard_drops_stale_and_rejects_gaps() {
        let expected = TransportTextResourceHandle { id: 9, version: 2 };
        let mut guard = RetainedResourceMutationSequenceGuard::default();
        let first = RetainedResourceMutationEnvelope::remove(5, 0, ObjectId::new(7), expected);
        assert_eq!(
            guard.accept(&first),
            Ok(RetainedResourceMutationSequenceOutcome::Accepted)
        );

        let stale = RetainedResourceMutationEnvelope::remove(5, 0, ObjectId::new(7), expected);
        assert_eq!(
            guard.accept(&stale),
            Ok(RetainedResourceMutationSequenceOutcome::DroppedStale)
        );

        let gap = RetainedResourceMutationEnvelope::remove(5, 2, ObjectId::new(7), expected);
        assert_eq!(
            guard.accept(&gap),
            Err(RetainedResourceMutationTransportError::SequenceGap {
                expected: 1,
                actual: 2,
            })
        );
    }

    #[test]
    fn new_session_requires_sequence_zero() {
        let expected = TransportTextResourceHandle { id: 1, version: 0 };
        let mut guard = RetainedResourceMutationSequenceGuard::default();
        guard
            .accept(&RetainedResourceMutationEnvelope::remove(
                4,
                0,
                ObjectId::new(3),
                expected,
            ))
            .unwrap();

        assert_eq!(
            guard.accept(&RetainedResourceMutationEnvelope::remove(
                8,
                3,
                ObjectId::new(3),
                expected,
            )),
            Err(
                RetainedResourceMutationTransportError::SessionRequiresSequenceZero {
                    session: 8,
                    sequence: 3,
                }
            )
        );
    }

    #[test]
    fn sequence_exhaustion_leaves_guard_unchanged() {
        let expected = TransportTextResourceHandle { id: 2, version: 4 };
        let mut guard = RetainedResourceMutationSequenceGuard {
            session: Some(9),
            next_sequence: u64::MAX,
        };
        let before = guard.clone();
        let exhausted =
            RetainedResourceMutationEnvelope::remove(9, u64::MAX, ObjectId::new(11), expected);

        assert_eq!(
            guard.accept(&exhausted),
            Err(RetainedResourceMutationTransportError::SequenceExhausted)
        );
        assert_eq!(guard, before);
    }
}

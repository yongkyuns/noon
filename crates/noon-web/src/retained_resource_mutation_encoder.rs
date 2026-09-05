use noon_core::ObjectId;

use crate::{
    RetainedResourceBundle, RetainedResourceMutationEnvelope,
    RetainedResourceMutationTransportError, TransportTextResourceHandle,
};

/// Authoritative session/sequence allocator for outbound retained resource mutations.
///
/// Callers provide semantic/resource identities only. Sequence ownership stays here so
/// producers cannot accidentally reuse, skip, or manually diverge from the ordering
/// contract enforced by `RetainedResourceMutationSequenceGuard` on the receiving side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedResourceMutationEncoder {
    session: u32,
    next_sequence: u64,
}

impl RetainedResourceMutationEncoder {
    pub const fn new(session: u32) -> Self {
        Self {
            session,
            next_sequence: 0,
        }
    }

    pub const fn session(&self) -> u32 {
        self.session
    }

    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    pub fn encode_replace(
        &mut self,
        object: ObjectId,
        expected: TransportTextResourceHandle,
        replacement: TransportTextResourceHandle,
        resources: RetainedResourceBundle,
    ) -> Result<RetainedResourceMutationEnvelope, RetainedResourceMutationTransportError> {
        let sequence = self.take_sequence()?;
        Ok(RetainedResourceMutationEnvelope::replace(
            self.session,
            sequence,
            object,
            expected,
            replacement,
            resources,
        ))
    }

    pub fn encode_remove(
        &mut self,
        object: ObjectId,
        expected: TransportTextResourceHandle,
    ) -> Result<RetainedResourceMutationEnvelope, RetainedResourceMutationTransportError> {
        let sequence = self.take_sequence()?;
        Ok(RetainedResourceMutationEnvelope::remove(
            self.session,
            sequence,
            object,
            expected,
        ))
    }

    fn take_sequence(&mut self) -> Result<u64, RetainedResourceMutationTransportError> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RetainedResourceMutationTransportError::SequenceExhausted)?;
        Ok(sequence)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FontResourceArena, GeometryResourceArena, Rect, TextResource, TextResourceArena,
        TextSourceKind, Vec2,
    };

    use super::*;
    use crate::RetainedResourceMutationOperation;

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
    fn encoder_assigns_one_monotonic_sequence_across_operation_kinds() {
        let expected = TransportTextResourceHandle { id: 7, version: 3 };
        let (replacement, resources) = replacement_bundle();
        let mut encoder = RetainedResourceMutationEncoder::new(23);

        let replace = encoder
            .encode_replace(ObjectId::new(41), expected, replacement, resources)
            .unwrap();
        assert_eq!(replace.session, 23);
        assert_eq!(replace.sequence, 0);
        assert_eq!(replace.object, ObjectId::new(41));
        assert_eq!(replace.expected, expected);
        assert!(matches!(
            replace.operation,
            RetainedResourceMutationOperation::Replace { .. }
        ));

        let remove = encoder.encode_remove(ObjectId::new(41), expected).unwrap();
        assert_eq!(remove.session, 23);
        assert_eq!(remove.sequence, 1);
        assert!(matches!(
            remove.operation,
            RetainedResourceMutationOperation::Remove
        ));
        assert_eq!(encoder.next_sequence(), 2);
    }

    #[test]
    fn encoder_output_is_accepted_by_the_matching_sequence_guard() {
        let expected = TransportTextResourceHandle { id: 9, version: 1 };
        let mut encoder = RetainedResourceMutationEncoder::new(5);
        let mut guard = crate::RetainedResourceMutationSequenceGuard::default();

        for object in [ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)] {
            let envelope = encoder.encode_remove(object, expected).unwrap();
            assert_eq!(
                guard.accept(&envelope),
                Ok(crate::RetainedResourceMutationSequenceOutcome::Accepted)
            );
        }
        assert_eq!(encoder.next_sequence(), 3);
    }

    #[test]
    fn sequence_exhaustion_leaves_encoder_unchanged() {
        let expected = TransportTextResourceHandle { id: 2, version: 4 };
        let mut encoder = RetainedResourceMutationEncoder {
            session: 9,
            next_sequence: u64::MAX,
        };
        let before = encoder.clone();

        assert_eq!(
            encoder.encode_remove(ObjectId::new(11), expected),
            Err(RetainedResourceMutationTransportError::SequenceExhausted)
        );
        assert_eq!(encoder, before);
    }
}

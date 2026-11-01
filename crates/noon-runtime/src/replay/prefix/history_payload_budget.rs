//! Saved revision-vector and inline-geometry admission. This is accounting, not a history owner.
use super::{PrefixError, ReplayError};
use noon_compile::CompiledReplayRevision;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HistoryPayloadBudget {
    bytes: usize,
    limit: usize,
}

impl HistoryPayloadBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }

    pub(super) fn bytes(self) -> usize {
        self.bytes
    }

    /// Pure admission; only publication/adoption commits the returned charge.
    /// Outstanding batches count at extraction. Rewind never refunds history.
    pub(super) fn with_additional(self, additional: usize) -> Result<Self, PrefixError> {
        self.bytes
            .checked_add(additional)
            .filter(|&total| total <= self.limit)
            .map(|bytes| Self { bytes, ..self })
            .ok_or(PrefixError::HistoryPayloadBudgetExceeded {
                retained_bytes: self.bytes,
                additional_bytes: additional,
                limit_bytes: self.limit,
            })
    }
}

/// Startup scans once; subsequent calls visit only the new suffix.
pub(super) fn clone_bytes<'a>(
    revisions: impl IntoIterator<Item = &'a CompiledReplayRevision>,
) -> Result<usize, PrefixError> {
    revisions.into_iter().try_fold(0usize, |total, revision| {
        total
            .checked_add(
                revision
                    .prefix_clone_payload_bytes()
                    .ok_or(ReplayError::RetentionLimit)?,
            )
            .ok_or(PrefixError::Replay(ReplayError::RetentionLimit))
    })
}

/// The reported capacity check is separate from the requested clone size.
pub(super) fn retained_bytes<'a>(
    revisions: impl IntoIterator<Item = &'a CompiledReplayRevision>,
) -> Result<usize, PrefixError> {
    revisions.into_iter().try_fold(0usize, |total, revision| {
        total
            .checked_add(
                revision
                    .prefix_payload_bytes()
                    .ok_or(ReplayError::RetentionLimit)?,
            )
            .ok_or(PrefixError::Replay(ReplayError::RetentionLimit))
    })
}

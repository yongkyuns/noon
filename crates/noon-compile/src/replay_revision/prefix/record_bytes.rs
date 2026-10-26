//! Shallow saved-record accounting; immutable content has a separate budget.
use super::super::{CompiledChannelKey, CompiledReplayRevision, CompiledTrack, SavedRow};
use noon_core::TrackId;
use std::mem::size_of;

fn record_bytes(rows: usize, channels: usize, tracks: usize) -> Option<usize> {
    rows.checked_mul(size_of::<SavedRow>())?
        .checked_add(channels.checked_mul(size_of::<CompiledChannelKey>())?)?
        .checked_add(tracks.checked_mul(size_of::<(TrackId, Option<CompiledTrack>)>())?)
}

impl CompiledReplayRevision {
    /// Logical bytes for saved row/channel/track elements, including absent
    /// opposite values. Forward conversion and revision exchange preserve these
    /// element counts, so selecting a different time cannot increase the charge.
    ///
    /// O(1), with checked arithmetic and target-native type sizes. The caller
    /// charges the outer revision table separately. This excludes spare vector
    /// capacity, allocator overhead and indirect Arc/resource/plan allocations.
    /// It is accounting, not capability admission or a process-memory estimate.
    pub fn prefix_record_payload_bytes(&self) -> Option<usize> {
        record_bytes(self.rows.len(), self.channels.len(), self.tracks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_record_kind_is_charged_by_its_native_layout() {
        assert_eq!(record_bytes(0, 0, 0), Some(0));
        assert_eq!(
            record_bytes(2, 3, 4),
            Some(
                2 * size_of::<SavedRow>()
                    + 3 * size_of::<CompiledChannelKey>()
                    + 4 * size_of::<(TrackId, Option<CompiledTrack>)>()
            )
        );
    }

    #[test]
    fn an_absent_opposite_row_still_reserves_its_full_record() {
        let revision = CompiledReplayRevision {
            rows: vec![SavedRow {
                index: 0,
                object: None,
                order: Some(None),
            }],
            channels: Vec::new(),
            tracks: Vec::new(),
            camera_motion_payloads: 0,
            path_motion_commands: 0,
        };
        assert_eq!(
            revision.prefix_record_payload_bytes(),
            Some(size_of::<SavedRow>())
        );
    }

    #[test]
    fn multiplication_and_combined_record_overflow_are_rejected() {
        assert_eq!(record_bytes(usize::MAX, 0, 0), None);
        assert_eq!(record_bytes(0, usize::MAX, 0), None);
        assert_eq!(record_bytes(0, 0, usize::MAX), None);
        let rows = usize::MAX / size_of::<SavedRow>();
        assert!(record_bytes(rows, 0, 0).is_some());
        assert_eq!(record_bytes(rows, 0, size_of::<SavedRow>()), None);
    }
}

//! A finite replay capability over this runtime's existing execution projection.
//!
//! Only local inverse revisions are retained. Seeking exchanges those revisions
//! and evaluates the ordinary runtime; it does not rerun a host program, clone a
//! semantic store or retain a second mutable scene.

use crate::{EvaluationError, EvaluationStats, SceneInstance, TrackGroup};
use noon_compile::{CompilePatchError, CompiledReplayRevision, ExecutionPatch};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayLimits {
    pub revisions: usize,
    /// Number of saved object/channel/track payloads. Shared resource bytes are
    /// owned by the execution's immutable resource arena, not duplicated here.
    pub payloads: usize,
}
impl Default for ReplayLimits {
    fn default() -> Self {
        Self {
            revisions: 100_000,
            payloads: 1_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    AlreadyStarted,
    NotRecording,
    Incomplete,
    UnsupportedDomain,
    /// A live reactive input changed without a recorded input history.
    UnrecordedInput,
    RetentionLimit,
    InvalidRange,
}
impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "replay unavailable: {self:?}")
    }
}
impl std::error::Error for ReplayError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplayStats {
    pub revisions_retained: usize,
    pub payloads_retained: usize,
    pub revisions_crossed: usize,
    pub objects_restored: usize,
    pub channels_restored: usize,
}

#[derive(Clone, Debug)]
struct Revision {
    time: f64,
    inverse: Option<CompiledReplayRevision>,
}

#[derive(Clone, Debug)]
struct RecordedInputChange {
    signal: noon_core::SignalId,
    previous: noon_core::ReactiveValue,
    next: noon_core::ReactiveValue,
}

#[derive(Clone, Debug)]
struct RecordedInputBatch {
    sequence: u64,
    time: f64,
    changes: Vec<RecordedInputChange>,
}
#[derive(Clone, Debug)]
pub(crate) struct ReplayHistory {
    start: f64,
    end: Option<f64>,
    limits: ReplayLimits,
    failure: Option<ReplayError>,
    revisions: Vec<Revision>,
    applied: usize,
    semantic_history: bool,
    inputs: Vec<RecordedInputBatch>,
    inputs_applied: usize,
    stats: ReplayStats,
}

impl SceneInstance {
    pub fn begin_replay_retention(&mut self, limits: ReplayLimits) -> Result<(), ReplayError> {
        if self.replay_history.is_some() {
            return Err(ReplayError::AlreadyStarted);
        }
        let unsupported = self
            .reactive
            .as_ref()
            .is_some_and(|reactive| reactive.has_property_bindings());
        // Existing immutable family channels are already part of this scope's
        // initial projection and use the ordinary scheduler during replay.
        self.replay_history = Some(ReplayHistory {
            start: self.frame.time,
            end: None,
            limits,
            failure: unsupported.then_some(ReplayError::UnsupportedDomain),
            revisions: Vec::new(),
            applied: 0,
            semantic_history: false,
            inputs: Vec::new(),
            inputs_applied: 0,
            stats: ReplayStats::default(),
        });
        Ok(())
    }
    pub fn replay_scope_active(&self) -> bool {
        self.replay_history.is_some()
    }
    pub fn replay_retention_valid(&self) -> bool {
        self.replay_history
            .as_ref()
            .is_some_and(|history| history.failure.is_none())
    }
    pub(crate) fn has_recorded_replay_inputs(&self) -> bool {
        self.replay_history
            .as_ref()
            .is_some_and(|history| !history.inputs.is_empty())
    }
    pub(crate) fn replay_input_recording_active(&self) -> bool {
        self.replay_history
            .as_ref()
            .is_some_and(|history| history.end.is_none() && history.failure.is_none())
    }
    pub(crate) fn can_record_signal_replay_input(&self) -> bool {
        self.replay_history.as_ref().is_some_and(|history| {
            history.end.is_none() && history.failure.is_none() && !history.semantic_history
        }) && self.effective_driver_rows.is_empty()
            && self.effective_content_drivers.is_empty()
            && !self.interactions_active()
            && self
                .reactive
                .as_ref()
                .is_some_and(|reactive| !reactive.has_property_bindings())
    }
    pub fn invalidate_replay_domain(&mut self) {
        self.invalidate_replay(ReplayError::UnsupportedDomain);
    }
    pub(crate) fn invalidate_replay_input(&mut self) {
        self.invalidate_replay(ReplayError::UnrecordedInput);
    }
    fn invalidate_replay(&mut self, reason: ReplayError) {
        if let Some(history) = self
            .replay_history
            .as_mut()
            .filter(|history| history.end.is_none())
        {
            // Keep the first reason a retained range became unavailable.
            history.failure.get_or_insert(reason);
            history.revisions.clear();
            history.applied = 0;
            history.inputs.clear();
            history.inputs_applied = 0;
            history.stats = ReplayStats::default();
        }
    }
    pub fn replay_is_sealed(&self) -> bool {
        self.replay_history
            .as_ref()
            .is_some_and(|history| history.end.is_some())
    }
    pub fn replay_end_time(&self) -> Option<f64> {
        self.replay_history.as_ref().and_then(|history| history.end)
    }

    /// Account for immutable scalar plan extensions in the same finite history budget.
    /// Their original schedule remains resident and time-qualified; there is no copy to exchange.
    pub fn retain_scalar_timeline_change(
        &mut self,
        timeline: &crate::PreparedSignalTimelineAppend,
    ) {
        if timeline.entries.is_empty() {
            return;
        }
        if timeline.runtime != self.runtime_identity() || timeline.current != self.frame.time {
            self.invalidate_replay_domain();
            return;
        }
        if self
            .replay_history
            .as_ref()
            .is_some_and(|history| !history.inputs.is_empty())
        {
            self.invalidate_replay_domain();
            return;
        }
        if let Some(history) = self.replay_history.as_mut() {
            history.semantic_history = true;
        }
        self.retain_supported_replay_change(None, timeline.entries.len());
    }

    pub fn replay_stats(&self) -> ReplayStats {
        self.replay_history
            .as_ref()
            .map_or_default(|history| history.stats)
    }
    pub fn seal_replay(&mut self) -> Result<(), ReplayError> {
        let history = self
            .replay_history
            .as_mut()
            .ok_or(ReplayError::NotRecording)?;
        if let Some(error) = history.failure {
            return Err(error);
        }
        if self
            .reactive
            .as_ref()
            .is_some_and(|reactive| reactive.has_property_bindings())
        {
            return Err(ReplayError::UnsupportedDomain);
        }
        if history.end.is_some() {
            return Ok(());
        }
        if self.frame.time < history.start
            || history
                .revisions
                .last()
                .is_some_and(|revision| revision.time > self.frame.time)
        {
            return Err(ReplayError::InvalidRange);
        }
        history.end = Some(self.frame.time);
        Ok(())
    }
    /// Restore the authored frontier before releasing the finite retained history.
    /// This is an explicit scope-maintenance operation, never an ordinary frame.
    pub fn discard_replay_retention(&mut self) {
        if let Some(end) = self.replay_history.as_ref().and_then(|history| history.end) {
            self.select_replay_revision(end);
            self.seek_unchecked(end);
            self.publish_execution_change();
        }
        self.replay_history = None;
    }
    pub(crate) fn require_replay_writable(&self) -> Result<(), CompilePatchError> {
        if self.replay_is_sealed() {
            Err(CompilePatchError::ReplaySealed)
        } else {
            Ok(())
        }
    }
    pub(crate) fn prepare_replay_change(
        &self,
        patch: &ExecutionPatch,
    ) -> Option<CompiledReplayRevision> {
        self.replay_history
            .as_ref()
            .filter(|h| h.end.is_none() && h.failure.is_none())?;
        if let ExecutionPatch::AddFamilyAnimation(animation) = patch {
            // A resident immutable channel is harmless before its mapped start.
            // Retroactive introduction would change an earlier replay frame,
            // so fail the capability closed rather than rewrite that history.
            let timing = noon_core::TrackTiming::new(
                animation.spec.start_time,
                animation.spec.duration,
                noon_core::RateFunction::Linear,
            );
            let (start, _) =
                noon_core::continuous_time_map_interval(timing, &animation.time_map).ok()?;
            if start < self.frame.time {
                return None;
            }
        }
        self.compiled.prepare_replay_revision(patch)
    }
    pub(crate) fn retain_replay_change(&mut self, change: Option<CompiledReplayRevision>) {
        if self
            .replay_history
            .as_ref()
            .is_some_and(|history| !history.inputs.is_empty())
        {
            // Input and compiled histories have separate cursors. Until they
            // share one total order, do not claim a mixed range can be replayed.
            self.invalidate_replay_domain();
            return;
        }
        match change {
            Some(change) => {
                let cost = change.retention_cost();
                self.retain_supported_replay_change(Some(change), cost);
            }
            None => self.invalidate_replay_domain(),
        }
    }

    pub(crate) fn retain_replay_input(
        &mut self,
        time: f64,
        changes: Vec<(
            noon_core::SignalId,
            noon_core::ReactiveValue,
            noon_core::ReactiveValue,
        )>,
    ) {
        if changes.is_empty() {
            return;
        }
        let Some(history) = self.replay_history.as_mut() else {
            return;
        };
        if history.failure.is_some() || history.end.is_some() {
            return;
        }
        if history.semantic_history {
            self.invalidate_replay_domain();
            return;
        }
        let retained = history.revisions.len().saturating_add(history.inputs.len());
        let payload_cost = changes.len().saturating_mul(2);
        let error = if retained >= history.limits.revisions
            || payload_cost
                > history
                    .limits
                    .payloads
                    .saturating_sub(history.stats.payloads_retained)
        {
            Some(ReplayError::RetentionLimit)
        } else if !time.is_finite()
            || time < history.start
            || history.inputs.last().is_some_and(|input| input.time > time)
            || history
                .revisions
                .last()
                .is_some_and(|revision| revision.time > time)
        {
            Some(ReplayError::InvalidRange)
        } else {
            None
        };
        if let Some(error) = error {
            self.invalidate_replay(error);
            return;
        }
        let Some(sequence) = u64::try_from(history.inputs.len())
            .ok()
            .and_then(|n| n.checked_add(1))
        else {
            self.invalidate_replay(ReplayError::RetentionLimit);
            return;
        };
        history.stats.payloads_retained += payload_cost;
        history.inputs.push(RecordedInputBatch {
            sequence,
            time,
            changes: changes
                .into_iter()
                .map(|(signal, previous, next)| RecordedInputChange {
                    signal,
                    previous,
                    next,
                })
                .collect(),
        });
        history.inputs_applied = history.inputs.len();
        history.stats.revisions_retained = retained + 1;
    }

    fn retain_supported_replay_change(
        &mut self,
        inverse: Option<CompiledReplayRevision>,
        cost: usize,
    ) {
        let Some(history) = self.replay_history.as_mut() else {
            return;
        };
        if history.failure.is_some() || history.end.is_some() {
            return;
        }
        let failure = if history.revisions.len().saturating_add(history.inputs.len())
            >= history.limits.revisions
            || cost
                > history
                    .limits
                    .payloads
                    .saturating_sub(history.stats.payloads_retained)
        {
            Some(ReplayError::RetentionLimit)
        } else if history
            .revisions
            .last()
            .is_some_and(|revision| revision.time > self.frame.time)
        {
            Some(ReplayError::InvalidRange)
        } else {
            None
        };
        if let Some(error) = failure {
            self.invalidate_replay(error);
            return;
        }
        history.stats.payloads_retained += cost;
        history.semantic_history = true;
        history.revisions.push(Revision {
            time: self.frame.time,
            inverse,
        });
        history.applied = history.revisions.len();
        history.stats.revisions_retained = history.revisions.len() + history.inputs.len();
    }
    pub(crate) fn next_replay_revision_time(&self) -> Option<f64> {
        let history = self.replay_history.as_ref()?;
        history.end?;
        let revision_time = history
            .revisions
            .get(history.applied)
            .map(|revision| revision.time);
        let input_time = history
            .inputs
            .get(history.inputs_applied)
            .map(|input| input.time);
        match (revision_time, input_time) {
            (Some(revision), Some(input)) => Some(revision.min(input)),
            (revision, input) => revision.or(input),
        }
    }
    pub(crate) fn validate_replay_time(&self, time: f64) -> Result<(), EvaluationError> {
        if let Some(history) = self.replay_history.as_ref() {
            if let Some(end) = history.end {
                if time < history.start || time > end {
                    return Err(EvaluationError::ReplayTimeOutsideRange {
                        time,
                        start: history.start,
                        end,
                    });
                }
            }
        }
        Ok(())
    }
    /// Returns whether the selected execution projection changed. The scan is over
    /// crossed local revisions, never over the whole retained source history.
    pub(crate) fn select_replay_revision(&mut self, time: f64) -> bool {
        let Some(mut history) = self.replay_history.take() else {
            return false;
        };
        if history.end.is_none() {
            self.replay_history = Some(history);
            return false;
        }
        let target = history
            .revisions
            .partition_point(|revision| revision.time <= time);
        let input_target = history.inputs.partition_point(|input| input.time <= time);
        history.stats.revisions_crossed = history
            .applied
            .abs_diff(target)
            .saturating_add(history.inputs_applied.abs_diff(input_target));
        history.stats.objects_restored = 0;
        history.stats.channels_restored = 0;
        if target == history.applied && input_target == history.inputs_applied {
            self.replay_history = Some(history);
            return false;
        }
        let mut rows = BTreeSet::new();
        let mut channels = BTreeSet::new();
        while history.applied != target {
            let backward = history.applied > target;
            let index = if backward {
                history.applied - 1
            } else {
                history.applied
            };
            if let Some(revision) = &mut history.revisions[index].inverse {
                rows.extend(revision.object_indices());
                channels.extend(revision.channels());
                self.compiled.exchange_replay_revision(revision);
            }
            if backward {
                history.applied -= 1;
            } else {
                history.applied += 1;
            }
        }
        while history.inputs_applied != input_target {
            let forward = history.inputs_applied < input_target;
            let index = if forward {
                history.inputs_applied
            } else {
                history.inputs_applied - 1
            };
            let batch = &history.inputs[index];
            debug_assert_eq!(batch.sequence, index as u64 + 1);
            let changes = batch
                .changes
                .iter()
                .map(|change| {
                    (
                        change.signal,
                        if forward {
                            change.next.clone()
                        } else {
                            change.previous.clone()
                        },
                    )
                })
                .collect::<Vec<_>>();
            if let Some(reactive) = self.reactive.as_mut() {
                let prepared = reactive
                    .prepare_input_batch(&changes)
                    .expect("retained replay inputs were validated against this runtime");
                debug_assert!(prepared.property_changes().is_empty());
                reactive.commit_prepared_input_batch(prepared);
            }
            if forward {
                history.inputs_applied += 1;
            } else {
                history.inputs_applied -= 1;
            }
        }
        history.stats.objects_restored = rows.len();
        history.stats.channels_restored = channels.len();
        for channel in channels {
            let tracks = self.compiled.channel_tracks(channel);
            self.timeline_scheduler.relower_channel(channel, tracks);
            if tracks.is_empty() {
                self.groups.remove(&channel);
            } else {
                self.groups.insert(
                    channel,
                    TrackGroup {
                        channel,
                        cursor: 0,
                        mapped: tracks.iter().any(|track| !track.time_map.is_identity()),
                    },
                );
            }
        }
        let mut evaluation = EvaluationStats::default();
        for index in rows {
            let index = index as usize;
            let was_live = self.painter_ranks[index].is_some();
            let live = self.compiled.object_slot_is_live(index as u32);
            if was_live && !live {
                let position = self.painter_ranks[index].take().expect("live row") as usize;
                self.painter_order.remove(position);
                for rank in position..self.painter_order.len() {
                    self.painter_ranks[self.painter_order[rank] as usize] = Some(rank as u32);
                }
                self.mark_painter_order_changed(position..self.painter_order.len() + 1);
                self.mark_removed(index);
            } else if !was_live && live {
                self.painter_ranks[index] = Some(self.painter_order.len() as u32);
                self.painter_order.push(index as u32);
                self.mark_painter_order_changed(
                    self.painter_order.len() - 1..self.painter_order.len(),
                );
                self.mark_added(index);
            }
            if live {
                self.relower_object(index, time, &mut evaluation);
                self.mark_changed(index);
            } else {
                self.frame.presences[index] = false;
                self.frame.render_geometries[index] = None;
                self.frame.render_transforms[index] = None;
            }
        }
        self.last_stats = evaluation;
        self.replay_history = Some(history);
        self.publish_execution_change();
        true
    }
}

#[cfg(test)]
mod input_tests;

#[cfg(test)]
mod family_tests;

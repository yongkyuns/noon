use std::collections::BTreeMap;

use noon_core::{
    HostCallbackId, HostCallbackReplayClass, HostFrameDisposition, HostPlaybackMode, HostSeekMode,
};

/// Runtime policy for arbitrary host-language callback execution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostExecutionPolicy {
    pub playback_mode: HostPlaybackMode,
    /// Presentation budget assigned to the whole host callback phase.
    /// `None` disables deadline accounting (useful for offline evaluation).
    pub frame_budget_ms: Option<f64>,
}

impl HostExecutionPolicy {
    pub const REALTIME_60HZ: Self = Self {
        playback_mode: HostPlaybackMode::Realtime,
        frame_budget_ms: Some(1000.0 / 60.0),
    };

    pub const REALTIME_120HZ: Self = Self {
        playback_mode: HostPlaybackMode::Realtime,
        frame_budget_ms: Some(1000.0 / 120.0),
    };

    pub const OFFLINE_DETERMINISTIC: Self = Self {
        playback_mode: HostPlaybackMode::DeterministicOffline,
        frame_budget_ms: None,
    };

    pub const fn disposition(self) -> HostFrameDisposition {
        self.playback_mode.frame_disposition()
    }
}

impl Default for HostExecutionPolicy {
    fn default() -> Self {
        Self::REALTIME_60HZ
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HostCallbackTimingStats {
    pub calls: u64,
    pub total_ms: f64,
    pub last_ms: f64,
    pub max_ms: f64,
}

impl HostCallbackTimingStats {
    pub fn average_ms(self) -> f64 {
        if self.calls == 0 {
            0.0
        } else {
            self.total_ms / self.calls as f64
        }
    }

    fn record(&mut self, elapsed_ms: f64) {
        self.calls = self.calls.saturating_add(1);
        self.total_ms += elapsed_ms;
        self.last_ms = elapsed_ms;
        self.max_ms = self.max_ms.max(elapsed_ms);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostPhaseReport {
    pub elapsed_ms: f64,
    pub deadline_missed: bool,
    pub missed_deadlines: u64,
    pub disposition: HostFrameDisposition,
    pub exact_seek_mode: HostSeekMode,
}

/// Metrics and semantic classification for the host compatibility slow path.
///
/// The host owns actual Python/JS invocation and reports measured durations here.
/// Keeping the profiler independent of a particular worker transport lets native,
/// browser, and test hosts share the same deadline/seek contract.
#[derive(Clone, Debug)]
pub struct HostCallbackProfiler {
    policy: HostExecutionPolicy,
    replay_classes: BTreeMap<HostCallbackId, HostCallbackReplayClass>,
    callback_timings: BTreeMap<HostCallbackId, HostCallbackTimingStats>,
    phases: u64,
    missed_deadlines: u64,
    last_phase_ms: f64,
    max_phase_ms: f64,
}

impl HostCallbackProfiler {
    pub fn new(policy: HostExecutionPolicy) -> Self {
        Self {
            policy,
            replay_classes: BTreeMap::new(),
            callback_timings: BTreeMap::new(),
            phases: 0,
            missed_deadlines: 0,
            last_phase_ms: 0.0,
            max_phase_ms: 0.0,
        }
    }

    pub const fn policy(&self) -> HostExecutionPolicy {
        self.policy
    }

    pub fn set_policy(&mut self, policy: HostExecutionPolicy) {
        self.policy = policy;
    }

    pub fn classify_callback(
        &mut self,
        callback: HostCallbackId,
        replay_class: HostCallbackReplayClass,
    ) {
        self.replay_classes.insert(callback, replay_class);
    }

    pub fn record_callback(&mut self, callback: HostCallbackId, elapsed_ms: f64) {
        assert!(
            elapsed_ms.is_finite() && elapsed_ms >= 0.0,
            "callback duration must be finite and non-negative"
        );
        self.callback_timings
            .entry(callback)
            .or_default()
            .record(elapsed_ms);
    }

    pub fn record_phase(&mut self, elapsed_ms: f64) -> HostPhaseReport {
        assert!(
            elapsed_ms.is_finite() && elapsed_ms >= 0.0,
            "host phase duration must be finite and non-negative"
        );
        self.phases = self.phases.saturating_add(1);
        self.last_phase_ms = elapsed_ms;
        self.max_phase_ms = self.max_phase_ms.max(elapsed_ms);
        let deadline_missed = self
            .policy
            .frame_budget_ms
            .is_some_and(|budget| elapsed_ms > budget);
        if deadline_missed {
            self.missed_deadlines = self.missed_deadlines.saturating_add(1);
        }
        HostPhaseReport {
            elapsed_ms,
            deadline_missed,
            missed_deadlines: self.missed_deadlines,
            disposition: self.policy.disposition(),
            exact_seek_mode: self.exact_seek_mode(),
        }
    }

    pub fn callback_timing(&self, callback: HostCallbackId) -> Option<HostCallbackTimingStats> {
        self.callback_timings.get(&callback).copied()
    }

    pub fn phases(&self) -> u64 {
        self.phases
    }

    pub fn missed_deadlines(&self) -> u64 {
        self.missed_deadlines
    }

    pub fn last_phase_ms(&self) -> f64 {
        self.last_phase_ms
    }

    pub fn max_phase_ms(&self) -> f64 {
        self.max_phase_ms
    }

    /// Exact scene seek must satisfy the least-seekable registered callback.
    pub fn exact_seek_mode(&self) -> HostSeekMode {
        self.replay_classes
            .values()
            .copied()
            .map(HostCallbackReplayClass::seek_mode)
            .max_by_key(|mode| match mode {
                HostSeekMode::Direct => 0,
                HostSeekMode::ReplayFromCheckpoint => 1,
                HostSeekMode::ReplayFromInitialization => 2,
            })
            .unwrap_or(HostSeekMode::Direct)
    }
}

impl Default for HostCallbackProfiler {
    fn default() -> Self {
        Self::new(HostExecutionPolicy::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_deadline_miss_never_changes_to_waiting_policy() {
        let mut profiler = HostCallbackProfiler::new(HostExecutionPolicy {
            playback_mode: HostPlaybackMode::Realtime,
            frame_budget_ms: Some(8.0),
        });
        let report = profiler.record_phase(12.5);
        assert!(report.deadline_missed);
        assert_eq!(report.missed_deadlines, 1);
        assert_eq!(
            report.disposition,
            HostFrameDisposition::PresentLatestCommitted
        );
    }

    #[test]
    fn offline_mode_waits_without_deadline_accounting() {
        let mut profiler = HostCallbackProfiler::new(HostExecutionPolicy::OFFLINE_DETERMINISTIC);
        let report = profiler.record_phase(250.0);
        assert!(!report.deadline_missed);
        assert_eq!(report.disposition, HostFrameDisposition::WaitForHostCommit);
    }

    #[test]
    fn exact_seek_mode_uses_least_seekable_callback() {
        let mut profiler = HostCallbackProfiler::default();
        profiler.classify_callback(HostCallbackId::new(1), HostCallbackReplayClass::Pure);
        profiler.classify_callback(
            HostCallbackId::new(2),
            HostCallbackReplayClass::StatefulDeterministic,
        );
        assert_eq!(
            profiler.exact_seek_mode(),
            HostSeekMode::ReplayFromCheckpoint
        );
        profiler.classify_callback(HostCallbackId::new(3), HostCallbackReplayClass::Opaque);
        assert_eq!(
            profiler.exact_seek_mode(),
            HostSeekMode::ReplayFromInitialization
        );
    }

    #[test]
    fn callback_costs_are_reported_per_slot() {
        let mut profiler = HostCallbackProfiler::default();
        let id = HostCallbackId::new(9);
        profiler.record_callback(id, 2.0);
        profiler.record_callback(id, 4.0);
        let stats = profiler.callback_timing(id).unwrap();
        assert_eq!(stats.calls, 2);
        assert_eq!(stats.last_ms, 4.0);
        assert_eq!(stats.max_ms, 4.0);
        assert_eq!(stats.average_ms(), 3.0);
    }
}

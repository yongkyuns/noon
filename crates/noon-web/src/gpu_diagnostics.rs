use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use serde::Serialize;

const GPU_COMPLETION_IN_FLIGHT_LIMIT: usize = 4;
const GPU_COMPLETION_SAMPLE_LIMIT: usize = 32;

/// Host-clock observation of a queue callback, not a GPU timestamp or scanout.
/// Callback dispatch/polling delay is included in the elapsed wall time.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GpuCompletionSample {
    pub session: Option<u32>,
    pub sequence: Option<u64>,
    pub presentation_sequence: u64,
    pub gpu_generation: u32,
    pub submission_started_ms: f64,
    pub completion_observed_ms: f64,
}

#[derive(Default)]
struct GpuCompletionState {
    pending: Vec<u64>,
    dropped: u64,
    failed: u64,
    samples: VecDeque<GpuCompletionSample>,
}

/// Opt-in host telemetry. Pending callbacks and completed observations are both
/// bounded. Retiring this state makes outstanding weak callback tickets inert.
#[derive(Default)]
pub(crate) struct GpuCompletionSamples(Option<Arc<Mutex<GpuCompletionState>>>);

pub(crate) struct GpuCompletionTicket {
    state: Weak<Mutex<GpuCompletionState>>,
    sample: GpuCompletionSample,
}

impl GpuCompletionSamples {
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if enabled {
            self.0.get_or_insert_with(|| {
                Arc::new(Mutex::new(GpuCompletionState {
                    pending: Vec::with_capacity(GPU_COMPLETION_IN_FLIGHT_LIMIT),
                    samples: VecDeque::with_capacity(GPU_COMPLETION_SAMPLE_LIMIT),
                    ..Default::default()
                }))
            });
        } else {
            self.0 = None;
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.0.is_some()
    }

    pub(crate) fn restart(&mut self) {
        if self.is_enabled() {
            self.set_enabled(false);
            self.set_enabled(true);
        }
    }

    pub(crate) fn reserve(&self, sample: GpuCompletionSample) -> Option<GpuCompletionTicket> {
        let state = self.0.as_ref()?;
        let mut data = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !sample.submission_started_ms.is_finite()
            || data.pending.contains(&sample.presentation_sequence)
        {
            data.failed = data.failed.saturating_add(1);
            return None;
        }
        if data.pending.len() == GPU_COMPLETION_IN_FLIGHT_LIMIT {
            data.dropped = data.dropped.saturating_add(1);
            return None;
        }
        data.pending.push(sample.presentation_sequence);
        Some(GpuCompletionTicket {
            state: Arc::downgrade(state),
            sample,
        })
    }

    pub(crate) fn take_json(&self) -> Result<String, serde_json::Error> {
        let Some(state) = &self.0 else {
            return Ok("null".to_owned());
        };
        let mut data = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let json = serde_json::json!({
            "observation": "queue_work_done_callback",
            "includesCallbackDispatchDelay": true,
            "callbackReportsSuccess": false,
            "gpuDurationMeasured": false,
            "displayScanoutMeasured": false,
            "inFlight": data.pending.len(),
            "dropped": data.dropped,
            "failed": data.failed,
            "samples": data.samples,
        });
        let json = serde_json::to_string(&json)?;
        data.samples.clear();
        Ok(json)
    }
}

impl GpuCompletionTicket {
    pub(crate) fn complete(mut self, observed_ms: f64, device_lost: bool) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut data = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        data.pending
            .retain(|serial| *serial != self.sample.presentation_sequence);
        if device_lost
            || !observed_ms.is_finite()
            || observed_ms < self.sample.submission_started_ms
        {
            data.failed = data.failed.saturating_add(1);
            return;
        }
        self.sample.completion_observed_ms = observed_ms;
        if data.samples.len() == GPU_COMPLETION_SAMPLE_LIMIT {
            data.samples.pop_front();
            data.dropped = data.dropped.saturating_add(1);
        }
        data.samples.push_back(self.sample);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GpuDiagnosticKind {
    Validation,
    OutOfMemory,
    Internal,
}

impl GpuDiagnosticKind {
    const fn severity(self) -> GpuDiagnosticSeverity {
        match self {
            Self::Validation => GpuDiagnosticSeverity::Recoverable,
            Self::OutOfMemory | Self::Internal => GpuDiagnosticSeverity::Fatal,
        }
    }

    const fn is_fatal(self) -> bool {
        matches!(self.severity(), GpuDiagnosticSeverity::Fatal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GpuDiagnosticSeverity {
    Recoverable,
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct GpuDiagnostic {
    pub generation: u32,
    pub backend: String,
    pub kind: GpuDiagnosticKind,
    pub severity: GpuDiagnosticSeverity,
    pub message: String,
}

impl GpuDiagnostic {
    fn new(
        generation: u32,
        backend: impl Into<String>,
        kind: GpuDiagnosticKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            generation,
            backend: backend.into(),
            kind,
            severity: kind.severity(),
            message: message.into(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn from_wgpu(generation: u32, backend: wgpu::Backend, error: wgpu::Error) -> Self {
        let kind = match &error {
            wgpu::Error::Validation { .. } => GpuDiagnosticKind::Validation,
            wgpu::Error::OutOfMemory { .. } => GpuDiagnosticKind::OutOfMemory,
            wgpu::Error::Internal { .. } => GpuDiagnosticKind::Internal,
        };
        let backend = match backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU".to_owned(),
            wgpu::Backend::Gl => "WebGL2".to_owned(),
            other => format!("{other:?}"),
        };
        Self::new(generation, backend, kind, error.to_string())
    }

    pub(crate) fn is_fatal(&self) -> bool {
        self.kind.is_fatal()
    }
}

/// Bounded, generation-aware handoff from wgpu's asynchronous error and
/// device-loss callbacks.
///
/// At most one diagnostic is retained. A newer GPU generation replaces an older
/// pending diagnostic, and a fatal diagnostic may replace a recoverable diagnostic
/// from the same generation. Device loss is tracked separately because it requires
/// replacing platform GPU state rather than surfacing a validation diagnostic.
#[derive(Clone, Default)]
pub(crate) struct GpuDiagnosticMailbox {
    pending: Arc<Mutex<Option<GpuDiagnostic>>>,
    lost_generation: Arc<Mutex<Option<u32>>>,
}

impl GpuDiagnosticMailbox {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn record_wgpu(&self, generation: u32, backend: wgpu::Backend, error: wgpu::Error) {
        self.record(GpuDiagnostic::from_wgpu(generation, backend, error));
    }

    fn record(&self, diagnostic: GpuDiagnostic) {
        self.with_slot(|pending| {
            let replace = match pending.as_ref() {
                None => true,
                Some(current) if current.generation < diagnostic.generation => true,
                Some(current) if current.generation > diagnostic.generation => false,
                Some(current) => !current.is_fatal() && diagnostic.is_fatal(),
            };
            if replace {
                *pending = Some(diagnostic);
            }
        });
    }

    pub(crate) fn take_for_generation(&self, generation: u32) -> Option<GpuDiagnostic> {
        self.with_slot(|pending| {
            let pending_generation = pending.as_ref().map(|diagnostic| diagnostic.generation);
            match pending_generation {
                Some(current) if current < generation => {
                    pending.take();
                    None
                }
                Some(current) if current == generation => pending.take(),
                _ => None,
            }
        })
    }

    pub(crate) fn record_device_loss(&self, generation: u32) {
        self.with_lost_generation(|pending| {
            if pending.is_none_or(|current| generation > current) {
                *pending = Some(generation);
            }
        });
    }

    pub(crate) fn device_loss_pending(&self, generation: u32) -> bool {
        self.with_lost_generation(|pending| match *pending {
            Some(current) if current < generation => {
                *pending = None;
                false
            }
            Some(current) => current == generation,
            None => false,
        })
    }

    pub(crate) fn clear_device_loss(&self, generation: u32) {
        self.with_lost_generation(|pending| {
            if *pending == Some(generation) {
                *pending = None;
            }
        });
    }

    fn with_slot<R>(&self, operation: impl FnOnce(&mut Option<GpuDiagnostic>) -> R) -> R {
        let mut pending = self.lock();
        operation(&mut pending)
    }

    fn with_lost_generation<R>(&self, operation: impl FnOnce(&mut Option<u32>) -> R) -> R {
        let mut pending = self
            .lost_generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation(&mut pending)
    }

    fn lock(&self) -> MutexGuard<'_, Option<GpuDiagnostic>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn install_wgpu_error_handler(
    device: &wgpu::Device,
    generation: u32,
    backend: wgpu::Backend,
    mailbox: GpuDiagnosticMailbox,
) {
    let loss_mailbox = mailbox.clone();
    device.set_device_lost_callback(move |_reason, _message| {
        loss_mailbox.record_device_loss(generation);
    });
    device.on_uncaptured_error(Arc::new(move |error| {
        mailbox.record_wgpu(generation, backend, error);
    }));
}

#[cfg(test)]
mod tests {
    use super::{
        GpuCompletionSample, GpuCompletionSamples, GpuDiagnostic, GpuDiagnosticKind,
        GpuDiagnosticMailbox, GpuDiagnosticSeverity, GPU_COMPLETION_IN_FLIGHT_LIMIT,
        GPU_COMPLETION_SAMPLE_LIMIT,
    };

    fn completion_sample(serial: u64) -> GpuCompletionSample {
        GpuCompletionSample {
            session: Some(7),
            sequence: Some(12),
            presentation_sequence: serial,
            gpu_generation: 2,
            submission_started_ms: 10.0,
            completion_observed_ms: 0.0,
        }
    }

    #[test]
    fn completion_observations_are_opt_in_bounded_and_drain_without_retiring_pending() {
        let mut samples = GpuCompletionSamples::default();
        assert!(!samples.is_enabled());
        assert!(samples.reserve(completion_sample(0)).is_none());
        assert_eq!(samples.take_json().unwrap(), "null");
        samples.set_enabled(true);
        let pending: Vec<_> = (0..GPU_COMPLETION_IN_FLIGHT_LIMIT as u64)
            .map(|serial| samples.reserve(completion_sample(serial)).unwrap())
            .collect();
        assert!(samples.reserve(completion_sample(100)).is_none());
        let metrics: serde_json::Value =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(metrics["inFlight"], GPU_COMPLETION_IN_FLIGHT_LIMIT);
        assert_eq!(metrics["dropped"], 1);
        for ticket in pending {
            ticket.complete(12.0, false);
        }
        for serial in 4..(GPU_COMPLETION_SAMPLE_LIMIT as u64 + 5) {
            samples
                .reserve(completion_sample(serial))
                .unwrap()
                .complete(13.0, false);
        }
        let metrics: serde_json::Value =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(metrics["inFlight"], 0);
        assert_eq!(
            metrics["samples"].as_array().unwrap().len(),
            GPU_COMPLETION_SAMPLE_LIMIT
        );
        assert_eq!(metrics["samples"][0]["presentationSequence"], 5);
        assert_eq!(metrics["samples"][0]["session"], 7);
        assert_eq!(metrics["samples"][0]["sequence"], 12);
        assert_eq!(metrics["samples"][0]["gpuGeneration"], 2);
        assert_eq!(metrics["gpuDurationMeasured"], false);
        assert_eq!(metrics["displayScanoutMeasured"], false);
        let drained: serde_json::Value =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(drained["samples"], serde_json::json!([]));
        assert_eq!(drained["dropped"], metrics["dropped"]);
    }

    #[test]
    fn completion_observations_reject_loss_invalid_clocks_and_old_generation_tickets() {
        let mut samples = GpuCompletionSamples::default();
        samples.set_enabled(true);
        let old = samples.reserve(completion_sample(0)).unwrap();
        samples.restart();
        old.complete(12.0, false);
        samples
            .reserve(completion_sample(1))
            .unwrap()
            .complete(12.0, true);
        for (serial, observed) in [(2, f64::NAN), (3, f64::INFINITY), (4, 9.0)] {
            samples
                .reserve(completion_sample(serial))
                .unwrap()
                .complete(observed, false);
        }
        let pending = samples.reserve(completion_sample(5)).unwrap();
        assert!(samples.reserve(completion_sample(5)).is_none());
        let mut invalid = completion_sample(6);
        invalid.submission_started_ms = f64::NAN;
        assert!(samples.reserve(invalid).is_none());
        let metrics: serde_json::Value =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(metrics["failed"], 6);
        assert_eq!(metrics["inFlight"], 1);
        assert_eq!(metrics["samples"], serde_json::json!([]));
        samples.set_enabled(false);
        samples.set_enabled(true);
        pending.complete(12.0, false);
        let metrics: serde_json::Value =
            serde_json::from_str(&samples.take_json().unwrap()).unwrap();
        assert_eq!(metrics["inFlight"], 0);
        assert_eq!(metrics["failed"], 0);
        assert_eq!(metrics["samples"], serde_json::json!([]));
    }

    fn diagnostic(generation: u32, kind: GpuDiagnosticKind, message: &str) -> GpuDiagnostic {
        GpuDiagnostic::new(generation, "WebGPU", kind, message)
    }

    #[test]
    fn validation_is_recoverable_and_one_shot() {
        let mailbox = GpuDiagnosticMailbox::default();
        mailbox.record(diagnostic(4, GpuDiagnosticKind::Validation, "first"));
        mailbox.record(diagnostic(4, GpuDiagnosticKind::Validation, "second"));

        let captured = mailbox.take_for_generation(4).expect("diagnostic");
        assert_eq!(captured.message, "first");
        assert_eq!(captured.severity, GpuDiagnosticSeverity::Recoverable);
        assert!(mailbox.take_for_generation(4).is_none());
    }

    #[test]
    fn fatal_same_generation_replaces_pending_validation() {
        let mailbox = GpuDiagnosticMailbox::default();
        mailbox.record(diagnostic(7, GpuDiagnosticKind::Validation, "recoverable"));
        mailbox.record(diagnostic(7, GpuDiagnosticKind::OutOfMemory, "fatal"));
        mailbox.record(diagnostic(
            7,
            GpuDiagnosticKind::Validation,
            "late validation",
        ));

        let captured = mailbox.take_for_generation(7).expect("fatal diagnostic");
        assert_eq!(captured.message, "fatal");
        assert_eq!(captured.severity, GpuDiagnosticSeverity::Fatal);
        assert!(mailbox.take_for_generation(7).is_none());
    }

    #[test]
    fn newer_generation_replaces_older_and_stale_cannot_replace_future() {
        let mailbox = GpuDiagnosticMailbox::default();
        mailbox.record(diagnostic(2, GpuDiagnosticKind::Internal, "old fatal"));
        mailbox.record(diagnostic(3, GpuDiagnosticKind::Validation, "current"));
        mailbox.record(diagnostic(2, GpuDiagnosticKind::Internal, "stale"));

        assert!(mailbox.take_for_generation(2).is_none());
        let captured = mailbox.take_for_generation(3).expect("current diagnostic");
        assert_eq!(captured.generation, 3);
        assert_eq!(captured.message, "current");
    }

    #[test]
    fn validation_burst_remains_bounded() {
        let mailbox = GpuDiagnosticMailbox::default();
        for index in 0..10_000 {
            mailbox.record(diagnostic(
                1,
                GpuDiagnosticKind::Validation,
                &format!("validation {index}"),
            ));
        }

        assert!(mailbox.take_for_generation(1).is_some());
        assert!(mailbox.take_for_generation(1).is_none());
    }

    #[test]
    fn device_loss_is_generation_aware_and_cleared_after_recovery() {
        let mailbox = GpuDiagnosticMailbox::default();
        mailbox.record_device_loss(2);
        assert!(mailbox.device_loss_pending(2));
        assert!(!mailbox.device_loss_pending(1));

        mailbox.record_device_loss(3);
        assert!(!mailbox.device_loss_pending(2));
        assert!(mailbox.device_loss_pending(3));

        mailbox.clear_device_loss(3);
        assert!(!mailbox.device_loss_pending(3));
    }
}

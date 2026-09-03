use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

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

/// Bounded, generation-aware handoff from wgpu's asynchronous error callback.
///
/// At most one diagnostic is retained. A newer GPU generation replaces an older
/// pending diagnostic, and a fatal diagnostic may replace a recoverable diagnostic
/// from the same generation. This prevents validation-error bursts from growing
/// memory or delaying an out-of-memory/internal failure behind recoverable noise.
#[derive(Clone, Default)]
pub(crate) struct GpuDiagnosticMailbox {
    pending: Arc<Mutex<Option<GpuDiagnostic>>>,
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

    fn with_slot<R>(&self, operation: impl FnOnce(&mut Option<GpuDiagnostic>) -> R) -> R {
        let mut pending = self.lock();
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
    device.on_uncaptured_error(Arc::new(move |error| {
        mailbox.record_wgpu(generation, backend, error);
    }));
}

#[cfg(test)]
mod tests {
    use super::{GpuDiagnostic, GpuDiagnosticKind, GpuDiagnosticMailbox, GpuDiagnosticSeverity};

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
}

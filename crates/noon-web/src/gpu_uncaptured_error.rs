use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GpuUncapturedErrorKind {
    Validation,
    OutOfMemory,
    Internal,
}

impl GpuUncapturedErrorKind {
    #[cfg(target_arch = "wasm32")]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::OutOfMemory => "out-of-memory",
            Self::Internal => "internal",
        }
    }

    pub(crate) const fn is_fatal(self) -> bool {
        !matches!(self, Self::Validation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct GpuUncapturedError {
    pub(crate) generation: u32,
    pub(crate) kind: GpuUncapturedErrorKind,
    pub(crate) fatal: bool,
    pub(crate) message: String,
}

#[cfg(target_arch = "wasm32")]
impl GpuUncapturedError {
    pub(crate) fn from_wgpu(generation: u32, backend: wgpu::Backend, error: wgpu::Error) -> Self {
        let kind = match &error {
            wgpu::Error::Validation { .. } => GpuUncapturedErrorKind::Validation,
            wgpu::Error::OutOfMemory { .. } => GpuUncapturedErrorKind::OutOfMemory,
            wgpu::Error::Internal { .. } => GpuUncapturedErrorKind::Internal,
        };
        let backend = match backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL2",
            _ => "GPU",
        };
        Self {
            generation,
            kind,
            fatal: kind.is_fatal(),
            message: format!(
                "{backend} generation {generation} {} error: {error}",
                kind.label()
            ),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct GpuUncapturedErrorSlot {
    pending: Arc<Mutex<Option<GpuUncapturedError>>>,
}

impl GpuUncapturedErrorSlot {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn report(&self, generation: u32, backend: wgpu::Backend, error: wgpu::Error) {
        self.report_captured(GpuUncapturedError::from_wgpu(generation, backend, error));
    }

    pub(crate) fn take(&self, generation: u32) -> Option<GpuUncapturedError> {
        self.with_pending(|pending| {
            let pending_generation = pending.as_ref().map(|error| error.generation);
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

    fn report_captured(&self, captured: GpuUncapturedError) {
        self.with_pending(|pending| {
            let replace = match pending.as_ref() {
                None => true,
                Some(current) if current.generation < captured.generation => true,
                Some(current) if current.generation > captured.generation => false,
                Some(current) => !current.fatal && captured.fatal,
            };
            if replace {
                *pending = Some(captured);
            }
        });
    }

    fn with_pending<R>(&self, operation: impl FnOnce(&mut Option<GpuUncapturedError>) -> R) -> R {
        let mut pending = self.lock();
        operation(&mut pending)
    }

    fn lock(&self) -> MutexGuard<'_, Option<GpuUncapturedError>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn install_uncaptured_error_handler(
    device: &wgpu::Device,
    generation: u32,
    backend: wgpu::Backend,
    slot: GpuUncapturedErrorSlot,
) {
    device.on_uncaptured_error(Arc::new(move |error| {
        slot.report(generation, backend, error);
    }));
}

#[cfg(test)]
mod tests {
    use super::{GpuUncapturedError, GpuUncapturedErrorKind, GpuUncapturedErrorSlot};

    fn captured(
        generation: u32,
        kind: GpuUncapturedErrorKind,
        message: &str,
    ) -> GpuUncapturedError {
        GpuUncapturedError {
            generation,
            kind,
            fatal: kind.is_fatal(),
            message: message.to_owned(),
        }
    }

    #[test]
    fn validation_is_one_shot_and_recoverable() {
        let slot = GpuUncapturedErrorSlot::default();
        slot.report_captured(captured(
            4,
            GpuUncapturedErrorKind::Validation,
            "validation",
        ));

        let error = slot.take(4).expect("validation error");
        assert_eq!(error.generation, 4);
        assert_eq!(error.kind, GpuUncapturedErrorKind::Validation);
        assert!(!error.fatal);
        assert!(slot.take(4).is_none());
    }

    #[test]
    fn fatal_error_replaces_validation_in_the_same_generation() {
        let slot = GpuUncapturedErrorSlot::default();
        slot.report_captured(captured(
            2,
            GpuUncapturedErrorKind::Validation,
            "validation",
        ));
        slot.report_captured(captured(2, GpuUncapturedErrorKind::OutOfMemory, "oom"));
        slot.report_captured(captured(
            2,
            GpuUncapturedErrorKind::Validation,
            "later validation",
        ));

        let error = slot.take(2).expect("fatal error");
        assert_eq!(error.kind, GpuUncapturedErrorKind::OutOfMemory);
        assert!(error.fatal);
        assert_eq!(error.message, "oom");
    }

    #[test]
    fn stale_error_is_dropped_without_consuming_future_generation() {
        let slot = GpuUncapturedErrorSlot::default();
        slot.report_captured(captured(3, GpuUncapturedErrorKind::Validation, "old"));
        assert!(slot.take(4).is_none());

        slot.report_captured(captured(5, GpuUncapturedErrorKind::Internal, "future"));
        assert!(slot.take(4).is_none());
        let future = slot.take(5).expect("future error");
        assert_eq!(future.message, "future");
        assert!(future.fatal);
    }

    #[test]
    fn older_generation_cannot_replace_newer_pending_error() {
        let slot = GpuUncapturedErrorSlot::default();
        slot.report_captured(captured(8, GpuUncapturedErrorKind::Validation, "new"));
        slot.report_captured(captured(7, GpuUncapturedErrorKind::Internal, "old fatal"));

        let error = slot.take(8).expect("new generation error");
        assert_eq!(error.message, "new");
        assert!(!error.fatal);
    }
}

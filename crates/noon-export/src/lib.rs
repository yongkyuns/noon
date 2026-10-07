//! Native, windowless frame capture over Noon's shared execution and renderer.
//!
//! This crate is the native GPU/output dependency boundary: it does not depend
//! on the window host, Python, WASM, or browser infrastructure. Scheduling and
//! continuation semantics remain in `noon::integration::ExportFrames`; retained
//! preparation and composition remain in `noon-render-wgpu`.
//!
//! The first implementation is deliberately serial. One GPU target, one staging
//! buffer and one reusable CPU pixel buffer belong to a run. The consumer borrows
//! each completed image; retaining images is an explicit consumer allocation.
//! A successful capture return certifies consumption, not encoder finalization.
//! The optional installed-FFmpeg file adapters in [`output`] also finalize output.

#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

mod gpu;
pub mod output;

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use noon::integration::{
    ExportFrame, ExportFrameOptions, ExportFrameSummary, ExportFrames, ExportFramesError,
    ExportFramesStatus, PixelReadbackError, Rgba8ReadbackLayout, SampleObservation,
};
use noon::{LiveContinuation, LiveProgram, RustHostCallbackTable};

pub use wgpu::{AdapterInfo, Backends};

/// Cooperative cancellation, including while replaying a discarded prefix.
/// It cannot preempt synchronous user code or an in-progress driver call.
#[derive(Clone, Debug, Default)]
pub struct CaptureCancellation(Arc<AtomicBool>);

impl CaptureCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Native capture configuration. Output size is independent of display/DPR.
#[derive(Clone, Debug)]
pub struct CaptureOptions {
    pub width: u32,
    pub height: u32,
    /// Bound on the padded staging buffer; not a total scene/GPU memory budget.
    pub max_readback_bytes: u64,
    /// Finite bound for each submitted GPU wait, not arbitrary source execution.
    pub gpu_wait_timeout: Duration,
    /// This native host supports Vulkan, Metal and DX12; no surface is created.
    pub backends: Backends,
    pub force_fallback_adapter: bool,
    /// Opaque clear RGB, in the production renderer's UNORM color convention.
    pub background: [f64; 3],
    pub cancellation: CaptureCancellation,
}

impl CaptureOptions {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            max_readback_bytes: 256 * 1024 * 1024,
            gpu_wait_timeout: Duration::from_secs(30),
            backends: Backends::VULKAN | Backends::METAL | Backends::DX12,
            force_fallback_adapter: false,
            background: [0.0; 3],
            cancellation: CaptureCancellation::default(),
        }
    }

    fn layout(&self) -> Result<Rgba8ReadbackLayout, CaptureError> {
        let supported = Backends::VULKAN | Backends::METAL | Backends::DX12;
        if self.backends.is_empty() || !supported.contains(self.backends) {
            return Err(CaptureError::Configuration("unsupported capture backend"));
        }
        if self.gpu_wait_timeout.is_zero() {
            return Err(CaptureError::Configuration(
                "GPU wait timeout must be positive",
            ));
        }
        if self
            .background
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(CaptureError::Configuration(
                "background RGB must be finite in [0, 1]",
            ));
        }
        Rgba8ReadbackLayout::new(
            self.width,
            self.height,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
            self.max_readback_bytes,
        )
        .map_err(CaptureError::Pixels)
    }
}

/// Output has tightly packed, top-down RGBA8 rows and opaque alpha. These are
/// direct Rgba8Unorm renderer bytes, not a claim of an HDR/linear/sRGB conversion.
/// A video sink must explicitly choose and validate its color conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePixelFormat {
    RendererRgba8UnormOpaque,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureWork {
    pub geometry_instances_repacked: usize,
    /// Retained scene upload only; excludes transient/spatial staging.
    pub retained_upload_bytes: usize,
}

/// Borrowed pixels from precisely one scheduled output sample. Endpoint and
/// discarded-prefix renders never reach this consumer interface.
#[derive(Debug)]
pub struct CapturedFrame<'a> {
    pub frame: ExportFrame,
    pub observation: SampleObservation,
    pub width: u32,
    pub height: u32,
    pub format: CapturePixelFormat,
    pub rgba: &'a [u8],
    pub work: CaptureWork,
}

#[derive(Debug)]
pub struct CaptureSummary {
    pub sampling: ExportFrameSummary,
    pub adapter: AdapterInfo,
    pub rendered_publications: u64,
    pub readbacks: u64,
    pub staging_buffer_bytes: usize,
    pub pixel_buffer_capacity: usize,
}

/// Run a fresh Rust live program without a window or a realtime clock.
///
/// Source/callbacks and the consumer execute on this caller's thread; none must
/// implement `Send`. Consumer latency provides backpressure, never skipped time.
/// On any error the run is abandoned, not retried against consumed publications.
/// Cancellation is checked between cooperative steps and GPU operations. Device
/// creation, synchronous user code and consumer calls are not preemptible.
///
/// The renderer is new for each run, preventing retained resources from being
/// accidentally paired with an unrelated session. Resources persist across all
/// frames *within* the run. The consumer must finalize any file/encoder itself.
pub fn capture_frames<C, F, E>(
    program: &mut LiveProgram<C>,
    callbacks: &mut RustHostCallbackTable,
    frame_options: ExportFrameOptions,
    options: CaptureOptions,
    mut consume: F,
) -> Result<CaptureSummary, CaptureRunError<C::Error, E>>
where
    C: LiveContinuation,
    F: FnMut(CapturedFrame<'_>) -> Result<(), E>,
{
    let layout = options.layout().map_err(CaptureRunError::Capture)?;
    let mut export =
        ExportFrames::new(program, callbacks, frame_options).map_err(CaptureRunError::Sampling)?;
    let result = (|| {
        if options.cancellation.is_cancelled() {
            return Err(CaptureRunError::Cancelled);
        }
        let mut gpu = pollster::block_on(gpu::NativeCapture::new(&options, layout))
            .map_err(CaptureRunError::Capture)?;
        loop {
            if options.cancellation.is_cancelled() {
                return Err(CaptureRunError::Cancelled);
            }
            match export.advance().map_err(CaptureRunError::Sampling)? {
                ExportFramesStatus::Progress => {}
                ExportFramesStatus::PublicationPending(context) => {
                    capture_publication(&mut gpu, &mut export, &options, context, false)?;
                    export
                        .admit_endpoint(context)
                        .map_err(CaptureRunError::Sampling)?;
                }
                ExportFramesStatus::SampleReady(sample) => {
                    let work = capture_publication(
                        &mut gpu,
                        &mut export,
                        &options,
                        sample.observation.publication,
                        sample.frame.is_some(),
                    )?;
                    if options.cancellation.is_cancelled() {
                        return Err(CaptureRunError::Cancelled);
                    }
                    if let Some(frame) = sample.frame {
                        consume(CapturedFrame {
                            frame,
                            observation: sample.observation,
                            width: options.width,
                            height: options.height,
                            format: CapturePixelFormat::RendererRgba8UnormOpaque,
                            rgba: gpu.pixels(),
                            work,
                        })
                        .map_err(CaptureRunError::Consumer)?;
                    }
                    export
                        .acknowledge_sample(sample)
                        .map_err(CaptureRunError::Sampling)?;
                }
                ExportFramesStatus::Complete(sampling) => {
                    return Ok(gpu.summary(sampling));
                }
            }
        }
    })();
    if result.is_err() {
        export.cancel();
    }
    result
}

// The same view/publication path handles endpoints, discarded samples and output.
fn capture_publication<C: LiveContinuation, E>(
    gpu: &mut gpu::NativeCapture,
    export: &mut ExportFrames<'_, C>,
    options: &CaptureOptions,
    expected: noon_core::PublicationContext,
    read_pixels: bool,
) -> Result<CaptureWork, CaptureRunError<C::Error, E>> {
    let view =
        gpu::CaptureView::new(export.session(), options).map_err(CaptureRunError::Capture)?;
    let queries = view
        .bounds
        .iter()
        .copied()
        .map(|bounds| export.query_viewport(bounds))
        .collect::<Vec<_>>();
    let visibility = export
        .session()
        .renderer_viewport_query_union(queries)
        .map_err(|e| CaptureRunError::Capture(CaptureError::View(e.to_string())))?;
    let publication = export
        .take_renderer_publication()
        .map_err(CaptureRunError::Sampling)?;
    if publication.context() != expected {
        return Err(CaptureRunError::Capture(CaptureError::PublicationMismatch));
    }
    gpu.render(
        &publication,
        &view,
        visibility.object_indices(),
        read_pixels,
    )
    .map_err(CaptureRunError::Capture)
}

#[derive(Debug)]
pub enum CaptureError {
    Configuration(&'static str),
    Pixels(PixelReadbackError),
    Gpu(String),
    View(String),
    PublicationMismatch,
}

impl CaptureError {
    fn gpu(error: impl fmt::Display) -> Self {
        Self::Gpu(error.to_string())
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "capture configuration: {message}"),
            Self::Pixels(error) => error.fmt(f),
            Self::Gpu(message) => write!(f, "capture GPU failure: {message}"),
            Self::View(message) => write!(f, "capture view failure: {message}"),
            Self::PublicationMismatch => {
                f.write_str("capture publication changed before consumption")
            }
        }
    }
}

impl Error for CaptureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Pixels(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum CaptureRunError<C, E> {
    Sampling(ExportFramesError<C>),
    Capture(CaptureError),
    Consumer(E),
    Cancelled,
}

impl<C: fmt::Display, E: fmt::Display> fmt::Display for CaptureRunError<C, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sampling(error) => error.fmt(f),
            Self::Capture(error) => error.fmt(f),
            Self::Consumer(error) => write!(f, "capture consumer failed: {error}"),
            Self::Cancelled => f.write_str("capture was cancelled"),
        }
    }
}

impl<C: Error + 'static, E: Error + 'static> Error for CaptureRunError<C, E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sampling(error) => Some(error),
            Self::Capture(error) => Some(error),
            Self::Consumer(error) => Some(error),
            Self::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests;

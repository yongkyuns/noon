//! Capture a settled publication owned by a native language host.
//!
//! No frame clock, source continuation, callback loop or file encoder lives here.
//! This is another entry point to the same native capture implementation, not a
//! renderer for Python or another representation of the scene.

use std::{error::Error, fmt};

use noon::integration::{ExecutionViewportQuery, RendererPublication, RuntimeIdentity};
use noon::ExecutionSession;
use noon_core::PublicationContext;

use crate::{gpu, CaptureError, CaptureOptions, CapturePixelFormat, CaptureSummary, CaptureWork};

/// One completed render of an existing runtime publication. This does not admit
/// an animation endpoint, advance its source, or finalize an output file.
#[derive(Clone, Copy, Debug)]
pub struct CaptureReceipt {
    pub publication: PublicationContext,
    pub published_time: f64,
    pub work: CaptureWork,
}

/// Pixels from the current session state. No requested time or video PTS is
/// invented: the shared sample driver supplies those at the output boundary.
#[derive(Debug)]
pub struct SessionFrame<'a> {
    pub receipt: CaptureReceipt,
    pub width: u32,
    pub height: u32,
    pub format: CapturePixelFormat,
    pub rgba: &'a [u8],
}

/// Retained native capture resources bound to exactly one runtime identity.
///
/// Native language bindings may own an `ExecutionSession` rather than a Rust
/// `LiveProgram`. They can render its settled publications here without moving
/// it into another program or serializing it. `capture_frames` uses the same
/// `gpu::NativeCapture` implementation underneath its Rust source driver.
///
/// This API does not sample time or invoke/complete callbacks. Its caller owns
/// the shared runtime's continuation and publication-admission protocol. Calling
/// it repeatedly renders the same state, not a series of animation frames.
/// This is an explicitly visual capture API: authored audio is neither played
/// nor encoded. [`crate::output::export_session_file`] combines this capture
/// boundary with shared mixing and qualified muxing for finite immutable scenes.
/// Failure poisons this capture object; it cannot replay a consumed publication.
pub struct SessionCapture {
    gpu: gpu::NativeCapture,
    options: CaptureOptions,
    runtime: RuntimeIdentity,
    failed: bool,
}

impl SessionCapture {
    /// Allocate one target, staging buffer and retained renderer for this owner.
    /// The session must be settled, but need not still be at time zero.
    pub fn new(
        session: &ExecutionSession,
        options: CaptureOptions,
    ) -> Result<Self, SessionCaptureError> {
        check_settled(session, &options)?;
        let capture = Self::allocate(session.runtime_identity(), options)?;
        check_settled(session, &capture.options)?;
        Ok(capture)
    }

    /// Allocate the same retained capture path for an already-adopted terminal
    /// history reader. No mutable session accessor or reconstructed source is used.
    #[cfg(feature = "prepared-prefix")]
    pub fn new_prefix(
        reader: &noon::PrefixReader,
        options: CaptureOptions,
    ) -> Result<Self, SessionCaptureError> {
        check_prefix(reader, &options)?;
        let capture = Self::allocate(reader.runtime_identity(), options)?;
        check_prefix(reader, &capture.options)?;
        Ok(capture)
    }

    fn allocate(
        runtime: RuntimeIdentity,
        options: CaptureOptions,
    ) -> Result<Self, SessionCaptureError> {
        let layout = options.layout().map_err(SessionCaptureError::Capture)?;
        let gpu = pollster::block_on(gpu::NativeCapture::new(&options, layout))
            .map_err(SessionCaptureError::Capture)?;
        Ok(Self {
            gpu,
            options,
            runtime,
            failed: false,
        })
    }

    /// Render/consume a required publication without reading or emitting pixels.
    /// Useful for off-grid endpoint barriers; it is not an extra video frame.
    pub fn render(
        &mut self,
        session: &mut ExecutionSession,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        self.render_inner(CaptureSource::Session(session), false)
    }

    /// Consume the current coherent publication and borrow its completed pixels.
    /// The borrow prevents another capture from overwriting these pixels. Copying
    /// them for retention is the consumer's explicit allocation.
    pub fn capture(
        &mut self,
        session: &mut ExecutionSession,
    ) -> Result<SessionFrame<'_>, SessionCaptureError> {
        let receipt = self.render_inner(CaptureSource::Session(session), true)?;
        Ok(self.frame(receipt))
    }

    #[cfg(feature = "prepared-prefix")]
    pub fn render_prefix(
        &mut self,
        reader: &mut noon::PrefixReader,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        self.render_inner(CaptureSource::Prefix(reader), false)
    }

    #[cfg(feature = "prepared-prefix")]
    pub fn capture_prefix(
        &mut self,
        reader: &mut noon::PrefixReader,
    ) -> Result<SessionFrame<'_>, SessionCaptureError> {
        let receipt = self.render_inner(CaptureSource::Prefix(reader), true)?;
        Ok(self.frame(receipt))
    }

    fn frame(&self, receipt: CaptureReceipt) -> SessionFrame<'_> {
        SessionFrame {
            receipt,
            width: self.options.width,
            height: self.options.height,
            format: self.gpu.pixel_format(),
            rgba: self.gpu.pixels(),
        }
    }

    pub(crate) fn summary(
        &self,
        sampling: noon::integration::ExportFrameSummary,
    ) -> CaptureSummary {
        self.gpu.summary(sampling)
    }

    fn render_inner(
        &mut self,
        mut source: CaptureSource<'_>,
        read_pixels: bool,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        let result = (|| {
            if self.failed {
                return Err(SessionCaptureError::Inactive);
            }
            if source.runtime_identity() != self.runtime {
                return Err(SessionCaptureError::WrongRuntime);
            }
            source.check_settled(&self.options)?;
            let context = source.publication_context();
            let time = source.time();
            let view = source
                .view(&self.options)
                .map_err(SessionCaptureError::Capture)?;
            let visibility = source.visibility(&view.bounds)?;
            let publication = source.take_renderer_publication();
            if publication.context() != context {
                return Err(SessionCaptureError::Capture(
                    CaptureError::PublicationMismatch,
                ));
            }
            let work = self
                .gpu
                .render(
                    &publication,
                    &view,
                    visibility.object_indices(),
                    read_pixels,
                )
                .map_err(SessionCaptureError::Capture)?;
            if self.options.cancellation.is_cancelled() {
                return Err(SessionCaptureError::Cancelled);
            }
            Ok(CaptureReceipt {
                publication: context,
                published_time: time,
                work,
            })
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

/// A temporary typed borrow of one existing owner, not a scene/runtime wrapper.
enum CaptureSource<'a> {
    Session(&'a mut ExecutionSession),
    #[cfg(feature = "prepared-prefix")]
    Prefix(&'a mut noon::PrefixReader),
}

impl CaptureSource<'_> {
    fn runtime_identity(&self) -> RuntimeIdentity {
        match self {
            Self::Session(session) => session.runtime_identity(),
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => reader.runtime_identity(),
        }
    }
    fn publication_context(&self) -> PublicationContext {
        match self {
            Self::Session(session) => session.publication_context(),
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => reader.publication_context(),
        }
    }
    fn time(&self) -> f64 {
        match self {
            Self::Session(session) => session.frame().time,
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => reader.frame().time,
        }
    }
    fn check_settled(&self, options: &CaptureOptions) -> Result<(), SessionCaptureError> {
        match self {
            Self::Session(session) => check_settled(session, options),
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => check_prefix(reader, options),
        }
    }
    fn view(&self, options: &CaptureOptions) -> Result<gpu::CaptureView, CaptureError> {
        match self {
            Self::Session(session) => gpu::CaptureView::new(session, options),
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => gpu::CaptureView::from_prefix(reader, options),
        }
    }
    fn visibility(
        &mut self,
        bounds: &[noon_core::Rect],
    ) -> Result<ExecutionViewportQuery, SessionCaptureError> {
        match self {
            Self::Session(session) => {
                let query = session.query_viewports(bounds);
                session.renderer_viewport_query(query).map_err(|error| {
                    SessionCaptureError::Capture(CaptureError::View(error.to_string()))
                })
            }
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => Ok(reader.query_renderer_viewports(bounds)),
        }
    }
    fn take_renderer_publication(&mut self) -> RendererPublication<'_> {
        match self {
            Self::Session(session) => session.take_renderer_publication(),
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(reader) => reader.take_renderer_publication(),
        }
    }
}

#[cfg(feature = "prepared-prefix")]
fn check_prefix(
    reader: &noon::PrefixReader,
    options: &CaptureOptions,
) -> Result<(), SessionCaptureError> {
    if options.cancellation.is_cancelled() {
        return Err(SessionCaptureError::Cancelled);
    }
    reader
        .completed_history_end_time()
        .map_err(SessionCaptureError::Prefix)?;
    if reader.interactions_active() || reader.pointer_gesture_active() {
        return Err(SessionCaptureError::ActiveInput);
    }
    Ok(())
}

fn check_settled(
    session: &ExecutionSession,
    options: &CaptureOptions,
) -> Result<(), SessionCaptureError> {
    if options.cancellation.is_cancelled() {
        return Err(SessionCaptureError::Cancelled);
    }
    if session.pending_callback_token().is_some() {
        return Err(SessionCaptureError::UnsettledCallback);
    }
    Ok(())
}

#[derive(Debug)]
pub enum SessionCaptureError {
    Capture(CaptureError),
    WrongRuntime,
    UnsettledCallback,
    #[cfg(feature = "prepared-prefix")]
    Prefix(noon::PrefixError),
    #[cfg(feature = "prepared-prefix")]
    ActiveInput,
    Cancelled,
    Inactive,
}

impl fmt::Display for SessionCaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(error) => error.fmt(f),
            Self::WrongRuntime => f.write_str("capture belongs to another runtime"),
            Self::UnsettledCallback => {
                f.write_str("settle the callback before capturing its frame")
            }
            #[cfg(feature = "prepared-prefix")]
            Self::Prefix(error) => error.fmt(f),
            #[cfg(feature = "prepared-prefix")]
            Self::ActiveInput => f.write_str("complete input effects before capturing history"),
            Self::Cancelled => f.write_str("session capture was cancelled"),
            Self::Inactive => f.write_str("session capture failed; start a new capture run"),
        }
    }
}

impl Error for SessionCaptureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            _ => None,
        }
    }
}

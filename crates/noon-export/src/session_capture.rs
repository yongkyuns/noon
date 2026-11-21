//! Capture a settled publication owned by a native language host.
//!
//! No frame clock, source continuation, callback loop or file encoder lives here.
//! This is another entry point to the same native capture implementation, not a
//! renderer for Python or another representation of the scene.

use std::{error::Error, fmt};

use noon::integration::scene_execution::ExecutionSession;
use noon::integration::{RendererPublication, RuntimeIdentity};
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
        let capture = Self::allocate(session.runtime_identity(), session.video_rasters(), options)?;
        check_settled(session, &capture.options)?;
        Ok(capture)
    }

    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn new_prefix(
        reader: &noon::PrefixReader,
        options: CaptureOptions,
    ) -> Result<Self, SessionCaptureError> {
        Self::allocate(reader.runtime_identity(), reader.video_rasters(), options)
    }

    fn allocate(
        runtime: RuntimeIdentity,
        videos: &[(
            noon_core::RasterImageResourceHandle,
            noon_core::VideoRasterResource,
        )],
        options: CaptureOptions,
    ) -> Result<Self, SessionCaptureError> {
        let layout = options.layout().map_err(SessionCaptureError::Capture)?;
        let mut gpu = pollster::block_on(gpu::NativeCapture::new(&options, layout))
            .map_err(SessionCaptureError::Capture)?;
        gpu.prepare_video(videos)
            .map_err(SessionCaptureError::Capture)?;
        Ok(Self {
            gpu,
            options,
            runtime,
            failed: false,
        })
    }

    /// Prepare immutable modules on this host's real device before the caller
    /// publishes a staged effect transaction. This consumes no frame or source
    /// continuation; the ordinary live publication still validates the request.
    pub fn prepare_effect_programs(
        &mut self,
        session: &ExecutionSession,
        programs: &[noon_core::WgslEffectProgram],
    ) -> Result<(), SessionCaptureError> {
        if self.failed {
            return Err(SessionCaptureError::Inactive);
        }
        if session.runtime_identity() != self.runtime {
            return Err(SessionCaptureError::WrongRuntime);
        }
        check_settled(session, &self.options)?;
        let result = self
            .gpu
            .prepare_effect_modules(programs)
            .map_err(SessionCaptureError::Capture);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// Render/consume a required publication without reading or emitting pixels.
    /// Useful for off-grid endpoint barriers; it is not an extra video frame.
    pub fn render(
        &mut self,
        session: &mut ExecutionSession,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        self.render_inner(session, false)
    }

    /// Consume the current coherent publication and borrow its completed pixels.
    /// The borrow prevents another capture from overwriting these pixels. Copying
    /// them for retention is the consumer's explicit allocation.
    pub fn capture(
        &mut self,
        session: &mut ExecutionSession,
    ) -> Result<SessionFrame<'_>, SessionCaptureError> {
        let receipt = self.render_inner(session, true)?;
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

    #[cfg(feature = "prepared-prefix")]
    pub(crate) fn capture_prefix(
        &mut self,
        reader: &mut noon::PrefixReader,
        read_pixels: bool,
    ) -> Result<SessionFrame<'_>, SessionCaptureError> {
        let result = (|| {
            self.check_owner(reader.runtime_identity())?;
            let context = reader.publication_context();
            let view = gpu::CaptureView::from_prefix(reader, &self.options)
                .map_err(SessionCaptureError::Capture)?;
            let visibility = reader.query_renderer_viewports(&view.bounds);
            self.render_publication(
                context,
                &reader.take_renderer_publication(),
                &view,
                visibility.object_indices(),
                read_pixels,
            )
        })();
        self.failed |= result.is_err();
        Ok(self.frame(result?))
    }

    pub(crate) fn summary(
        &self,
        sampling: noon::integration::ExportFrameSummary,
    ) -> CaptureSummary {
        self.gpu.summary(sampling)
    }

    fn render_inner(
        &mut self,
        session: &mut ExecutionSession,
        read_pixels: bool,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        let result = (|| {
            self.check_owner(session.runtime_identity())?;
            check_settled(session, &self.options)?;
            let context = session.publication_context();
            let view = gpu::CaptureView::new(session, &self.options)
                .map_err(SessionCaptureError::Capture)?;
            let queries = view
                .bounds
                .iter()
                .copied()
                .map(|bounds| session.query_viewport(bounds))
                .collect::<Vec<_>>();
            let visibility = session
                .renderer_viewport_query_union(queries)
                .map_err(|e| SessionCaptureError::Capture(CaptureError::View(e.to_string())))?;
            self.render_publication(
                context,
                &session.take_renderer_publication(),
                &view,
                visibility.object_indices(),
                read_pixels,
            )
        })();
        self.failed |= result.is_err();
        result
    }

    fn check_owner(&self, runtime: RuntimeIdentity) -> Result<(), SessionCaptureError> {
        if self.failed {
            return Err(SessionCaptureError::Inactive);
        }
        if runtime != self.runtime {
            return Err(SessionCaptureError::WrongRuntime);
        }
        if self.options.cancellation.is_cancelled() {
            return Err(SessionCaptureError::Cancelled);
        }
        Ok(())
    }

    fn render_publication(
        &mut self,
        context: PublicationContext,
        publication: &RendererPublication<'_>,
        view: &gpu::CaptureView,
        visible: &[usize],
        read_pixels: bool,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        if publication.context() != context {
            return Err(SessionCaptureError::Capture(
                CaptureError::PublicationMismatch,
            ));
        }
        let work = self
            .gpu
            .render(publication, view, visible, read_pixels)
            .map_err(SessionCaptureError::Capture)?;
        if self.options.cancellation.is_cancelled() {
            return Err(SessionCaptureError::Cancelled);
        }
        Ok(CaptureReceipt {
            publication: context,
            published_time: publication.frame().time,
            work,
        })
    }
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

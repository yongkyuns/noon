//! Capture a settled publication owned by a native language host.
//!
//! No frame clock, source continuation, callback loop or file encoder lives here.
//! This is another entry point to the same native capture implementation, not a
//! renderer for Python or another representation of the scene.

use std::{error::Error, fmt};

use noon::integration::RuntimeIdentity;
use noon::ExecutionSession;
use noon_core::PublicationContext;

use crate::{gpu, CaptureError, CaptureOptions, CapturePixelFormat, CaptureWork};

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
        let layout = options.layout().map_err(SessionCaptureError::Capture)?;
        let gpu = pollster::block_on(gpu::NativeCapture::new(&options, layout))
            .map_err(SessionCaptureError::Capture)?;
        check_settled(session, &options)?;
        Ok(Self {
            gpu,
            options,
            runtime: session.runtime_identity(),
            failed: false,
        })
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
        Ok(SessionFrame {
            receipt,
            width: self.options.width,
            height: self.options.height,
            format: CapturePixelFormat::RendererRgba8UnormOpaque,
            rgba: self.gpu.pixels(),
        })
    }

    fn render_inner(
        &mut self,
        session: &mut ExecutionSession,
        read_pixels: bool,
    ) -> Result<CaptureReceipt, SessionCaptureError> {
        let result = (|| {
            if self.failed {
                return Err(SessionCaptureError::Inactive);
            }
            if session.runtime_identity() != self.runtime {
                return Err(SessionCaptureError::WrongRuntime);
            }
            check_settled(session, &self.options)?;
            let context = session.publication_context();
            let time = session.frame().time;
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
            let publication = session.take_renderer_publication();
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

//! Optional native Python output adapter. The existing Context remains the
//! semantic/runtime owner; only frame policy, GPU capture and file lifetime live
//! here. Python is entered at source/callback barriers, never to schedule frames.
use std::{fmt, path::PathBuf};

use noon::integration::{
    CallbackAdvance, ExportFrameOptions, ExportFramePolicy, ExportFramePolicyStatus,
    ExportSample, ExportStop, FrameRate, SampleObservation,
};
use noon_export::{
    output::{FileSink, OutputOptions}, CaptureOptions, CapturedFrame, SessionCapture,
};
use pyo3::{exceptions::PyRuntimeError, prelude::*, types::PyDict};

use crate::{callback, context::Context, engine_error};

fn output_error(error: impl fmt::Display) -> PyErr {
    PyRuntimeError::new_err(format!("native video export: {error}"))
}

/// Native-only optional adapter for one finite Python source invocation.
/// Does not extend the native binding's supported geometry/resource profile.
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct VideoExport {
    // Drop the sink first: it must stop writing before capture/context retirement.
    sink: Option<FileSink>,
    capture: Option<SessionCapture>,
    context: Option<Py<Context>>,
    policy: ExportFramePolicy,
    options: CaptureOptions,
    transition_limit: u32,
    remaining: u32,
    request: Option<f64>,
    active: bool,
}

#[pymethods]
impl VideoExport {
    #[new]
    #[pyo3(signature = (path, *, width=1280, height=720, p=30, q=1,
        max_frames=108000, start_frame=0, final_hold=0.0, png=false,
        overwrite=false, fallback=false, ffmpeg=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        path: PathBuf,
        width: u32,
        height: u32,
        p: u32,
        q: u32,
        max_frames: u64,
        start_frame: u64,
        final_hold: f64,
        png: bool,
        overwrite: bool,
        fallback: bool,
        ffmpeg: Option<PathBuf>,
    ) -> PyResult<Self> {
        let rate = FrameRate::new(p, q).map_err(output_error)?;
        let frame_options = ExportFrameOptions {
            frame_rate: rate,
            start_frame,
            stop: ExportStop::SourceEnd,
            max_frames,
            max_transitions_per_sample: 4096,
            final_hold_seconds: final_hold,
        };
        let policy = ExportFramePolicy::new(frame_options).map_err(output_error)?;
        if png && max_frames > i32::MAX as u64 {
            return Err(output_error("PNG safety cap exceeds encoder numbering"));
        }
        let mut options = CaptureOptions::new(width, height);
        options.force_fallback_adapter = fallback;
        options.layout().map_err(output_error)?;
        let mut output = if png {
            OutputOptions::png_sequence(path)
        } else {
            OutputOptions::mp4(path)
        };
        output.overwrite = overwrite;
        if let Some(ffmpeg) = ffmpeg {
            output.ffmpeg = ffmpeg;
        }
        let sink = FileSink::new(output, rate, width, height, options.cancellation.clone())
            .map_err(output_error)?;
        Ok(Self {
            sink: Some(sink),
            capture: None,
            context: None,
            policy,
            options,
            transition_limit: frame_options.max_transitions_per_sample,
            remaining: 0,
            request: None,
            active: true,
        })
    }

    fn bind(&mut self, context: &Bound<'_, Context>) -> PyResult<()> {
        let result = (|| {
            self.require_active()?;
            if self.context.is_some() {
                return Err(output_error("one export accepts exactly one source context"));
            }
            let source = context.try_borrow()?;
            source.require_active()?;
            if source.execution.is_some() || source.segment.is_some() {
                return Err(output_error("export requires a fresh source context"));
            }
            drop(source);
            self.context = Some(context.clone().unbind());
            Ok(())
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn drive(&mut self, py: Python<'_>, context: &Bound<'_, Context>) -> PyResult<Py<PyDict>> {
        let result = (|| {
            self.require_context(context)?;
            let mut source = context.try_borrow_mut()?;
            self.drive_inner(py, &mut source)
        })();
        if result.is_err() {
            self.abort();
        }
        result
    }

    /// Called only after the selected Python source successfully returned. This
    /// drains terminal publications/holds and finalizes the existing file sink.
    fn finish(&mut self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let result = self.finish_inner(py);
        if result.is_err() {
            self.abort();
        }
        result
    }

    fn abort(&mut self) {
        self.active = false;
        self.policy.cancel();
        self.sink.take();
        self.capture.take();
        self.context.take();
    }
}

impl VideoExport {
    fn require_active(&self) -> PyResult<()> {
        if !self.active {
            return Err(output_error("export is finished, failed or cancelled"));
        }
        Ok(())
    }

    fn require_context(&self, context: &Bound<'_, Context>) -> PyResult<()> {
        self.require_active()?;
        if !self.context.as_ref().is_some_and(|owner| owner.as_ptr() == context.as_ptr()) {
            return Err(output_error("export belongs to another source context"));
        }
        Ok(())
    }

    fn ensure_capture(&mut self, source: &Context) -> PyResult<()> {
        if self.capture.is_none() {
            let session = source.execution.as_ref().ok_or_else(|| output_error("no execution"))?;
            self.capture = Some(SessionCapture::new(session, self.options.clone()).map_err(output_error)?);
        }
        Ok(())
    }

    fn spend_transition(&mut self, requested: f64) -> PyResult<()> {
        if self.request != Some(requested) {
            self.request = Some(requested);
            self.remaining = self.transition_limit;
        }
        self.remaining = self.remaining.checked_sub(1)
            .ok_or_else(|| output_error("sample transition budget exhausted"))?;
        Ok(())
    }

    fn drive_inner(&mut self, py: Python<'_>, source: &mut Context) -> PyResult<Py<PyDict>> {
        source.require_active()?;
        if source.pending_ack.is_some() {
            return Err(output_error("acknowledge the committed callback before advancing"));
        }
        let segment = source.segment.ok_or_else(|| output_error("no pending segment"))?;
        self.ensure_capture(source)?;
        loop {
            py.check_signals()?;
            match self.policy.status().map_err(output_error)? {
                ExportFramePolicyStatus::NeedsSample(requested) => {
                    self.spend_transition(requested)?;
                    let session = source.execution.as_mut().ok_or_else(|| output_error("no execution"))?;
                    match session.advance_segment_to_callback_barrier(segment, requested).map_err(engine_error)? {
                        CallbackAdvance::HostRequired { invocations, overlay } => {
                            source.callback_regions += 1;
                            return callback::event(py, "callback", Some(callback::phase(py, overlay, invocations)?));
                        }
                        CallbackAdvance::Ready(_) => {}
                    }
                    if session.frame().time >= segment.end_time() {
                        source.scene.live(session).complete_segment(segment).map_err(engine_error)?;
                        // Consume this exact endpoint before Python resumes. It is
                        // NOT an output sample, even when it lies on the frame grid:
                        // same-time source edits must settle first.
                        self.capture.as_mut().ok_or_else(|| output_error("capture inactive"))?
                            .render(session).map_err(output_error)?;
                        source.segment = None;
                        return callback::event(py, "complete", None);
                    }
                    if session.frame().time != requested {
                        continue;
                    }
                    self.policy.observe(SampleObservation {
                        requested_time: requested,
                        published_time: session.frame().time,
                        publication: session.publication_context(),
                    }, false).map_err(output_error)?;
                }
                ExportFramePolicyStatus::SampleReady(sample) => self.consume(source, sample)?,
                ExportFramePolicyStatus::Complete(_) => {
                    return Err(output_error("source ended outside its continuation boundary"));
                }
            }
        }
    }

    fn consume(&mut self, source: &mut Context, sample: ExportSample) -> PyResult<()> {
        let session = source.execution.as_mut().ok_or_else(|| output_error("no execution"))?;
        let capture = self.capture.as_mut().ok_or_else(|| output_error("capture inactive"))?;
        if let Some(frame) = sample.frame {
            let pixels = capture.capture(session).map_err(output_error)?;
            if pixels.receipt.publication != sample.observation.publication
                || pixels.receipt.published_time != sample.observation.published_time {
                return Err(output_error("capture does not match the settled sample"));
            }
            self.sink.as_mut().ok_or_else(|| output_error("encoder inactive"))?
                .write(CapturedFrame {
                    frame,
                    observation: sample.observation,
                    width: pixels.width,
                    height: pixels.height,
                    format: pixels.format,
                    rgba: pixels.rgba,
                    work: pixels.receipt.work,
                }).map_err(output_error)?;
            source.frames += 1;
        } else {
            let receipt = capture.render(session).map_err(output_error)?;
            if receipt.publication != sample.observation.publication
                || receipt.published_time != sample.observation.published_time {
                return Err(output_error("publication changed before consumption"));
            }
        }
        self.policy.acknowledge_sample(sample).map_err(output_error)
    }

    fn finish_inner(&mut self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        self.require_active()?;
        let owner = self.context.as_ref().ok_or_else(|| output_error("source created no scene"))?.clone_ref(py);
        let mut source = owner.try_borrow_mut(py)?;
        source.require_active()?;
        if source.segment.is_some() || source.pending_ack.is_some() {
            return Err(output_error("source returned with unfinished animation or callback work"));
        }
        if source.execution.is_none() {
            source.execution = Some(source.scene.execution_session().map_err(|e| output_error(e.to_string()))?);
        }
        self.ensure_capture(&source)?;
        let sampling = loop {
            py.check_signals()?;
            match self.policy.status().map_err(output_error)? {
                ExportFramePolicyStatus::NeedsSample(requested) => {
                    let session = source.execution.as_ref().ok_or_else(|| output_error("no execution"))?;
                    self.policy.observe(SampleObservation {
                        requested_time: requested,
                        published_time: session.frame().time,
                        publication: session.publication_context(),
                    }, true).map_err(output_error)?;
                }
                ExportFramePolicyStatus::SampleReady(sample) => self.consume(&mut source, sample)?,
                ExportFramePolicyStatus::Complete(summary) => break summary,
            }
        };
        let (path, format, diagnostics) = self.sink.take().ok_or_else(|| output_error("encoder inactive"))?
            .finish(sampling.frames).map_err(output_error)?;
        self.active = false;
        self.capture.take();
        self.context.take();
        let result = PyDict::new(py);
        result.set_item("path", path.to_string_lossy().as_ref())?;
        result.set_item("format", format!("{format:?}"))?;
        result.set_item("frames", sampling.frames)?;
        result.set_item("fps", (sampling.frame_rate.numerator(), sampling.frame_rate.denominator()))?;
        result.set_item("start_time", sampling.start_time)?;
        result.set_item("end_time", sampling.end_time)?;
        result.set_item("source_end", sampling.source_end)?;
        result.set_item("duration", sampling.scheduled_duration)?;
        result.set_item("encoder_diagnostics", diagnostics)?;
        Ok(result.unbind())
    }
}

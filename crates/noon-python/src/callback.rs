//! Typed CPython projections of the existing Rust callback transaction.
//! Only Rust selects occurrences, evaluates frames, validates writes and commits.
use crate::{context::Context, engine_error};
use noon::integration::{
    CallbackAdvance, CallbackPhaseOverlay, CallbackPhaseToken, CallbackReadRequest,
    CallbackReadValue, CallbackRegionAdvance, EffectiveObjectProperties, EffectivePropertyBatch,
    EffectiveSemanticPropertyWrite, RequiredCallbackInvocation,
};
use noon::{Color, SemanticNodeId, Style, Transform2D, Vec2};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
};

fn node<'py>(py: Python<'py>, id: SemanticNodeId) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("slot", id.slot())?;
    result.set_item("generation", id.generation())?;
    Ok(result)
}
fn read_node(value: &Bound<'_, PyAny>) -> PyResult<SemanticNodeId> {
    Ok(SemanticNodeId::new(
        value.get_item("slot")?.extract()?,
        value.get_item("generation")?.extract()?,
    ))
}
fn point<'py>(py: Python<'py>, v: Vec2) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("x", f64::from(v.x))?;
    d.set_item("y", f64::from(v.y))?;
    Ok(d)
}
fn read_point(value: &Bound<'_, PyAny>) -> PyResult<Vec2> {
    Ok(Vec2::new(
        value.get_item("x")?.extract()?,
        value.get_item("y")?.extract()?,
    ))
}
fn color<'py>(py: Python<'py>, value: Option<Color>) -> PyResult<Bound<'py, PyAny>> {
    let Some(c) = value else {
        return Ok(py.None().into_bound(py));
    };
    let d = PyDict::new(py);
    for (key, value) in [
        ("red", c.red),
        ("green", c.green),
        ("blue", c.blue),
        ("alpha", c.alpha),
    ] {
        d.set_item(key, f64::from(value))?;
    }
    Ok(d.into_any())
}
fn read_color(value: &Bound<'_, PyAny>) -> PyResult<Option<Color>> {
    if value.is_none() {
        return Ok(None);
    }
    Ok(Some(Color::rgba(
        value.get_item("red")?.extract()?,
        value.get_item("green")?.extract()?,
        value.get_item("blue")?.extract()?,
        value.get_item("alpha")?.extract()?,
    )))
}
fn transform<'py>(py: Python<'py>, v: Transform2D) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("translation", point(py, v.translation)?)?;
    d.set_item("scale", point(py, v.scale)?)?;
    d.set_item("rotation", f64::from(v.rotation))?;
    Ok(d)
}
fn read_transform(value: &Bound<'_, PyAny>) -> PyResult<Transform2D> {
    Ok(Transform2D {
        translation: read_point(&value.get_item("translation")?)?,
        scale: read_point(&value.get_item("scale")?)?,
        rotation: value.get_item("rotation")?.extract()?,
    })
}
fn style<'py>(py: Python<'py>, v: Style) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("fill", color(py, v.fill)?)?;
    d.set_item("stroke", color(py, v.stroke)?)?;
    d.set_item("stroke_width", f64::from(v.stroke_width))?;
    d.set_item("opacity", f64::from(v.opacity))?;
    d.set_item(
        "stroke_width_mode",
        match v.stroke_width_mode {
            noon::StrokeWidthMode::ScaleWithObject => "scale_with_object",
            noon::StrokeWidthMode::ScreenSpace => "screen_space",
        },
    )?;
    d.set_item(
        "stroke_join",
        match v.stroke_join {
            noon::StrokeJoin::Round => "round",
            noon::StrokeJoin::Bevel => "bevel",
            noon::StrokeJoin::Miter => "miter",
        },
    )?;
    d.set_item(
        "stroke_cap",
        match v.stroke_cap {
            noon::StrokeCap::Round => "round",
            noon::StrokeCap::Butt => "butt",
            noon::StrokeCap::Square => "square",
        },
    )?;
    Ok(d)
}
fn object<'py>(
    py: Python<'py>,
    id: SemanticNodeId,
    v: &EffectiveObjectProperties,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("node", node(py, id)?)?;
    d.set_item("transform", transform(py, v.transform)?)?;
    d.set_item("style", style(py, v.style)?)?;
    d.set_item("appearance", v.appearance)?;
    d.set_item("presence", v.presence)?;
    d.set_item("reveal", v.reveal)?;
    d.set_item("morph", v.morph)?;
    if let Some(bounds) = v.bounds {
        let b = PyDict::new(py);
        b.set_item("min", point(py, bounds.min)?)?;
        b.set_item("max", point(py, bounds.max)?)?;
        d.set_item("bounds", b)?;
    } else {
        d.set_item("bounds", py.None())?;
    }
    Ok(d)
}
pub(crate) fn token_dict<'py>(
    py: Python<'py>,
    token: CallbackPhaseToken,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    let p = PyDict::new(py);
    let publication = token.publication();
    p.set_item(
        "scene_revision",
        publication.scene_revision().get().to_string(),
    )?;
    p.set_item(
        "execution_revision",
        publication.execution_revision().get().to_string(),
    )?;
    p.set_item("frame_epoch", publication.frame_epoch().get().to_string())?;
    d.set_item("runtime", token.runtime().get().to_string())?;
    d.set_item("sequence", token.sequence().get().to_string())?;
    d.set_item("publication", p)?;
    Ok(d)
}
fn require_token(
    py: Python<'_>,
    actual: &Bound<'_, PyAny>,
    expected: CallbackPhaseToken,
) -> PyResult<()> {
    if !actual.eq(token_dict(py, expected)?)? {
        return Err(engine_error(noon::integration::AuthoringFailure::new(
            "stale_publication",
            "callback.stale_token",
            "callback token does not identify the pending publication",
        )));
    }
    Ok(())
}
fn phase<'py>(
    py: Python<'py>,
    overlay: CallbackPhaseOverlay,
    invocations: Vec<RequiredCallbackInvocation>,
) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("token", token_dict(py, overlay.token())?)?;
    d.set_item("region", overlay.region())?;
    d.set_item("time", overlay.time())?;
    d.set_item("delta_time", overlay.delta_time())?;
    let objects = PyList::empty(py);
    for (id, value) in overlay.objects() {
        objects.append(object(py, id, value)?)?;
    }
    d.set_item("objects", objects)?;
    let calls = PyList::empty(py);
    for i in invocations {
        let call = PyDict::new(py);
        call.set_item("callback_id", i.callback_id().get().to_string())?;
        call.set_item("target", node(py, i.target())?)?;
        call.set_item("occurrence_index", i.occurrence_index())?;
        calls.append(call)?;
    }
    d.set_item("invocations", calls)?;
    Ok(d)
}
fn event<'py>(
    py: Python<'py>,
    kind: &str,
    phase: Option<Bound<'py, PyDict>>,
) -> PyResult<Py<PyDict>> {
    let d = PyDict::new(py);
    d.set_item("kind", kind)?;
    if let Some(phase) = phase {
        d.set_item("phase", phase)?;
    }
    Ok(d.unbind())
}

impl Context {
    /// Advance the compiled Rust runtime until Python is genuinely required.
    /// Sampling is an explicit renderer-free host policy, not a Python timeline.
    pub(crate) fn drive_impl(&mut self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        self.require_active()?;
        if self.pending_ack.is_some() {
            return Err(engine_error(
                "callback wrapper publication must be acknowledged before advancing",
            ));
        }
        let segment = self
            .segment
            .ok_or_else(|| engine_error("no pending segment"))?;
        let session = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("no execution"))?;
        loop {
            let requested = match self.requested_sample {
                Some(time) => time,
                None => {
                    let current = session.frame().time;
                    // Skip inactive intervals using the runtime's indexed cadence.
                    // Never scan objects or invent a host-owned activity schedule.
                    let next = match session.segment_state(segment).timeline() {
                        noon::integration::TimelineWakeState::Deadline(time) => time,
                        noon::integration::TimelineWakeState::Quiescent => segment.end_time(),
                        noon::integration::TimelineWakeState::Continuous => {
                            self.next_sample = self
                                .next_sample
                                .max((current * self.sample_hz).ceil() as u64);
                            self.next_sample as f64 / self.sample_hz
                        }
                    }
                    .min(segment.end_time());
                    self.requested_sample = Some(next);
                    next
                }
            };
            match session
                .advance_segment_to_callback_barrier(segment, requested)
                .map_err(engine_error)?
            {
                CallbackAdvance::HostRequired {
                    invocations,
                    overlay,
                } => {
                    self.callback_regions += 1;
                    return event(py, "callback", Some(phase(py, overlay, invocations)?));
                }
                CallbackAdvance::Ready(_) => {}
            }
            if session.frame().time < requested {
                continue;
            }
            self.frames += 1;
            self.requested_sample = None;
            // Retain monotonic grid progress even when n/hz*hz rounds below n.
            // Recomputing solely with floor can resubmit sample n forever.
            self.next_sample = self
                .next_sample
                .saturating_add(1)
                .max(((requested * self.sample_hz).floor() as u64).saturating_add(1));
            if session.frame().time >= segment.end_time() {
                self.scene
                    .live(session)
                    .complete_segment(segment)
                    .map_err(engine_error)?;
                self.segment = None;
                return event(py, "complete", None);
            }
        }
    }
    /// Validate and commit the same Rust-owned effective batch as the browser.
    pub(crate) fn submit_callback_impl(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        value: &Bound<'_, PyDict>,
    ) -> PyResult<Py<PyDict>> {
        self.require_active()?;
        let session = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("no execution"))?;
        let expected = session
            .pending_callback_token()
            .ok_or_else(|| engine_error("no pending callback"))?;
        require_token(py, token, expected)?;
        require_token(
            py,
            &value
                .get_item("token")?
                .ok_or_else(|| engine_error("missing batch token"))?,
            expected,
        )?;
        if value.contains("content")? {
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "unsupported_operation",
                "python.callback_content",
                "native callback content replacement is outside the qualified profile",
            )));
        }
        let region: u32 = value
            .get_item("region")?
            .ok_or_else(|| engine_error("missing callback region"))?
            .extract()?;
        let values = value
            .get_item("writes")?
            .ok_or_else(|| engine_error("missing callback writes"))?;
        let mut writes = Vec::new();
        for value in values.try_iter()? {
            let value = value?;
            let object = read_node(&value.get_item("object")?)?;
            let kind: String = value.get_item("kind")?.extract()?;
            let write = match kind.as_str() {
                "translation" => EffectiveSemanticPropertyWrite::Translation {
                    object,
                    translation: read_point(&value.get_item("translation")?)?,
                },
                "rotation" => EffectiveSemanticPropertyWrite::Rotation {
                    object,
                    rotation: value.get_item("rotation")?.extract()?,
                },
                "scale" => EffectiveSemanticPropertyWrite::Scale {
                    object,
                    scale: read_point(&value.get_item("scale")?)?,
                },
                "opacity" => EffectiveSemanticPropertyWrite::Opacity {
                    object,
                    opacity: value.get_item("opacity")?.extract()?,
                },
                "stroke_width" => EffectiveSemanticPropertyWrite::StrokeWidth {
                    object,
                    stroke_width: value.get_item("stroke_width")?.extract()?,
                },
                "fill" => EffectiveSemanticPropertyWrite::Fill {
                    object,
                    fill: read_color(&value.get_item("fill")?)?,
                },
                "stroke" => EffectiveSemanticPropertyWrite::Stroke {
                    object,
                    stroke: read_color(&value.get_item("stroke")?)?,
                },
                "presence" => EffectiveSemanticPropertyWrite::Presence {
                    object,
                    presence: value.get_item("presence")?.extract()?,
                },
                "transform" => EffectiveSemanticPropertyWrite::Transform {
                    object,
                    transform: read_transform(&value.get_item("transform")?)?,
                },
                _ => return Err(engine_error(format!("unsupported effective write: {kind}"))),
            };
            writes.push(write);
        }
        let batch = EffectivePropertyBatch::new(expected, writes).with_region(region);
        match session
            .submit_required_callback_region(batch)
            .map_err(engine_error)?
        {
            CallbackRegionAdvance::HostRequired {
                invocations,
                overlay,
            } => {
                self.callback_regions += 1;
                event(py, "callback", Some(phase(py, overlay, invocations)?))
            }
            CallbackRegionAdvance::Complete(batch) => {
                session
                    .commit_required_callback_phase(batch)
                    .map_err(engine_error)?;
                self.pending_ack = Some(expected);
                let phase = PyDict::new(py);
                phase.set_item("token", token_dict(py, expected)?)?;
                phase.set_item("region", region)?;
                event(py, "callback_committed", Some(phase))
            }
        }
    }
    pub(crate) fn acknowledge_callback_impl(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.require_active()?;
        let expected = self
            .pending_ack
            .ok_or_else(|| engine_error("no pending callback acknowledgement"))?;
        require_token(py, token, expected)?;
        self.pending_ack = None;
        Ok(())
    }
    pub(crate) fn read_callback_impl(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        request: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyDict>> {
        self.require_active()?;
        let session = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("no execution"))?;
        let expected = session
            .pending_callback_token()
            .ok_or_else(|| engine_error("no pending callback"))?;
        require_token(py, token, expected)?;
        let id = read_node(&request.get_item("node")?)?;
        let kind: String = request.get_item("kind")?.extract()?;
        let request = match kind.as_str() {
            "object" => CallbackReadRequest::Object(id),
            "scalar_signal" => CallbackReadRequest::ScalarSignal(id),
            _ => return Err(engine_error("unsupported callback read")),
        };
        let d = PyDict::new(py);
        match session
            .required_callback_read(expected, request)
            .map_err(engine_error)?
        {
            CallbackReadValue::Scalar(v) => {
                d.set_item("kind", "scalar")?;
                d.set_item("value", v)?;
            }
            CallbackReadValue::Object(v) => {
                d.set_item("kind", "object")?;
                d.set_item("object", object(py, id, &v)?)?;
            }
        }
        Ok(d.unbind())
    }
    pub(crate) fn fail_callback_impl(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        message: &str,
    ) -> PyResult<()> {
        self.require_active()?;
        let session = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("no execution"))?;
        let expected = session
            .pending_callback_token()
            .ok_or_else(|| engine_error("no pending callback"))?;
        require_token(py, token, expected)?;
        session
            .fail_required_callback_phase(expected)
            .map_err(engine_error)?;
        Err(engine_error(noon::integration::AuthoringFailure::new(
            "callback_failure",
            "python.callback_failed",
            message,
        )))
    }
}

//! Native binding ownership for a single existing Rust scene/runtime pair.
//! The sample driver is compiled Rust; Python is entered only at source or
//! required-callback boundaries. It owns no timeline or effective scene copy.
use crate::callback_values::*;
use crate::{
    composition::Composition,
    engine_error,
    geometry::GeometryOptions,
    mobject::{pivot, LayoutAnchor, LayoutObservation, MobjectHandle},
};
use noon::integration::{HostCallbackId, SemanticMutationTransaction, SemanticStore};
use noon::{
    ExecutionSegment, ExecutionSession, Mobject, MobjectTarget, Scene, SceneMembershipRequest,
};
use noon::{Transform2D, Vec2};
use pyo3::prelude::*;
use std::{cell::RefCell, rc::Rc};

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct Store {
    semantics: Rc<RefCell<SemanticStore>>,
}
#[pymethods]
impl Store {
    #[new]
    fn new() -> Self {
        Self {
            semantics: Rc::new(RefCell::new(SemanticStore::new())),
        }
    }
    fn context(&self) -> Context {
        Context::new(Scene::with_integration_store(Rc::clone(&self.semantics)))
    }
    fn geometry(&self, options: &GeometryOptions) -> PyResult<MobjectHandle> {
        Mobject::from_manim_geometry(Rc::clone(&self.semantics), options.options.clone())
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct MembershipBatch {
    kind: String,
    members: Vec<Mobject>,
}
#[pymethods]
impl MembershipBatch {
    #[new]
    fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            members: Vec::new(),
        }
    }
    #[pyo3(name = "reserveMobjectBinding")]
    fn reserve(&self, _id: &str, handle: &MobjectHandle) -> PyResult<()> {
        handle.handle.validate().map_err(engine_error)
    }
    #[pyo3(name = "appendMobject")]
    fn append(&mut self, _id: &str, handle: &MobjectHandle) {
        self.members.push(handle.handle.clone());
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct Context {
    pub(crate) scene: Scene,
    pub(crate) execution: Option<ExecutionSession>,
    pub(crate) segment: Option<ExecutionSegment>,
    pub(crate) terminal: bool,
    pub(crate) sample_hz: f64,
    pub(crate) next_sample: u64,
    pub(crate) frames: u64,
    pub(crate) segments: u64,
    pub(crate) requested_sample: Option<f64>,
    pub(crate) pending_ack: Option<noon::integration::CallbackPhaseToken>,
    pub(crate) callback_regions: u64,
}
impl Context {
    fn new(scene: Scene) -> Self {
        Self {
            scene,
            execution: None,
            segment: None,
            terminal: false,
            sample_hz: 60.0,
            next_sample: 0,
            frames: 0,
            segments: 0,
            requested_sample: None,
            pending_ack: None,
            callback_regions: 0,
        }
    }
    pub(crate) fn require_active(&self) -> PyResult<()> {
        if self.terminal {
            Err(engine_error(noon::integration::AuthoringFailure::new(
                "ownership",
                "python.retired_session",
                "Python source session has been retired",
            )))
        } else {
            Ok(())
        }
    }
    fn same_store(&self, target: &MobjectHandle) -> PyResult<()> {
        if !Rc::ptr_eq(
            self.scene.integration_store(),
            target.handle.integration_store(),
        ) {
            return Err(engine_error(noon::AuthoringError::ForeignStore));
        }
        target.handle.validate().map_err(engine_error)
    }
    fn available(&self) -> PyResult<()> {
        self.require_active()?;
        if self.segment.is_some() {
            Err(engine_error("a segment is already active"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn with_live<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> PyResult<T> {
        self.require_active()?;
        let session = self
            .execution
            .as_mut()
            .ok_or_else(|| engine_error("no execution session"))?;
        operation(&mut self.scene.live(session)).map_err(engine_error)
    }
    fn publish(&mut self, transaction: SemanticMutationTransaction) -> PyResult<()> {
        self.require_active()?;
        if self.execution.is_some() {
            self.with_live(|live| live.apply(transaction).map(|_| ()))
        } else {
            transaction
                .apply(&mut self.scene.integration_store().borrow_mut())
                .map(|_| ())
                .map_err(engine_error)
        }
    }
}
#[pymethods]
impl Context {
    #[pyo3(name = "liveExecutionOwnership")]
    fn ownership(&self) -> PyResult<&'static str> {
        self.require_active()?;
        Ok(if self.execution.is_some() {
            "active"
        } else {
            "none"
        })
    }
    #[pyo3(name = "requireActive")]
    fn check_active(&self) -> PyResult<()> {
        self.require_active()
    }
    #[pyo3(name = "authoredDuration")]
    fn duration(&self) -> f64 {
        self.handoff().unwrap_or_else(|| self.scene.time())
    }
    #[pyo3(name = "liveHandoffDuration")]
    fn handoff(&self) -> Option<f64> {
        self.execution.as_ref().map(|s| {
            self.segment.map_or(s.effective_time(), |segment| {
                s.effective_time().max(segment.end_time())
            })
        })
    }
    #[pyo3(name = "beginMembershipBatch")]
    fn batch(&self, kind: &str) -> MembershipBatch {
        MembershipBatch {
            kind: kind.into(),
            members: Vec::new(),
        }
    }
    #[pyo3(name = "editMembership")]
    fn edit_membership(&mut self, batch: &MembershipBatch) -> PyResult<()> {
        self.available()?;
        let targets: Vec<_> = batch.members.iter().map(MobjectTarget::Object).collect();
        let request = match batch.kind.as_str() {
            "add" => SceneMembershipRequest::Add(&targets),
            "remove" => SceneMembershipRequest::Remove(&targets),
            "clear" => SceneMembershipRequest::Clear,
            "add_foreground" => SceneMembershipRequest::AddForeground(&targets),
            "remove_foreground" => SceneMembershipRequest::RemoveForeground(&targets),
            "bring_to_back" => SceneMembershipRequest::BringToBack(&targets),
            "replace" if targets.len() == 2 => SceneMembershipRequest::Replace {
                old: targets[0],
                new: targets[1],
            },
            _ => return Err(engine_error("unsupported membership request")),
        };
        if self.execution.is_some() {
            self.with_live(|live| live.edit_membership(request).map(|_| ()))
        } else {
            self.scene
                .edit_membership(request)
                .map(|_| ())
                .map_err(engine_error)
        }
    }
    #[pyo3(name = "containsMobject")]
    fn contains(&mut self, target: &MobjectHandle) -> PyResult<bool> {
        self.require_active()?;
        self.same_store(target)?;
        if self.execution.is_some() {
            self.with_live(|live| live.contains(&target.handle))
        } else {
            noon_core::semantic_scene_root_contains(
                &self.scene.integration_store().borrow(),
                self.scene.root(),
                target.handle.node_id(),
            )
            .map_err(engine_error)
        }
    }
    #[pyo3(name = "liveContainsMobject")]
    fn live_contains(&mut self, target: &MobjectHandle) -> PyResult<bool> {
        self.contains(target)
    }
    #[pyo3(name = "rootMembershipKeys")]
    fn membership(&self) -> PyResult<Vec<String>> {
        self.require_active()?;
        self.scene
            .integration_store()
            .borrow()
            .node(self.scene.root())
            .map(|node| {
                node.members()
                    .iter()
                    .map(|n| format!("{}:{}", n.slot(), n.generation()))
                    .collect()
            })
            .ok_or_else(|| engine_error("scene root has been retired"))
    }
    #[pyo3(name = "rootForegroundKeys")]
    fn foreground(&self) -> PyResult<Vec<String>> {
        self.require_active()?;
        self.scene
            .integration_store()
            .borrow()
            .node(self.scene.root())
            .map(|node| {
                node.foreground_members()
                    .iter()
                    .map(|n| format!("{}:{}", n.slot(), n.generation()))
                    .collect()
            })
            .ok_or_else(|| engine_error("retired scene root"))
    }
    #[pyo3(name = "bindMobject")]
    fn bind(&mut self, _id: &str, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.scene.add(&target.handle).map_err(engine_error)
    }
    #[pyo3(name = "liveAdd")]
    fn live_add(&mut self, _id: &str, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.add(&target.handle).map(|_| ()))
    }
    #[pyo3(name = "queryMobjectLayout")]
    fn layout(&mut self, target: &MobjectHandle) -> PyResult<LayoutObservation> {
        self.require_active()?;
        self.same_store(target)?;
        if self.execution.is_some() {
            self.with_live(|live| live.effective_layout(&target.handle))
                .map(LayoutObservation::from_effective)
        } else {
            Ok(LayoutObservation {
                center: target.handle.center().map_err(engine_error)?,
                width: target.handle.width().map_err(engine_error)?,
                height: target.handle.height().map_err(engine_error)?,
            })
        }
    }
    #[pyo3(name = "liveTargetEditor")]
    fn target(&mut self, source: &MobjectHandle) -> PyResult<MobjectHandle> {
        self.available()?;
        self.with_live(|live| live.target_editor(&source.handle))
            .map(MobjectHandle::from_semantic_mobject)
    }
    #[pyo3(name = "liveCreateManimGeometry")]
    fn geometry(&mut self, options: &GeometryOptions) -> PyResult<MobjectHandle> {
        self.available()?;
        self.with_live(|live| live.create_manim_geometry(options.options.clone()))
            .map(MobjectHandle::from_semantic_mobject)
    }
    #[pyo3(name = "liveShift")]
    fn shift(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.shift(&target.handle, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveSetFill")]
    fn fill(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_fill(&target.handle, r, g, b, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetFillColor")]
    fn fill_color(
        &mut self,
        target: &MobjectHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_fill_color(&target.handle, r, g, b, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetFillOpacity")]
    fn fill_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_fill_opacity(&target.handle, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetColor")]
    fn color(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_color(&target.handle, r, g, b, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetOpacity")]
    fn opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_opacity(&target.handle, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetObjectOpacity")]
    fn object_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_object_opacity(&target.handle, a).map(|_| ()))
    }
    #[pyo3(name = "liveScaleLayout")]
    fn scale(
        &mut self,
        target: &LayoutAnchor,
        sx: f64,
        sy: f64,
        x: f64,
        y: f64,
        point: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| {
            live.scale_layout(&target.anchor, sx, sy, pivot(x, y, point))
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveRotateLayout")]
    fn rotate(
        &mut self,
        target: &LayoutAnchor,
        angle: f64,
        x: f64,
        y: f64,
        point: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| {
            live.rotate_layout(&target.anchor, angle, pivot(x, y, point))
                .map(|_| ())
        })
    }
    #[pyo3(name="beginOrdinaryCompositionBuilder",signature=(kind,duration,lag,play_duration))]
    fn composition(
        &self,
        kind: &str,
        duration: Option<f64>,
        lag: f64,
        play_duration: Option<f64>,
    ) -> PyResult<Composition> {
        Composition::new(kind, duration, lag, play_duration)
    }
    #[pyo3(name = "ordinaryCanPlayComposition")]
    fn can_play(&self, _candidate: &Composition) -> PyResult<bool> {
        self.available()?;
        Ok(true)
    }
    #[pyo3(name = "beginOrdinaryComposition")]
    fn begin(&mut self, candidate: &Composition) -> PyResult<()> {
        self.available()?;
        if self.execution.is_none() {
            let mut session = self
                .scene
                .execution_session()
                .map_err(|e| engine_error(e.to_string()))?;
            let segment = self
                .scene
                .live(&mut session)
                .declare_and_activate_composition(&candidate.request(), candidate.play_options)
                .map_err(engine_error)?;
            self.execution = Some(session);
            self.segment = Some(segment);
        } else {
            self.segment = Some(self.with_live(|live| {
                live.declare_and_activate_composition(&candidate.request(), candidate.play_options)
            })?);
        }
        self.segments += 1;
        Ok(())
    }
    #[pyo3(name = "beginOrdinaryWait")]
    fn wait(&mut self, duration: f64) -> PyResult<()> {
        self.available()?;
        if self.execution.is_none() {
            let mut session = self
                .scene
                .execution_session()
                .map_err(|e| engine_error(e.to_string()))?;
            let segment = self
                .scene
                .live(&mut session)
                .wait_segment(duration)
                .map_err(engine_error)?;
            self.execution = Some(session);
            self.segment = Some(segment);
        } else {
            self.segment = Some(self.with_live(|live| live.wait_segment(duration))?);
        }
        self.segments += 1;
        Ok(())
    }
    #[pyo3(name="addUpdater",signature=(target,id,time,position))]
    fn add_updater(
        &mut self,
        target: &MobjectHandle,
        id: &str,
        time: f64,
        position: Option<usize>,
    ) -> PyResult<()> {
        self.available()?;
        if !Rc::ptr_eq(
            self.scene.integration_store(),
            target.handle.integration_store(),
        ) {
            return Err(engine_error(noon::AuthoringError::ForeignStore));
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(
            target.handle.node_id(),
            HostCallbackId::new(
                id.parse()
                    .map_err(|_| engine_error("invalid callback ID"))?,
            ),
            time,
            position,
        );
        self.publish(transaction)
    }
    #[pyo3(name = "removeUpdater")]
    fn remove_updater(&mut self, target: &MobjectHandle, id: &str, time: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_updater(
            target.handle.node_id(),
            HostCallbackId::new(
                id.parse()
                    .map_err(|_| engine_error("invalid callback ID"))?,
            ),
            time,
        );
        self.publish(transaction)
    }
    #[pyo3(name = "clearUpdaters")]
    fn clear_updaters(&mut self, target: &MobjectHandle, time: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let mut tx = SemanticMutationTransaction::new();
        tx.clear_updaters(target.handle.node_id(), time);
        self.publish(tx)
    }
    fn configure(&mut self, sample_hz: f64) -> PyResult<()> {
        if !sample_hz.is_finite() || sample_hz <= 0.0 {
            return Err(engine_error("sample rate must be positive and finite"));
        }
        if self.execution.is_some() {
            return Err(engine_error("configure sample rate before execution"));
        }
        self.sample_hz = sample_hz;
        Ok(())
    }
    fn retire(&mut self) {
        if let Some(session) = self.execution.as_mut() {
            if let Some(token) = session.pending_callback_token() {
                let _ = session.interrupt_required_callback_phase(token);
            }
        }
        self.terminal = true;
        self.segment = None;
        self.pending_ack = None;
        self.requested_sample = None;
    }
    #[pyo3(name="callbackRotateTransformAboutPoint",signature=(translation_x,translation_y,rotation,scale_x,scale_y,angle,pivot_x,pivot_y))]
    #[allow(clippy::too_many_arguments)]
    fn callback_rotate_transform_about_point(
        &self,
        translation_x: f64,
        translation_y: f64,
        rotation: f64,
        scale_x: f64,
        scale_y: f64,
        angle: f64,
        pivot_x: f64,
        pivot_y: f64,
    ) -> Result<CallbackTransform, PyErr> {
        let transform = noon::integration::rotate_effective_transform_about_point(
            Transform2D {
                translation: Vec2::new(translation_x as f32, translation_y as f32),
                rotation: rotation as f32,
                scale: Vec2::new(scale_x as f32, scale_y as f32),
            },
            angle,
            Vec2::new(pivot_x as f32, pivot_y as f32),
        )
        .map_err(engine_error)?;
        Ok(CallbackTransform { transform })
    }
    #[pyo3(name="callbackPaintSetColor",signature=(fill_red,fill_green,fill_blue,fill_alpha,stroke_red,stroke_green,stroke_blue,stroke_alpha,red,green,blue,alpha))]
    #[allow(clippy::too_many_arguments)]
    fn callback_paint_set_color(
        &self,
        fill_red: Option<f64>,
        fill_green: Option<f64>,
        fill_blue: Option<f64>,
        fill_alpha: Option<f64>,
        stroke_red: Option<f64>,
        stroke_green: Option<f64>,
        stroke_blue: Option<f64>,
        stroke_alpha: Option<f64>,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<CallbackPaint, PyErr> {
        let style = callback_paint_style(
            callback_color("callback fill", fill_red, fill_green, fill_blue, fill_alpha)?,
            callback_color(
                "callback stroke",
                stroke_red,
                stroke_green,
                stroke_blue,
                stroke_alpha,
            )?,
        );
        Ok(callback_paint_result(
            noon::integration::effective_style_with_color(style, red, green, blue, alpha)
                .map_err(engine_error)?,
        ))
    }
    #[pyo3(name="callbackPaintSetOpacity",signature=(fill_red,fill_green,fill_blue,fill_alpha,stroke_red,stroke_green,stroke_blue,stroke_alpha,opacity))]
    #[allow(clippy::too_many_arguments)]
    fn callback_paint_set_opacity(
        &self,
        fill_red: Option<f64>,
        fill_green: Option<f64>,
        fill_blue: Option<f64>,
        fill_alpha: Option<f64>,
        stroke_red: Option<f64>,
        stroke_green: Option<f64>,
        stroke_blue: Option<f64>,
        stroke_alpha: Option<f64>,
        opacity: f64,
    ) -> Result<CallbackPaint, PyErr> {
        let style = callback_paint_style(
            callback_color("callback fill", fill_red, fill_green, fill_blue, fill_alpha)?,
            callback_color(
                "callback stroke",
                stroke_red,
                stroke_green,
                stroke_blue,
                stroke_alpha,
            )?,
        );
        Ok(callback_paint_result(
            noon::integration::effective_style_with_paint_opacity(style, opacity)
                .map_err(engine_error)?,
        ))
    }
    #[pyo3(name="callbackPaintSetFill",signature=(fill_red,fill_green,fill_blue,fill_alpha,stroke_red,stroke_green,stroke_blue,stroke_alpha,color_red,color_green,color_blue,color_alpha,opacity))]
    #[allow(clippy::too_many_arguments)]
    fn callback_paint_set_fill(
        &self,
        fill_red: Option<f64>,
        fill_green: Option<f64>,
        fill_blue: Option<f64>,
        fill_alpha: Option<f64>,
        stroke_red: Option<f64>,
        stroke_green: Option<f64>,
        stroke_blue: Option<f64>,
        stroke_alpha: Option<f64>,
        color_red: Option<f64>,
        color_green: Option<f64>,
        color_blue: Option<f64>,
        color_alpha: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<CallbackPaint, PyErr> {
        let fill = callback_color("callback fill", fill_red, fill_green, fill_blue, fill_alpha)?;
        let color = callback_color(
            "callback requested fill",
            color_red,
            color_green,
            color_blue,
            color_alpha,
        )?;
        let stroke = callback_color(
            "callback stroke",
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
        )?;
        let style = callback_paint_style(fill, stroke);
        let style = match (color, opacity) {
            (Some(color), Some(opacity)) => noon::integration::effective_style_with_fill(
                style,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                opacity,
            ),
            (Some(color), None) => noon::integration::effective_style_with_fill_color(
                style,
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            ),
            (None, Some(opacity)) => {
                noon::integration::effective_style_with_fill_opacity(style, opacity)
            }
            (None, None) => Ok(style),
        }
        .map_err(engine_error)?;
        Ok(callback_paint_result(style))
    }
    #[pyo3(name="callbackPaintSetStroke",signature=(fill_red,fill_green,fill_blue,fill_alpha,stroke_red,stroke_green,stroke_blue,stroke_alpha,color_red,color_green,color_blue,color_alpha))]
    #[allow(clippy::too_many_arguments)]
    fn callback_paint_set_stroke(
        &self,
        fill_red: Option<f64>,
        fill_green: Option<f64>,
        fill_blue: Option<f64>,
        fill_alpha: Option<f64>,
        stroke_red: Option<f64>,
        stroke_green: Option<f64>,
        stroke_blue: Option<f64>,
        stroke_alpha: Option<f64>,
        color_red: f64,
        color_green: f64,
        color_blue: f64,
        color_alpha: f64,
    ) -> Result<CallbackPaint, PyErr> {
        let style = callback_paint_style(
            callback_color("callback fill", fill_red, fill_green, fill_blue, fill_alpha)?,
            callback_color(
                "callback stroke",
                stroke_red,
                stroke_green,
                stroke_blue,
                stroke_alpha,
            )?,
        );
        Ok(callback_paint_result(
            noon::integration::effective_style_with_stroke_color(
                style,
                color_red,
                color_green,
                color_blue,
                color_alpha,
            )
            .map_err(engine_error)?,
        ))
    }

    #[pyo3(name = "queryMobjectFillOpacity")]
    fn query_fill_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.execution.is_some() {
            self.with_live(|live| live.effective(&target.handle))
                .map(|v| v.fill_opacity())
        } else {
            target.handle.fill_opacity().map_err(engine_error)
        }
    }
    #[pyo3(name = "queryMobjectStrokeOpacity")]
    fn query_stroke_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.execution.is_some() {
            self.with_live(|live| live.effective(&target.handle))
                .map(|v| v.stroke_opacity())
        } else {
            target.handle.stroke_opacity().map_err(engine_error)
        }
    }
    #[pyo3(name = "queryMobjectStrokeWidth")]
    fn query_stroke_width(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.execution.is_some() {
            self.with_live(|live| live.effective_stroke_width(&target.handle))
        } else {
            target.handle.stroke_width().map_err(engine_error)
        }
    }
    #[pyo3(name = "liveMoveToPoint")]
    #[allow(clippy::too_many_arguments)]
    fn move_to_point(
        &mut self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| {
            live.move_to(
                &target.handle,
                noon::LiveLayoutTarget::Point(x, y),
                (ex, ey),
                (mx, my),
            )
            .map(|_| ())
        })
    }
    fn runtime_identity(&self) -> PyResult<Option<u64>> {
        self.require_active()?;
        Ok(self.execution.as_ref().map(|s| s.runtime_identity().get()))
    }
    #[pyo3(name = "liveSetStroke")]
    fn set_stroke(
        &mut self,
        target: &MobjectHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_stroke(&target.handle, r, g, b, a).map(|_| ()))
    }
    #[pyo3(name = "liveSetStrokeColor")]
    fn set_stroke_color(
        &mut self,
        target: &MobjectHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| {
            live.set_stroke_color(&target.handle, r, g, b, a)
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveSetStrokeOpacity")]
    fn set_stroke_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.set_stroke_opacity(&target.handle, a).map(|_| ()))
    }
    fn metrics(&self) -> (u64, u64, u64) {
        (self.frames, self.segments, self.callback_regions)
    }
    fn drive(&mut self, py: Python<'_>) -> PyResult<Py<pyo3::types::PyDict>> {
        self.drive_impl(py)
    }
    fn submit_callback(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        value: &Bound<'_, pyo3::types::PyDict>,
    ) -> PyResult<Py<pyo3::types::PyDict>> {
        self.submit_callback_impl(py, token, value)
    }
    fn acknowledge_callback(&mut self, py: Python<'_>, token: &Bound<'_, PyAny>) -> PyResult<()> {
        self.acknowledge_callback_impl(py, token)
    }
    fn read_callback(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        request: &Bound<'_, PyAny>,
    ) -> PyResult<Py<pyo3::types::PyDict>> {
        self.read_callback_impl(py, token, request)
    }
    fn fail_callback(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        message: &str,
    ) -> PyResult<()> {
        self.fail_callback_impl(py, token, message)
    }
}

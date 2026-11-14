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
use noon::integration::scene_execution::ExecutionSession;
use noon::integration::{scene_execution, AuthoringFailure, HostCallbackId, SemanticStore};
use noon::{ExecutionSegment, Mobject, MobjectTarget, Scene, SceneMembershipRequest};
use noon::{Transform2D, Vec2};
use pyo3::prelude::*;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct Store {
    pub(crate) semantics: Rc<RefCell<SemanticStore>>,
    #[cfg(feature = "latex")]
    pub(crate) latex: crate::latex::LatexCompiler,
}
#[pymethods]
impl Store {
    fn mesh(&self, candidate: PyRefMut<'_, crate::spatial::MeshInput>) -> PyResult<MobjectHandle> {
        crate::spatial::authored_mesh(self, candidate)
    }
    fn mesh_family(
        &self,
        candidate: PyRefMut<'_, crate::spatial::MeshFamilyInput>,
    ) -> PyResult<crate::family::FamilyHandle> {
        crate::spatial::authored_family(self, candidate)
    }
    #[cfg(feature = "typst")]
    fn typst(&self, source: &str, math: bool, size: f64) -> PyResult<MobjectHandle> {
        crate::typst::authored(self, source, math, size)
    }
    fn tracker(&self, initial: f64) -> PyResult<crate::signals::ValueTrackerHandle> {
        noon::ValueTracker::detached(Rc::clone(&self.semantics), initial)
            .map(|value| crate::signals::ValueTrackerHandle { value })
            .map_err(engine_error)
    }

    fn coordinates(
        &self,
        options: &crate::coordinates::CoordinateOptions,
    ) -> PyResult<crate::family::FamilyHandle> {
        crate::coordinates::create(&self.semantics, options)
    }

    #[new]
    fn new() -> Self {
        Self {
            semantics: Rc::new(RefCell::new(SemanticStore::new())),
            #[cfg(feature = "latex")]
            latex: Default::default(),
        }
    }
    fn context(&self) -> Context {
        Context::new(Scene::with_integration_store(Rc::clone(&self.semantics)))
    }
    fn family(
        &self,
        batch: &MembershipBatch,
        z_index: f64,
        draw_enabled: bool,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.family_with_color(batch, z_index, draw_enabled, 1.0, 1.0, 1.0, 1.0)
    }
    #[allow(clippy::too_many_arguments)]
    fn family_with_color(
        &self,
        batch: &MembershipBatch,
        z_index: f64,
        draw_enabled: bool,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<crate::family::FamilyHandle> {
        if batch.kind != "add" {
            return Err(engine_error("family creation requires an add batch"));
        }
        noon::MobjectFamily::create_with_presentation(
            Rc::clone(&self.semantics),
            &batch.targets(),
            z_index,
            noon::Color::rgba(r as f32, g as f32, b as f32, a as f32),
            draw_enabled,
        )
        .map(|family| crate::family::FamilyHandle { family })
        .map_err(engine_error)
    }
    #[cfg(feature = "native-text")]
    #[pyo3(signature = (source, family, size, spacing, colors=None, face=None))]
    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        source: &str,
        family: &str,
        size: f64,
        spacing: f64,
        colors: Option<PyRefMut<'_, crate::text::TextColorBatch>>,
        face: Option<PyRef<'_, crate::text::FontFace>>,
    ) -> PyResult<MobjectHandle> {
        crate::text::authored(self, source, family, size, spacing, colors, face, false)
    }
    #[cfg(feature = "native-text")]
    #[pyo3(signature = (source, family, size, spacing, face=None))]
    fn markup_text(
        &self,
        source: &str,
        family: &str,
        size: f64,
        spacing: f64,
        face: Option<PyRef<'_, crate::text::FontFace>>,
    ) -> PyResult<MobjectHandle> {
        crate::text::authored(self, source, family, size, spacing, None, face, true)
    }
    fn geometry(&self, options: &GeometryOptions) -> PyResult<MobjectHandle> {
        Mobject::from_manim_geometry(Rc::clone(&self.semantics), options.options.clone())
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
    }
    fn image(
        &self,
        mut options: PyRefMut<'_, crate::image::ImageOptions>,
    ) -> PyResult<MobjectHandle> {
        Mobject::from_image(Rc::clone(&self.semantics), options.take()?)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
    }
    #[pyo3(signature = (source, should_center, height, width_or_defaults))]
    fn svg(
        &self,
        source: &str,
        should_center: bool,
        height: Option<f64>,
        width_or_defaults: &Bound<'_, PyAny>,
    ) -> PyResult<crate::family::FamilyHandle> {
        crate::svg::authored(self, source, should_center, height, width_or_defaults)
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct MembershipBatch {
    pub(crate) kind: String,
    members: Vec<NativeFamilyMember>,
    bindings: Vec<Mobject>,
    spatial_domain: Option<noon::integration::spatial_input::SpatialMembershipPolicy>,
}
enum NativeFamilyMember {
    Object(Mobject),
    Family(noon::MobjectFamily),
}
impl MembershipBatch {
    pub(crate) fn is_add(&self) -> PyResult<bool> {
        match self.kind.as_str() {
            "add" => Ok(true),
            "remove" => Ok(false),
            _ => Err(engine_error("membership requires add or remove")),
        }
    }

    pub(crate) fn add_targets(&self) -> PyResult<Vec<MobjectTarget<'_>>> {
        if self.kind != "add" {
            return Err(engine_error("membership operation requires an add batch"));
        }
        Ok(self.targets())
    }

    pub(crate) fn single_member(&self) -> PyResult<MobjectTarget<'_>> {
        let targets = self.add_targets()?;
        let [target] = targets.as_slice() else {
            return Err(engine_error("insertion requires exactly one member"));
        };
        Ok(*target)
    }

    pub(crate) fn edit_children(&self, owner: MobjectTarget<'_>) -> PyResult<Vec<bool>> {
        let targets = self.targets();
        if self.is_add()? {
            owner.add_many(&targets)
        } else {
            owner.remove_many(&targets)
        }
        .map_err(engine_error)
    }

    pub(crate) fn targets(&self) -> Vec<MobjectTarget<'_>> {
        self.members
            .iter()
            .map(|member| match member {
                NativeFamilyMember::Object(object) => MobjectTarget::Object(object),
                NativeFamilyMember::Family(family) => MobjectTarget::Family(family),
            })
            .collect()
    }
}
#[pymethods]
impl MembershipBatch {
    #[pyo3(name = "setSpatialDomain")]
    fn spatial_domain(&mut self, value: &str) -> PyResult<()> {
        if self.kind != "add" {
            return Err(engine_error(
                "spatial composition requires an add membership batch",
            ));
        }
        self.spatial_domain = Some(
            noon::integration::spatial_input::SpatialMembershipPolicy::from_name(value)
                .map_err(engine_error)?,
        );
        Ok(())
    }
    #[new]
    fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            members: Vec::new(),
            bindings: Vec::new(),
            spatial_domain: None,
        }
    }
    #[pyo3(name = "reserveMobjectBinding")]
    fn reserve(&mut self, _id: &str, handle: &MobjectHandle) -> PyResult<()> {
        handle.handle.validate().map_err(engine_error)?;
        self.bindings.push(handle.handle.clone());
        Ok(())
    }
    #[pyo3(name = "appendFamily")]
    fn append_family(&mut self, handle: &crate::family::FamilyHandle) -> PyResult<()> {
        handle.family.validate().map_err(engine_error)?;
        self.members
            .push(NativeFamilyMember::Family(handle.family.clone()));
        Ok(())
    }
    #[pyo3(name = "appendMobject")]
    fn append(&mut self, _id: &str, handle: &MobjectHandle) {
        self.members
            .push(NativeFamilyMember::Object(handle.handle.clone()));
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct Context {
    pub(crate) scene: Scene,
    // Host binding provenance only; membership and object state stay in Rust Scene.
    bound_ids: HashSet<noon_core::SemanticNodeId>,
    // Pointer policy declared before the first segment installs the session.
    drag_targets: Vec<Mobject>,
    pub(crate) segment: Option<ExecutionSegment>,
    pub(crate) terminal: bool,
    pub(crate) sample_hz: f64,
    retain_history: bool,
    // Explicit host observation range; never an execution cursor or scheduler.
    observation_horizon: Option<f64>,
    prepare_shaders: bool,
    #[cfg(feature = "capture")]
    effect_preparer: Option<noon_export::SessionCapture>,
    pub(crate) next_sample: u64,
    pub(crate) frames: u64,
    pub(crate) segments: u64,
    pub(crate) requested_sample: Option<f64>,
    pub(crate) pending_ack: Option<noon::integration::CallbackPhaseToken>,
    pub(crate) callback_regions: u64,
    pub(crate) callback_membership: Option<noon::integration::CallbackMembershipCollector>,
    pub(crate) callback_geometry_sources: noon_core::GeometryResourceArena,
}
impl Context {
    fn new(scene: Scene) -> Self {
        Self {
            scene,
            bound_ids: HashSet::new(),
            drag_targets: Vec::new(),
            segment: None,
            terminal: false,
            sample_hz: 60.0,
            retain_history: false,
            observation_horizon: None,
            prepare_shaders: true,
            #[cfg(feature = "capture")]
            effect_preparer: None,
            next_sample: 0,
            frames: 0,
            segments: 0,
            requested_sample: None,
            pending_ack: None,
            callback_regions: 0,
            callback_membership: None,
            callback_geometry_sources: noon_core::GeometryResourceArena::new(),
        }
    }
    fn staged(
        &self,
        pending: noon::StagedEffectPublication,
        bindings: Vec<Mobject>,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        let runtime = scene_execution::session(&self.scene)
            .ok_or_else(|| engine_error("no execution session"))?
            .runtime_identity();
        Ok(crate::effects::StagedEffectPublication {
            pending: Some(pending),
            runtime,
            bindings,
        })
    }
    fn bootstrap(&self) -> PyResult<ExecutionSession> {
        let mut session =
            scene_execution::lower(&self.scene).map_err(|e| engine_error(e.to_string()))?;
        session.require_effect_resource_preparation();
        // Predeclared audio uses the immutable timeline for preview and export.
        if !self.drag_targets.is_empty() {
            scene_execution::borrow_uninstalled(&self.scene, &mut session)
                .set_translation_drag_targets(&self.drag_targets)
                .map_err(engine_error)?;
        } else if self.retain_history && !session.has_audio() {
            session
                .begin_replay_retention(Default::default())
                .map_err(|e| engine_error(e.to_string()))?;
        }
        Ok(session)
    }

    fn attach_session(&mut self, session: ExecutionSession) -> PyResult<()> {
        scene_execution::attach(&mut self.scene, session)
            .map_err(|error| engine_error(error.to_string()))?;
        self.drag_targets.clear();
        Ok(())
    }

    fn with_bootstrapped_live<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> PyResult<T> {
        if self.has_execution() {
            return self.with_live(operation);
        }
        let mut session = self.bootstrap()?;
        let mut live = scene_execution::borrow_uninstalled(&self.scene, &mut session);
        let result = operation(&mut live).map_err(engine_error)?;
        self.attach_session(session)?;
        Ok(result)
    }

    pub(crate) fn prepare_viewer(&mut self) -> PyResult<noon::SceneExecution> {
        self.available()?;
        if !self.has_execution() {
            let session = self.bootstrap()?;
            self.attach_session(session)?;
        }
        let execution = scene_execution::take_for_native_viewer(&mut self.scene).map_err(
            |error| match error {
                scene_execution::NativeViewerError::HistoryRequired => engine_error(
                    "prepare the source with retain_history=True before native preview",
                ),
                scene_execution::NativeViewerError::Replay(error) => engine_error(
                    AuthoringFailure::new("unsupported_operation", "python.preview_history", error),
                ),
                error => engine_error(error.to_string()),
            },
        )?;
        #[cfg(feature = "capture")]
        {
            self.effect_preparer = None;
        }
        self.terminal = true;
        Ok(execution)
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
    fn same_family_store(&self, target: &crate::family::FamilyHandle) -> PyResult<()> {
        if !Rc::ptr_eq(
            self.scene.integration_store(),
            target.family.integration_store(),
        ) {
            return Err(engine_error(noon::AuthoringError::ForeignStore));
        }
        target.family.validate().map_err(engine_error)
    }
    pub(crate) fn available(&self) -> PyResult<()> {
        self.require_active()?;
        if self.segment.is_some() {
            Err(engine_error(
                noon::ExecutionSessionPublicationError::SegmentCompletionPending,
            ))
        } else {
            Ok(())
        }
    }
    pub(crate) fn has_execution(&self) -> bool {
        scene_execution::session(&self.scene).is_some()
    }

    pub(crate) fn live_session(&mut self) -> Option<noon::LiveSession<'_>> {
        scene_execution::live(&mut self.scene)
    }

    pub(crate) fn with_live<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> PyResult<T> {
        self.require_active()?;
        let mut live = self
            .live_session()
            .ok_or_else(|| engine_error("no execution session"))?;
        operation(&mut live).map_err(engine_error)
    }

    fn observe_bound_mobject<T>(
        &self,
        target: &MobjectHandle,
        observe: impl FnOnce(&Scene, &Mobject) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<T> {
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        self.observe_mobject(target, observe)
    }

    fn observe_mobject<T>(
        &self,
        target: &MobjectHandle,
        observe: impl FnOnce(&Scene, &Mobject) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<T> {
        let running = self.has_execution();
        observe(&self.scene, &target.handle)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    fn place<T>(
        &mut self,
        operation: impl FnOnce(&mut Scene) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<()> {
        self.require_active()?;
        let running = self.has_execution();
        operation(&mut self.scene).map(|_| ()).map_err(|error| {
            if running {
                engine_error(noon::LiveSessionError::from(error))
            } else {
                engine_error(error)
            }
        })
    }

    fn promote_own_points(
        &mut self,
        target: MobjectTarget<'_>,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.available()?;
        self.scene
            .promote_own_points(target, &references.add_targets()?)
            .map(|copied| crate::family::FamilyCopy { copied })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    fn edit_children(
        &mut self,
        owner: MobjectTarget<'_>,
        batch: &MembershipBatch,
    ) -> PyResult<Vec<bool>> {
        self.available()?;
        let running = self.has_execution();
        let adding = batch.is_add()?;
        let members = batch.targets();
        if adding {
            self.scene.add_family_members(owner, &members)
        } else {
            self.scene.remove_family_members(owner, &members)
        }
        .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    fn prepare_children(
        &mut self,
        owner: MobjectTarget<'_>,
        batch: &MembershipBatch,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.available()?;
        let members = batch.add_targets()?;
        let pending = self.with_effect_preparation(|scene| {
            scene
                .prepare_family_addition(owner, &members)
                .map(|(pending, _)| pending)
        })?;
        self.staged(pending, batch.bindings.clone())
    }

    fn with_effect_preparation<T>(
        &mut self,
        prepare: impl FnOnce(&mut Scene) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<T> {
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        prepare(&mut self.scene)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }

    fn observe_points<'a, T>(
        &self,
        target: MobjectTarget<'a>,
        observe: impl FnOnce(&Scene, MobjectTarget<'a>) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<T> {
        self.require_active()?;
        observe(&self.scene, target).map_err(engine_error)
    }

    fn critical_point_3d(
        &self,
        target: MobjectTarget<'_>,
        direction: noon::SemanticVec3,
        vector_boundary: bool,
    ) -> PyResult<(f64, f64, f64)> {
        self.observe_points(target, |scene, target| {
            scene.critical_point_3d(target, direction, vector_boundary)
        })
        .map(|point| (point.x, point.y, point.z))
    }

    fn edit_path(
        &mut self,
        target: &MobjectHandle,
        edit: impl FnOnce(&mut Scene, &noon::Mobject) -> Result<(), noon::AuthoringError>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        edit(&mut self.scene, &target.handle).map_err(engine_error)
    }
    fn publish_path_edit(
        &mut self,
        edit: impl FnOnce(&mut Scene) -> Result<(), noon::AuthoringError>,
    ) -> PyResult<()> {
        self.require_active()?;
        let running = self.has_execution();
        edit(&mut self.scene).map_err(|error| {
            if running {
                engine_error(noon::LiveSessionError::from(error))
            } else {
                engine_error(error)
            }
        })
    }
    fn edit_scene_property(
        &mut self,
        edit: impl FnOnce(&mut Scene) -> Result<(), noon::AuthoringError>,
    ) -> PyResult<()> {
        self.require_active()?;
        let running = self.has_execution();
        edit(&mut self.scene)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    fn edit_scene<T>(
        &mut self,
        edit: impl FnOnce(&mut Scene) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<()> {
        self.available()?;
        let running = self.has_execution();
        edit(&mut self.scene)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl Context {
    #[pyo3(name = "callbackLineTarget")]
    fn callback_line_target(
        &self,
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> PyResult<CallbackLineTarget> {
        self.require_active()?;
        let point = |x, y| {
            noon::integration::authoring_xy_f64(x, y)
                .map(|value| Vec2::new(value.x as f32, value.y as f32))
                .map_err(engine_error)
        };
        Ok(CallbackLineTarget {
            start: point(start_x, start_y)?,
            end: point(end_x, end_y)?,
            store: Rc::clone(self.scene.integration_store()),
        })
    }
    #[pyo3(name = "callbackMatchLineTransform")]
    fn callback_match_line_transform(
        &self,
        source: &MobjectHandle,
        target: &CallbackLineTarget,
    ) -> PyResult<CallbackTransform> {
        self.require_active()?;
        self.same_store(source)?;
        if !Rc::ptr_eq(self.scene.integration_store(), &target.store) {
            return Err(engine_error(noon::AuthoringError::ForeignStore));
        }
        source
            .handle
            .line_match_transform(target.start, target.end)
            .map(|transform| CallbackTransform { transform })
            .map_err(engine_error)
    }
    #[allow(clippy::too_many_arguments)]
    #[pyo3(name="setStyle",signature=(source,fill_enabled,fill_red,fill_green,fill_blue,fill_alpha,fill_opacity,stroke_enabled,stroke_red,stroke_green,stroke_blue,stroke_alpha,stroke_width,stroke_opacity,background_stroke_enabled,background_stroke_red,background_stroke_green,background_stroke_blue,background_stroke_alpha,background_stroke_width,background_stroke_opacity))]
    fn set_style(
        &mut self,
        source: &MobjectHandle,
        fill_enabled: bool,
        fill_red: f64,
        fill_green: f64,
        fill_blue: f64,
        fill_alpha: f64,
        fill_opacity: Option<f64>,
        stroke_enabled: bool,
        stroke_red: f64,
        stroke_green: f64,
        stroke_blue: f64,
        stroke_alpha: f64,
        stroke_width: Option<f64>,
        stroke_opacity: Option<f64>,
        background_stroke_enabled: bool,
        background_stroke_red: f64,
        background_stroke_green: f64,
        background_stroke_blue: f64,
        background_stroke_alpha: f64,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        self.available()?;
        self.same_store(source)?;
        let update = noon::integration::style_update(
            fill_enabled,
            fill_red,
            fill_green,
            fill_blue,
            fill_alpha,
            fill_opacity,
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
            stroke_width,
            stroke_opacity,
        )
        .map_err(engine_error)?;
        let update = noon::integration::style_update_with_background_stroke(
            update,
            background_stroke_enabled,
            [
                background_stroke_red,
                background_stroke_green,
                background_stroke_blue,
                background_stroke_alpha,
            ],
            background_stroke_width,
            background_stroke_opacity,
        )
        .map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_style(&source.handle, update))
    }
    #[allow(clippy::too_many_arguments)]
    #[pyo3(name="setFamilyStyle",signature=(source,fill_enabled,fill_red,fill_green,fill_blue,fill_alpha,fill_opacity,stroke_enabled,stroke_red,stroke_green,stroke_blue,stroke_alpha,stroke_width,stroke_opacity,background_stroke_enabled,background_stroke_red,background_stroke_green,background_stroke_blue,background_stroke_alpha,background_stroke_width,background_stroke_opacity))]
    fn set_family_style(
        &mut self,
        source: &crate::family::FamilyHandle,
        fill_enabled: bool,
        fill_red: f64,
        fill_green: f64,
        fill_blue: f64,
        fill_alpha: f64,
        fill_opacity: Option<f64>,
        stroke_enabled: bool,
        stroke_red: f64,
        stroke_green: f64,
        stroke_blue: f64,
        stroke_alpha: f64,
        stroke_width: Option<f64>,
        stroke_opacity: Option<f64>,
        background_stroke_enabled: bool,
        background_stroke_red: f64,
        background_stroke_green: f64,
        background_stroke_blue: f64,
        background_stroke_alpha: f64,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> Result<(), PyErr> {
        self.available()?;
        self.same_family_store(source)?;
        let update = noon::integration::style_update(
            fill_enabled,
            fill_red,
            fill_green,
            fill_blue,
            fill_alpha,
            fill_opacity,
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
            stroke_width,
            stroke_opacity,
        )
        .map_err(engine_error)?;
        let update = noon::integration::style_update_with_background_stroke(
            update,
            background_stroke_enabled,
            [
                background_stroke_red,
                background_stroke_green,
                background_stroke_blue,
                background_stroke_alpha,
            ],
            background_stroke_width,
            background_stroke_opacity,
        )
        .map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_family_style(&source.family, update).map(|_| ()))
    }
    #[pyo3(name = "createMesh")]
    fn create_mesh(
        &mut self,
        candidate: PyRefMut<'_, crate::spatial::MeshInput>,
    ) -> PyResult<MobjectHandle> {
        crate::spatial::create_mesh(self, candidate)
    }
    #[pyo3(name = "createMeshFamily")]
    fn create_mesh_family(
        &mut self,
        candidate: PyRefMut<'_, crate::spatial::MeshFamilyInput>,
    ) -> PyResult<crate::family::FamilyHandle> {
        crate::spatial::create_family(self, candidate)
    }
    #[pyo3(name = "createCamera3D")]
    fn camera_3d(
        &mut self,
        _id: &str,
        position: Vec<f64>,
        rotation: Vec<f64>,
        perspective_fov: f64,
        near: f64,
        far: f64,
    ) -> PyResult<MobjectHandle> {
        self.available()?;
        let position = noon::integration::spatial_input::vec3(&position, "camera position")
            .map_err(engine_error)?;
        let [w, x, y, z] = rotation.as_slice() else {
            return Err(engine_error(
                "camera rotation requires quaternion [w,x,y,z]",
            ));
        };
        let orientation = noon_core::SemanticRotation3D::from_components(*w, *x, *y, *z)
            .ok_or_else(|| engine_error("invalid camera rotation"))?;
        let camera = noon::SemanticCamera3D::new(
            position,
            orientation,
            noon::SemanticProjection3D::Perspective {
                vertical_fov_radians: perspective_fov,
                near,
                far,
            },
        )
        .ok_or_else(|| engine_error("invalid camera projection or pose"))?;
        let object = self.scene.camera_3d(camera).map_err(engine_error)?;
        self.bound_ids.insert(object.node_id());
        Ok(MobjectHandle::from_semantic_mobject(object))
    }
    #[pyo3(name = "createCamera3DProfile")]
    fn camera_profile(
        &mut self,
        _id: &str,
        values: Vec<f64>,
        near: f64,
        far: f64,
    ) -> PyResult<MobjectHandle> {
        self.available()?;
        let profile =
            noon::integration::spatial_input::camera_profile(&values).map_err(engine_error)?;
        let object = self
            .scene
            .camera_3d_profile(profile, near, far)
            .map_err(engine_error)?;
        self.bound_ids.insert(object.node_id());
        Ok(MobjectHandle::from_semantic_mobject(object))
    }
    #[pyo3(name = "createPointLight3D")]
    fn point_light(
        &mut self,
        position: Vec<f64>,
        rgba: Vec<f64>,
        intensity: f64,
    ) -> PyResult<MobjectHandle> {
        self.available()?;
        let position = noon::integration::spatial_input::vec3(&position, "light position")
            .map_err(engine_error)?;
        let [r, g, b, a] = rgba.as_slice() else {
            return Err(engine_error("point light color requires RGBA components"));
        };
        let color =
            noon::integration::spatial_input::color(*r, *g, *b, *a).map_err(engine_error)?;
        let running = self.has_execution();
        self.scene
            .point_light_3d(position, color, intensity)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "setWorldTransform")]
    fn set_world_transform(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let transform = noon::integration::spatial_input::world_transform_from_values(&values)
            .map_err(engine_error)?;
        let running = self.has_execution();
        self.scene
            .set_world_transform(&target.handle, transform)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "effectiveWorldTransform")]
    fn world_transform(&mut self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_store(target)?;
        let running = self.has_execution();
        let value = self
            .scene
            .world_transform(&target.handle)
            .map_err(|error| {
                if running {
                    engine_error(noon::LiveSessionError::from(error))
                } else {
                    engine_error(error)
                }
            })?;
        Ok(noon::integration::spatial_input::transform_values(value))
    }
    #[pyo3(name = "effectiveWorldCenter")]
    fn world_center(&mut self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_store(target)?;
        let running = self.has_execution();
        let value = self
            .scene
            .effective_world_center(&target.handle)
            .map_err(|error| {
                engine_error(if running {
                    AuthoringFailure::from(noon::LiveSessionError::from(error))
                } else {
                    error.into()
                })
            })?;
        Ok(vec![value.x, value.y, value.z])
    }
    #[pyo3(name = "effectiveWorldFamilyCenter")]
    fn world_family_center(&mut self, target: &crate::family::FamilyHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_family_store(target)?;
        let running = self.has_execution();
        let value = self
            .scene
            .effective_world_family_center(&target.family)
            .map_err(|error| {
                engine_error(if running {
                    AuthoringFailure::from(noon::LiveSessionError::from(error))
                } else {
                    error.into()
                })
            })?;
        Ok(vec![value.x, value.y, value.z])
    }
    #[pyo3(name = "effectiveCameraProfile")]
    fn effective_camera_profile(&mut self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_store(target)?;
        let running = self.has_execution();
        let value = self
            .scene
            .effective_camera_profile(&target.handle)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))?;
        Ok(noon::integration::spatial_input::camera_profile_values(
            value.0,
        ))
    }
    #[pyo3(name = "setCameraProfile")]
    fn set_camera_profile(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let profile =
            noon::integration::spatial_input::camera_profile(&values).map_err(engine_error)?;
        let running = self.has_execution();
        self.scene
            .set_camera_profile(&target.handle, profile)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "setSurfaceCheckerboard")]
    fn set_surface_checkerboard(
        &mut self,
        target: &crate::family::FamilyHandle,
        first: Vec<f64>,
        second: Vec<f64>,
        opacity: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        let surface =
            noon::SurfaceFamily::from_family(target.family.clone()).map_err(engine_error)?;
        let colors = crate::spatial::checkerboard(&first, &second)?;
        let running = self.has_execution();
        self.scene
            .set_surface_checkerboard(&surface, colors, opacity)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "shiftWorld")]
    fn shift_world(&mut self, target: &MobjectHandle, x: f64, y: f64, z: f64) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Object(&target.handle),
            noon::WorldAffineEdit::Shift(noon::SemanticVec3::new(x, y, z)),
        )
    }
    #[pyo3(name = "rotateWorld")]
    fn rotate_world(
        &mut self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        angle: f64,
        pivot: Vec<f64>,
    ) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Object(&target.handle),
            noon::WorldAffineEdit::Rotate {
                axis: noon::SemanticVec3::new(x, y, z),
                radians: angle,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            },
        )
    }
    #[pyo3(name = "scaleWorld")]
    fn scale_world(
        &mut self,
        target: &MobjectHandle,
        factor: f64,
        pivot: Vec<f64>,
    ) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Object(&target.handle),
            noon::WorldAffineEdit::Scale {
                factor,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            },
        )
    }
    #[pyo3(name = "shiftFamilyWorld")]
    fn family_shift_world(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Family(&target.family),
            noon::WorldAffineEdit::Shift(noon::SemanticVec3::new(x, y, z)),
        )
    }
    #[pyo3(name = "rotateFamilyWorld")]
    fn family_rotate_world(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        z: f64,
        angle: f64,
        pivot: Vec<f64>,
    ) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Family(&target.family),
            noon::WorldAffineEdit::Rotate {
                axis: noon::SemanticVec3::new(x, y, z),
                radians: angle,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            },
        )
    }
    #[pyo3(name = "scaleFamilyWorld")]
    fn family_scale_world(
        &mut self,
        target: &crate::family::FamilyHandle,
        factor: f64,
        pivot: Vec<f64>,
    ) -> PyResult<()> {
        crate::spatial::world_affine(
            self,
            noon::MobjectTarget::Family(&target.family),
            noon::WorldAffineEdit::Scale {
                factor,
                about: noon::integration::spatial_input::about(&pivot).map_err(engine_error)?,
            },
        )
    }
    #[pyo3(name = "beginAmbientCameraRotation")]
    fn begin_ambient_camera_rotation(
        &mut self,
        target: &MobjectHandle,
        rate: f64,
        axis: &str,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let axis = match axis {
            "phi" => noon_core::CameraRotationAxis::Phi,
            "theta" => noon_core::CameraRotationAxis::Theta,
            "gamma" => noon_core::CameraRotationAxis::Gamma,
            _ => return Err(engine_error("unknown ambient camera rotation axis")),
        };
        let running = self.has_execution();
        self.scene
            .begin_ambient_camera_rotation(&target.handle, axis, rate)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "stopAmbientCameraRotation")]
    fn stop_ambient_camera_rotation(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let running = self.has_execution();
        self.scene
            .stop_ambient_camera_rotation(&target.handle)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "begin3DIllusionCameraRotation")]
    #[pyo3(signature=(target,rate,origin_phi,origin_theta))]
    fn begin_3dillusion_camera_rotation(
        &mut self,
        target: &MobjectHandle,
        rate: f64,
        origin_phi: Option<f64>,
        origin_theta: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let running = self.has_execution();
        self.scene
            .begin_3dillusion_camera_rotation(&target.handle, rate, origin_phi, origin_theta)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "stop3DIllusionCameraRotation")]
    fn stop_3dillusion_camera_rotation(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let running = self.has_execution();
        self.scene
            .stop_3dillusion_camera_rotation(&target.handle)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    fn audio(
        &mut self,
        source: &crate::audio::AudioSource,
        options: &crate::audio::ClipOptions,
    ) -> PyResult<crate::audio::AudioClip> {
        self.available()?;
        if self.has_execution() {
            return Err(engine_error(AuthoringFailure::new(
                "unsupported_operation",
                "authoring.finite_media_after_execution",
                "audio and playback-control declarations must precede execution",
            )));
        }
        let node = self
            .scene
            .audio(source.0.clone(), options.0)
            .map_err(engine_error)?;
        Ok(crate::audio::AudioClip {
            store: Rc::clone(self.scene.integration_store()),
            node,
        })
    }

    #[pyo3(name = "authoredWait")]
    fn authored_wait(&mut self, duration: f64) -> PyResult<f64> {
        self.available()?;
        if self.has_execution() {
            return Err(engine_error(
                "authored waits require a Scene before execution",
            ));
        }
        self.scene.wait(duration).map_err(engine_error)?;
        Ok(self.scene.time())
    }

    #[pyo3(name = "declareValue")]
    fn declare_value(
        &mut self,
        tracker: &crate::signals::ValueTrackerHandle,
        target: f64,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        self.available()?;
        if self.has_execution() {
            return Err(engine_error(
                "scalar declarations require a Scene before execution",
            ));
        }
        self.scene
            .play_value(&tracker.value, target)
            .rate_func(crate::composition::rate(easing)?)
            .run_time(duration)
            .map(|_| ())
            .map_err(engine_error)
    }

    #[cfg(feature = "latex")]
    #[pyo3(name = "setLatexMemberColors")]
    fn set_latex_member_colors(
        &mut self,
        handle: &crate::latex::LatexHandle,
        rgba: Vec<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let colors = noon::integration::latex_member_colors(&rgba).map_err(engine_error)?;
        let running = self.has_execution();
        self.scene
            .set_family_member_colors(handle.0.family(), &colors)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    #[cfg(feature = "latex")]
    #[pyo3(name = "latexFontSize")]
    fn latex_font_size(&self, handle: &crate::latex::LatexHandle) -> PyResult<f64> {
        self.require_active()?;
        self.scene.latex_font_size(&handle.0).map_err(engine_error)
    }

    #[pyo3(name = "createValueTracker")]
    fn create_tracker(&mut self, initial: f64) -> PyResult<crate::signals::ValueTrackerHandle> {
        self.available()?;
        self.scene
            .value_tracker(initial)
            .map(|value| crate::signals::ValueTrackerHandle { value })
            .map_err(engine_error)
    }
    #[pyo3(name = "associateValueTracker")]
    fn associate_tracker(&mut self, tracker: &crate::signals::ValueTrackerHandle) -> PyResult<()> {
        self.available()?;
        self.scene
            .associate_value_tracker(&tracker.value)
            .map_err(engine_error)
    }
    #[pyo3(name = "valueTrackerValue")]
    fn tracker_value(&self, tracker: &crate::signals::ValueTrackerHandle) -> PyResult<f64> {
        self.require_active()?;
        self.scene
            .value_tracker_value(&tracker.value)
            .map_err(|error| {
                engine_error(AuthoringFailure::from_scene_mutation(
                    error,
                    self.has_execution(),
                ))
            })
    }
    #[pyo3(name = "setValueTracker")]
    fn set_tracker(
        &mut self,
        tracker: &crate::signals::ValueTrackerHandle,
        value: f64,
    ) -> PyResult<()> {
        self.available()?;
        let running = self.has_execution();
        self.scene
            .set_value(&tracker.value, value)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "trackerPosition")]
    fn tracker_position(
        &mut self,
        tracker: &crate::signals::ValueTrackerHandle,
        dx: f64,
        dy: f64,
        x: f64,
        y: f64,
    ) -> PyResult<crate::signals::TrackerPositionHandle> {
        self.available()?;
        self.scene
            .position_from_tracker(
                &tracker.value,
                noon::SemanticVec3::new(dx, dy, 0.),
                noon::SemanticVec3::new(x, y, 0.),
            )
            .map(|value| crate::signals::TrackerPositionHandle { value })
            .map_err(engine_error)
    }
    #[pyo3(name = "bindTrackerPosition")]
    fn bind_tracker_position(
        &mut self,
        target: &MobjectHandle,
        position: &crate::signals::TrackerPositionHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.scene
            .bind_position(&target.handle, &position.value)
            .map_err(engine_error)
    }
    #[pyo3(name = "bindRotation")]
    fn bind_tracker_rotation(
        &mut self,
        target: &MobjectHandle,
        tracker: &crate::signals::ValueTrackerHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.scene
            .bind_rotation(&target.handle, &tracker.value)
            .map_err(engine_error)
    }
    #[pyo3(name = "bindOpacity")]
    fn bind_tracker_opacity(
        &mut self,
        target: &MobjectHandle,
        tracker: &crate::signals::ValueTrackerHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.scene
            .bind_opacity(&target.handle, &tracker.value)
            .map_err(engine_error)
    }

    #[pyo3(name = "nextLayoutTo")]
    fn next_layout_to_anchor(
        &mut self,
        source: &LayoutAnchor,
        target: &LayoutAnchor,
        aligner: &LayoutAnchor,
        dx: f64,
        dy: f64,
        buff: f64,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.next_layout_to_aligned(
                &source.anchor,
                noon::LayoutTarget::Anchor(&target.anchor),
                &aligner.anchor,
                noon::ManimNextToArgs {
                    direction: (dx, dy),
                    buff,
                    aligned_edge: (ex, ey),
                    mask: (mx, my),
                },
            )
        })
    }
    #[pyo3(name = "nextLayoutToPoint")]
    fn next_layout_to_point(
        &mut self,
        source: &LayoutAnchor,
        x: f64,
        y: f64,
        aligner: &LayoutAnchor,
        dx: f64,
        dy: f64,
        buff: f64,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.next_layout_to_aligned(
                &source.anchor,
                noon::LayoutTarget::Point(x, y),
                &aligner.anchor,
                noon::ManimNextToArgs {
                    direction: (dx, dy),
                    buff,
                    aligned_edge: (ex, ey),
                    mask: (mx, my),
                },
            )
        })
    }
    #[pyo3(name = "nextLayoutToEdgeInFrame")]
    fn next_layout_edge(
        &mut self,
        source: &LayoutAnchor,
        target: &LayoutAnchor,
        ex: f64,
        ey: f64,
        dx: f64,
        dy: f64,
        buff: f64,
        frame_buff: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene
                .next_layout_to_edge_in_frame(
                    &source.anchor,
                    &target.anchor,
                    (ex, ey),
                    noon::ManimNextToArgs {
                        direction: (dx, dy),
                        buff,
                        aligned_edge: (0., 0.),
                        mask: (1., 1.),
                    },
                    frame_buff,
                )
                .map(|_| ())
        })
    }
    #[pyo3(name = "arrangeFamily")]
    fn arrange_family_with_options(
        &mut self,
        target: &crate::family::FamilyHandle,
        options: &crate::family::FamilyArrangeOptions,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| scene.arrange_family_with_options(&target.family, &options.options))
    }
    #[pyo3(name = "arrangeFamilyInGrid")]
    fn arrange_family_in_grid_with_options(
        &mut self,
        target: &crate::family::FamilyHandle,
        options: &crate::family::FamilyGridOptions,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| {
            scene.arrange_family_in_grid_with_options(&target.family, &options.options)
        })
    }
    #[pyo3(name = "moveFamilyToPoint")]
    fn move_family_to_point(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| {
            scene.move_family_to(
                &target.family,
                noon::LayoutTarget::Point(x, y),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "moveFamilyToMobject")]
    fn move_family_to_mobject(
        &mut self,
        target: &crate::family::FamilyHandle,
        other: &MobjectHandle,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.same_store(other)?;
        self.place(|scene| {
            scene.move_family_to(
                &target.family,
                noon::LayoutTarget::Mobject(&other.handle),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "moveFamilyToFamily")]
    fn move_family_to_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        other: &crate::family::FamilyHandle,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.same_family_store(other)?;
        self.place(|scene| {
            scene.move_family_to(
                &target.family,
                noon::LayoutTarget::Family(&other.family),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "alignFamilyToPoint")]
    fn align_family_to_point(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        ex: f64,
        ey: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| {
            scene.align_family_to(&target.family, noon::LayoutTarget::Point(x, y), (ex, ey))
        })
    }
    #[pyo3(name = "alignFamilyToMobject")]
    fn align_family_to_mobject(
        &mut self,
        target: &crate::family::FamilyHandle,
        other: &MobjectHandle,
        ex: f64,
        ey: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.same_store(other)?;
        self.place(|scene| {
            scene.align_family_to(
                &target.family,
                noon::LayoutTarget::Mobject(&other.handle),
                (ex, ey),
            )
        })
    }
    #[pyo3(name = "alignFamilyToFamily")]
    fn align_family_to_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        other: &crate::family::FamilyHandle,
        ex: f64,
        ey: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.same_family_store(other)?;
        self.place(|scene| {
            scene.align_family_to(
                &target.family,
                noon::LayoutTarget::Family(&other.family),
                (ex, ey),
            )
        })
    }
    #[pyo3(name = "alignFamilyOnFrame")]
    fn align_family_on_frame(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        buff: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| scene.align_family_on_frame(&target.family, (x, y), buff))
    }

    #[pyo3(name = "setFamilyObjectOpacity")]
    fn family_object_opacity(
        &mut self,
        target: &crate::family::FamilyHandle,
        value: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.edit_scene(|scene| {
            scene
                .set_family_object_opacity(&target.family, value)
                .map(|_| ())
        })
    }

    #[pyo3(name = "createCameraFrame")]
    fn camera_frame(&mut self, _id: &str) -> PyResult<MobjectHandle> {
        self.available()?;
        let handle = self.scene.camera_frame().map_err(engine_error)?;
        self.bound_ids.insert(handle.node_id());
        Ok(MobjectHandle::from_semantic_mobject(handle))
    }
    #[pyo3(name = "setViewColorProfile")]
    fn view_color_profile(&mut self, target: &MobjectHandle, value: &str) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let value = noon::integration::camera_color_profile(value).map_err(engine_error)?;
        let running = self.has_execution();
        self.scene
            .set_view_color_profile(&target.handle, value)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "setEmission")]
    fn emission(&mut self, target: &MobjectHandle, value: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.edit_scene(|scene| scene.set_emission(&target.handle, value))
    }
    #[pyo3(name = "setFamilyEmission")]
    fn family_emission(
        &mut self,
        target: &crate::family::FamilyHandle,
        value: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        let emission = noon_core::Emission::new(value)
            .map_err(noon::AuthoringError::Emission)
            .map_err(engine_error)?;
        self.edit_scene(|scene| {
            scene
                .set_family_style(
                    &target.family,
                    noon::StyleUpdate {
                        emission: Some(emission),
                        ..Default::default()
                    },
                )
                .map(|_| ())
        })
    }

    #[pyo3(name = "queryNumberPlaneFrame")]
    fn query_number_plane_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::AxesObservation> {
        self.require_active()?;
        let plane = noon::ManimNumberPlane::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        self.scene
            .number_plane_frame(&plane)
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryPolarPlaneFrame")]
    fn query_polar_plane_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::PolarObservation> {
        self.require_active()?;
        let plane = noon::ManimPolarPlane::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        self.scene
            .polar_plane_frame(&plane)
            .map(|frame| crate::coordinates::PolarObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "queryNumberLineFrame")]
    fn query_number_line_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::NumberLineObservation> {
        self.require_active()?;
        let coordinates = noon::ManimNumberLine::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        self.scene
            .number_line_frame(&coordinates)
            .map(|frame| crate::coordinates::NumberLineObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryAxesFrame")]
    fn query_axes_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::AxesObservation> {
        self.require_active()?;
        let coordinates = noon::ManimAxes::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        self.scene
            .axes_frame(&coordinates)
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryThreeDAxesFrame")]
    fn query_three_d_axes_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::ThreeDAxesObservation> {
        self.require_active()?;
        let coordinates = noon::ManimThreeDAxes::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        self.scene
            .three_d_axes_frame(&coordinates)
            .map(|frame| crate::coordinates::ThreeDAxesObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[cfg(feature = "native-text")]
    #[pyo3(name = "createNumberLineTextLabels")]
    fn create_number_line_text_labels(
        &mut self,
        family: &crate::family::FamilyHandle,
        values: Vec<f64>,
        automatic: bool,
        options: &crate::coordinates::NumberLabelOptions,
        attach: bool,
    ) -> PyResult<crate::family::FamilyHandle> {
        use crate::coordinates::{label_failure, label_numbers};
        self.available()?;
        let line =
            noon::ManimNumberLine::from_family(family.family.clone()).map_err(label_failure)?;
        self.scene
            .number_line_labels(
                &line,
                label_numbers(&values, automatic)?,
                &options.options,
                attach,
            )
            .map(crate::family::FamilyHandle::from_semantic_family)
            .map_err(label_failure)
    }

    #[cfg(feature = "native-text")]
    #[pyo3(name = "createNumberLineCustomLabels")]
    fn create_number_line_custom_labels(
        &mut self,
        family: &crate::family::FamilyHandle,
        values: Vec<f64>,
        texts: Vec<String>,
        options: &crate::coordinates::NumberLabelOptions,
    ) -> PyResult<crate::family::FamilyHandle> {
        use crate::coordinates::{custom_label_values, label_failure};
        self.available()?;
        let line =
            noon::ManimNumberLine::from_family(family.family.clone()).map_err(label_failure)?;
        self.scene
            .number_line_custom_labels(
                &line,
                &custom_label_values(values, texts)?,
                &options.options,
            )
            .map(crate::family::FamilyHandle::from_semantic_family)
            .map_err(label_failure)
    }

    #[cfg(feature = "native-text")]
    #[pyo3(name = "createCoordinateTextLabels")]
    #[allow(clippy::too_many_arguments)]
    fn create_coordinate_text_labels(
        &mut self,
        x_axis: &crate::family::FamilyHandle,
        y_axis: &crate::family::FamilyHandle,
        x: Vec<f64>,
        automatic_x: bool,
        y: Vec<f64>,
        automatic_y: bool,
        x_options: &crate::coordinates::NumberLabelOptions,
        y_options: &crate::coordinates::NumberLabelOptions,
    ) -> PyResult<(crate::family::FamilyHandle, crate::family::FamilyHandle)> {
        use crate::coordinates::{label_failure, label_numbers};
        self.available()?;
        let x_axis =
            noon::ManimNumberLine::from_family(x_axis.family.clone()).map_err(label_failure)?;
        let y_axis =
            noon::ManimNumberLine::from_family(y_axis.family.clone()).map_err(label_failure)?;
        let values = [
            label_numbers(&x, automatic_x)?,
            label_numbers(&y, automatic_y)?,
        ];
        let [x, y] = self
            .scene
            .coordinate_text_labels(
                &x_axis,
                &y_axis,
                values,
                [&x_options.options, &y_options.options],
            )
            .map_err(label_failure)?
            .map(crate::family::FamilyHandle::from_semantic_family);
        Ok((x, y))
    }

    #[pyo3(name = "createThreeDAxesLabels")]
    #[allow(clippy::too_many_arguments)]
    fn create_three_d_axes_labels(
        &mut self,
        axes: &crate::family::FamilyHandle,
        x: &crate::family::FamilyHandle,
        y: &crate::family::FamilyHandle,
        z: &crate::family::FamilyHandle,
        selected_axes: u32,
        buff: f64,
        fixed_orientation: bool,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        let axes = noon::ManimThreeDAxes::from_family(axes.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let labels = noon::integration::three_d_axis_label_targets(
            [&x.family, &y.family, &z.family],
            selected_axes,
        )
        .map_err(engine_error)?;
        self.scene
            .three_d_axis_labels(&axes, &labels, buff, fixed_orientation)
            .map(crate::family::FamilyHandle::from_semantic_family)
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "mobjectShapeMatcherBounds")]
    fn mobject_shape_matcher_bounds(&self, target: &MobjectHandle) -> PyResult<[f64; 4]> {
        self.available()?;
        self.scene
            .shape_matcher_bounds((&target.handle).into())
            .map(crate::geometry::bounds_coordinates)
            .map_err(engine_error)
    }

    #[pyo3(name = "familyShapeMatcherBounds")]
    fn family_shape_matcher_bounds(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<[f64; 4]> {
        self.available()?;
        self.scene
            .shape_matcher_bounds((&target.family).into())
            .map(crate::geometry::bounds_coordinates)
            .map_err(engine_error)
    }

    #[pyo3(name = "beginBooleanGeometry")]
    fn begin_boolean_geometry(
        &self,
        name: &str,
        operands: &crate::geometry::BooleanOperands,
    ) -> PyResult<crate::geometry::GeometryOptions> {
        self.available()?;
        self.scene
            .boolean_geometry_options(crate::geometry::boolean_operation(name)?, &operands.0)
            .map(crate::geometry::GeometryOptions::from_options)
            .map_err(engine_error)
    }

    #[pyo3(name = "beginUnderline")]
    fn begin_underline(
        &self,
        target: &crate::mobject::MobjectHandle,
        buff: f64,
    ) -> PyResult<crate::geometry::GeometryOptions> {
        self.available()?;
        self.scene
            .underline_options(&target.handle, buff)
            .map(crate::geometry::GeometryOptions::from_options)
            .map_err(engine_error)
    }

    #[pyo3(name = "prepareTangentLine")]
    fn prepare_tangent_line(
        &self,
        source: &MobjectHandle,
        alpha: f64,
        length: f64,
        d_alpha: f64,
    ) -> PyResult<GeometryOptions> {
        self.available()?;
        self.scene
            .tangent_line_options(&source.handle, alpha, length, d_alpha)
            .map(GeometryOptions::from_options)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    #[pyo3(name = "prepareAxesArea")]
    fn prepare_axes_area(
        &self,
        axes: &crate::family::FamilyHandle,
        graph: &MobjectHandle,
        range: Vec<f64>,
        bounded_graph: &MobjectHandle,
        has_bounded_graph: bool,
    ) -> PyResult<GeometryOptions> {
        self.available()?;
        let axes = noon::ManimAxes::from_family(axes.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let range = noon::integration::plot_optional_range(&range, "area.range", "area")
            .map_err(engine_error)?;
        self.scene
            .axes_area_options(
                &axes,
                &graph.handle,
                range,
                has_bounded_graph.then_some(&bounded_graph.handle),
            )
            .map(GeometryOptions::from_options)
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[cfg(feature = "latex")]
    #[pyo3(name = "changeBarValues")]
    fn change_bar_values(
        &mut self,
        chart: &mut crate::bar_chart::BarChartHandle,
        values: Vec<f64>,
        update_colors: bool,
    ) -> PyResult<()> {
        self.available()?;
        let failure = if self.has_execution() {
            noon::integration::live_coordinate_publication_failure
        } else {
            noon::integration::coordinate_failure
        };
        chart
            .0
            .change_bar_values(&mut self.scene, &values, update_colors)
            .map_err(failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "braceGeometryOptions")]
    fn brace_geometry_options(
        &self,
        target: &LayoutAnchor,
        x: f64,
        y: f64,
        buff: f64,
        sharpness: f64,
    ) -> PyResult<GeometryOptions> {
        self.require_active()?;
        let options = crate::brace::options(x, y, buff, sharpness, 0.0);
        self.scene
            .brace_geometry_options(&target.anchor, options)
            .map(GeometryOptions::from_options)
            .map_err(|error| crate::brace::failure("brace.prepare", &error))
    }

    #[pyo3(name = "createBraceLabel")]
    fn create_brace_label(
        &mut self,
        target: &LayoutAnchor,
        label: &LayoutAnchor,
        x: f64,
        y: f64,
        buff: f64,
        sharpness: f64,
        label_buff: f64,
    ) -> PyResult<crate::brace::BraceLabelHandle> {
        self.available()?;
        let options = crate::brace::options(x, y, buff, sharpness, label_buff);
        noon::BraceLabel::new(
            &mut self.scene,
            &target.anchor,
            label.anchor.clone(),
            options,
        )
        .map(crate::brace::BraceLabelHandle)
        .map_err(|error| crate::brace::failure("brace.create", &error))
    }

    #[pyo3(name = "shiftBraceLabel")]
    fn shift_brace_label(
        &mut self,
        brace: &mut crate::brace::BraceLabelHandle,
        target: &LayoutAnchor,
    ) -> PyResult<()> {
        self.available()?;
        brace
            .0
            .shift_brace(&mut self.scene, &target.anchor)
            .map_err(|error| crate::brace::failure("brace.shift", &error))
    }

    #[pyo3(name = "replaceBraceLabel")]
    fn replace_brace_label(
        &mut self,
        brace: &mut crate::brace::BraceLabelHandle,
        label: &LayoutAnchor,
    ) -> PyResult<()> {
        self.available()?;
        brace
            .0
            .change_label(&mut self.scene, label.anchor.clone())
            .map_err(|error| crate::brace::failure("brace.label", &error))
    }

    #[pyo3(name = "changeBraceLabel")]
    fn change_brace_label(
        &mut self,
        brace: &mut crate::brace::BraceLabelHandle,
        target: &LayoutAnchor,
        label: &LayoutAnchor,
    ) -> PyResult<()> {
        self.available()?;
        let code = if self.has_execution() {
            "brace.change"
        } else {
            "brace.replace"
        };
        brace
            .0
            .change_brace_label(&mut self.scene, &target.anchor, label.anchor.clone())
            .map_err(|error| crate::brace::failure(code, &error))
    }

    /// Build an opaque native Graph/DiGraph through the owning Scene.
    /// Key mapping belongs to the language facade; topology remains in Rust.
    #[pyo3(name = "createGraph")]
    fn create_graph(
        &mut self,
        directed: bool,
        vertex_ids: Vec<u32>,
        positions: Vec<f64>,
        edge_pairs: Vec<u32>,
        options: &crate::graph::GraphOptions,
    ) -> Result<crate::graph::GraphHandle, PyErr> {
        self.available()?;
        let vertices =
            noon::integration::graph::vertices(&vertex_ids, &positions).map_err(engine_error)?;
        let edges =
            noon::integration::graph::pairs(&edge_pairs, "Graph edges").map_err(engine_error)?;
        crate::graph::create_graph(self, directed, vertices, edges, options.options.clone())
            .map(|inner| crate::graph::GraphHandle { inner })
            .map_err(engine_error)
    }

    /// Compute a circular layout before publishing any semantic nodes.
    #[pyo3(name = "createCircularGraph")]
    fn create_circular_graph(
        &mut self,
        directed: bool,
        vertex_ids: Vec<u32>,
        edge_pairs: Vec<u32>,
        scale: f64,
        center_x: f64,
        center_y: f64,
        options: &crate::graph::GraphOptions,
    ) -> Result<crate::graph::GraphHandle, PyErr> {
        self.available()?;
        let edges =
            noon::integration::graph::pairs(&edge_pairs, "Graph edges").map_err(engine_error)?;
        let positions = noon::GraphLayoutOptions {
            layout: noon::GraphLayout::Circular,
            scale,
            center: (center_x, center_y),
            ..Default::default()
        }
        .positions(vertex_ids.len(), &[])
        .map_err(|error| {
            engine_error(noon::integration::AuthoringFailure::unclassified(
                "graph.layout",
                &error,
            ))
        })?;
        let vertices = vertex_ids.iter().copied().zip(positions).collect();
        crate::graph::create_graph(self, directed, vertices, edges, options.options.clone())
            .map(|inner| crate::graph::GraphHandle { inner })
            .map_err(engine_error)
    }

    #[pyo3(name = "graphCopy")]
    fn graph_copy(
        &mut self,
        graph: &crate::graph::GraphHandle,
    ) -> Result<crate::graph::GraphHandle, PyErr> {
        self.available()?;
        crate::graph::copy_graph(self, graph)
            .map(|inner| crate::graph::GraphHandle { inner })
            .map_err(engine_error)
    }

    #[pyo3(name = "graphAddVertices")]
    fn graph_add_vertices(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        vertex_ids: Vec<u32>,
        positions: Vec<f64>,
    ) -> Result<(), PyErr> {
        self.available()?;
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::AddVertices(
                noon::integration::graph::vertices(&vertex_ids, &positions)
                    .map_err(engine_error)?,
            ),
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "graphAddEdges")]
    fn graph_add_edges(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        edge_pairs: Vec<u32>,
    ) -> Result<(), PyErr> {
        self.available()?;
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::AddEdges(
                noon::integration::graph::pairs(&edge_pairs, "Graph edges")
                    .map_err(engine_error)?,
            ),
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "graphCircularLayout")]
    fn graph_circular_layout(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        scale: f64,
        center_x: f64,
        center_y: f64,
    ) -> Result<(), PyErr> {
        self.available()?;
        if !scale.is_finite() || !center_x.is_finite() || !center_y.is_finite() {
            return Err(engine_error("circular Graph layout values must be finite"));
        }
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::Circular {
                scale,
                center: (center_x, center_y),
            },
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "graphExplicitLayout")]
    fn graph_explicit_layout(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        positions: Vec<f64>,
    ) -> Result<(), PyErr> {
        self.available()?;
        let ids = (0..positions.len() / 2)
            .map(|id| id as u32)
            .collect::<Vec<_>>();
        let positions = noon::integration::graph::vertices(&ids, &positions)
            .map_err(engine_error)?
            .into_iter()
            .map(|(_, point)| point)
            .collect();
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::Explicit(positions),
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "graphRemoveVertices")]
    fn graph_remove_vertices(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        vertex_ids: Vec<u32>,
    ) -> Result<(), PyErr> {
        self.available()?;
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::RemoveVertices(vertex_ids),
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "graphRemoveEdges")]
    fn graph_remove_edges(
        &mut self,
        graph: &mut crate::graph::GraphHandle,
        edge_pairs: Vec<u32>,
    ) -> Result<(), PyErr> {
        self.available()?;
        crate::graph::mutate_graph(
            self,
            graph,
            noon::integration::graph::GraphOperation::RemoveEdges(
                noon::integration::graph::pairs(&edge_pairs, "Graph edges")
                    .map_err(engine_error)?,
            ),
        )
        .map_err(engine_error)
    }

    #[pyo3(name = "createSampleSpace")]
    fn create_sample_space(
        &mut self,
        options: &crate::sample_space::SampleSpaceOptions,
    ) -> PyResult<crate::sample_space::SampleSpaceHandle> {
        self.available()?;
        noon::SampleSpace::new_with_options(&mut self.scene, &options.0)
            .map(crate::sample_space::SampleSpaceHandle)
            .map_err(crate::sample_space::failure)
    }

    #[pyo3(name = "getHorizontalDivision")]
    fn get_horizontal_division(
        &mut self,
        space: &crate::sample_space::SampleSpaceHandle,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        crate::sample_space::family(space.0.get_horizontal_division_in_scene(
            &mut self.scene,
            probabilities,
            &crate::sample_space::colors(rgba)?,
        ))
    }

    #[pyo3(name = "getVerticalDivision")]
    fn get_vertical_division(
        &mut self,
        space: &crate::sample_space::SampleSpaceHandle,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        crate::sample_space::family(space.0.get_vertical_division_in_scene(
            &mut self.scene,
            probabilities,
            &crate::sample_space::colors(rgba)?,
        ))
    }

    #[pyo3(name = "divideHorizontally")]
    fn divide_horizontally(
        &mut self,
        space: &mut crate::sample_space::SampleSpaceHandle,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        crate::sample_space::family(space.0.divide_horizontally(
            &mut self.scene,
            probabilities,
            &crate::sample_space::colors(rgba)?,
        ))
    }

    #[pyo3(name = "divideVertically")]
    fn divide_vertically(
        &mut self,
        space: &mut crate::sample_space::SampleSpaceHandle,
        probabilities: Vec<f64>,
        rgba: Vec<f64>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        crate::sample_space::family(space.0.divide_vertically(
            &mut self.scene,
            probabilities,
            &crate::sample_space::colors(rgba)?,
        ))
    }

    #[pyo3(name = "prepareRiemannPlan")]
    fn prepare_riemann_plan(
        &self,
        axes: &crate::family::FamilyHandle,
        graph: &MobjectHandle,
        mut options: PyRefMut<'_, crate::riemann::RiemannOptions>,
        bounded: &MobjectHandle,
        has_bounded: bool,
    ) -> PyResult<crate::riemann::RiemannPlan> {
        self.available()?;
        let axes = crate::riemann::axes(axes)?;
        let options = options.take(graph, has_bounded.then_some(bounded))?;
        self.scene
            .riemann_plan(&axes, &graph.handle, options)
            .map(crate::riemann::RiemannPlan)
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "publishRiemannPlan")]
    fn publish_riemann_plan(
        &mut self,
        plan: &crate::riemann::RiemannPlan,
        top: Vec<f64>,
        baseline: Vec<f64>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        plan.publish_on_scene(&mut self.scene, &top, &baseline)
    }

    #[pyo3(name = "createSvgFromString", signature = (source, should_center, height, width_or_defaults))]
    fn create_svg_from_string(
        &mut self,
        source: &str,
        should_center: bool,
        height: Option<f64>,
        width_or_defaults: &Bound<'_, PyAny>,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        crate::svg::on_scene(
            &mut self.scene,
            source,
            should_center,
            height,
            width_or_defaults,
        )
    }

    #[pyo3(name = "createCoordinates")]
    fn coordinates(
        &mut self,
        options: &crate::coordinates::CoordinateOptions,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        let running = self.has_execution();
        crate::coordinates::create_on_scene(&mut self.scene, options, running)
    }

    #[pyo3(name = "associatePublishedMobjects")]
    fn associate_published(&mut self, batch: &MembershipBatch) -> PyResult<()> {
        self.available()?;
        if batch.kind != "add" || !batch.members.is_empty() {
            return Err(engine_error(
                "published association accepts add binding reservations only",
            ));
        }
        if scene_execution::session(&self.scene).is_some_and(|execution| {
            execution.publication_context().scene_revision() != self.scene.revision()
        }) {
            return Err(engine_error(
                "published association requires a coherent execution revision",
            ));
        }
        for handle in &batch.bindings {
            let target = MobjectHandle::from_semantic_mobject(handle.clone());
            if !self.contains(&target)? {
                return Err(engine_error(
                    "published association target is not in this Scene",
                ));
            }
        }
        self.bound_ids
            .extend(batch.bindings.iter().map(|handle| handle.node_id()));
        Ok(())
    }

    #[pyo3(name = "setFamilyOwnStyle", signature = (source, fill_enabled, fill_red, fill_green, fill_blue, fill_alpha, fill_opacity, stroke_enabled, stroke_red, stroke_green, stroke_blue, stroke_alpha, stroke_width, stroke_opacity))]
    #[allow(clippy::too_many_arguments)]
    fn set_family_own_style(
        &mut self,
        source: &crate::family::FamilyHandle,
        fill_enabled: bool,
        fill_red: f64,
        fill_green: f64,
        fill_blue: f64,
        fill_alpha: f64,
        fill_opacity: Option<f64>,
        stroke_enabled: bool,
        stroke_red: f64,
        stroke_green: f64,
        stroke_blue: f64,
        stroke_alpha: f64,
        stroke_width: Option<f64>,
        stroke_opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let update = noon::integration::style_update(
            fill_enabled,
            fill_red,
            fill_green,
            fill_blue,
            fill_alpha,
            fill_opacity,
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
            stroke_width,
            stroke_opacity,
        )
        .map_err(engine_error)?;
        self.same_family_store(source)?;
        self.edit_scene(|scene| scene.set_family_own_style(&source.family, update))
    }

    #[pyo3(name = "setFamilyOwnStyleWithBackgroundStroke", signature = (source, fill_enabled, fill_red, fill_green, fill_blue, fill_alpha, fill_opacity, stroke_enabled, stroke_red, stroke_green, stroke_blue, stroke_alpha, stroke_width, stroke_opacity, background_stroke_color_enabled, background_stroke_red, background_stroke_green, background_stroke_blue, background_stroke_alpha, background_stroke_width, background_stroke_opacity))]
    #[allow(clippy::too_many_arguments)]
    fn set_family_own_style_with_background_stroke(
        &mut self,
        source: &crate::family::FamilyHandle,
        fill_enabled: bool,
        fill_red: f64,
        fill_green: f64,
        fill_blue: f64,
        fill_alpha: f64,
        fill_opacity: Option<f64>,
        stroke_enabled: bool,
        stroke_red: f64,
        stroke_green: f64,
        stroke_blue: f64,
        stroke_alpha: f64,
        stroke_width: Option<f64>,
        stroke_opacity: Option<f64>,
        background_stroke_color_enabled: bool,
        background_stroke_red: f64,
        background_stroke_green: f64,
        background_stroke_blue: f64,
        background_stroke_alpha: f64,
        background_stroke_width: Option<f64>,
        background_stroke_opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let update = noon::integration::style_update(
            fill_enabled,
            fill_red,
            fill_green,
            fill_blue,
            fill_alpha,
            fill_opacity,
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
            stroke_width,
            stroke_opacity,
        )
        .map_err(engine_error)?;
        let background_stroke_color = noon::integration::family_color(
            background_stroke_color_enabled,
            background_stroke_red,
            background_stroke_green,
            background_stroke_blue,
            background_stroke_alpha,
        )
        .map_err(engine_error)?;
        self.same_family_store(source)?;
        self.edit_scene(|scene| {
            scene.set_family_own_style_with_background_stroke(
                &source.family,
                update,
                background_stroke_color,
                background_stroke_width,
                background_stroke_opacity,
            )
        })
    }

    #[pyo3(name = "setFamilyOwnOpacity")]
    fn set_family_own_opacity(
        &mut self,
        source: &crate::family::FamilyHandle,
        opacity: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(source)?;
        self.edit_scene(|scene| scene.set_family_own_opacity(&source.family, opacity))
    }

    #[pyo3(name = "fadeFamilyOwn")]
    fn fade_family_own(
        &mut self,
        source: &crate::family::FamilyHandle,
        darkness: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(source)?;
        self.edit_scene(|scene| scene.fade_family_own(&source.family, darkness))
    }

    #[pyo3(name = "setMobjectSheenDirection")]
    fn set_mobject_sheen_direction(
        &mut self,
        source: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        dimensions: u8,
        family: bool,
    ) -> PyResult<()> {
        let direction =
            noon::SemanticSheenDirection::new(noon::SemanticVec3::new(x, y, z), dimensions);
        let target = MobjectTarget::from(&source.handle);
        self.edit_scene(|scene| scene.set_sheen_direction(&target, direction, family))
    }

    #[pyo3(name = "rotateMobjectSheenDirection")]
    fn rotate_mobject_sheen_direction(
        &mut self,
        source: &MobjectHandle,
        angle: f64,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        family: bool,
    ) -> PyResult<()> {
        let axis = noon::SemanticVec3::new(axis_x, axis_y, axis_z);
        let target = MobjectTarget::from(&source.handle);
        self.edit_scene(|scene| scene.rotate_sheen_direction(&target, angle, axis, family))
    }

    #[pyo3(name = "setFamilySheenDirection")]
    fn set_family_sheen_direction(
        &mut self,
        source: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        z: f64,
        dimensions: u8,
        family: bool,
    ) -> PyResult<()> {
        let direction =
            noon::SemanticSheenDirection::new(noon::SemanticVec3::new(x, y, z), dimensions);
        let target = MobjectTarget::from(&source.family);
        self.edit_scene(|scene| scene.set_sheen_direction(&target, direction, family))
    }

    #[pyo3(name = "rotateFamilySheenDirection")]
    fn rotate_family_sheen_direction(
        &mut self,
        source: &crate::family::FamilyHandle,
        angle: f64,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        family: bool,
    ) -> PyResult<()> {
        let axis = noon::SemanticVec3::new(axis_x, axis_y, axis_z);
        let target = MobjectTarget::from(&source.family);
        self.edit_scene(|scene| scene.rotate_sheen_direction(&target, angle, axis, family))
    }

    #[pyo3(name = "setFamilyOwnStrokeCap")]
    fn set_family_own_stroke_cap(
        &mut self,
        source: &crate::family::FamilyHandle,
        cap: &str,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(source)?;
        self.edit_scene(|scene| scene.set_family_own_stroke_cap(&source.family, cap))
    }

    #[pyo3(name = "setStrokeCap")]
    fn set_stroke_cap(&mut self, handle: &MobjectHandle, cap: &str) -> PyResult<()> {
        self.available()?;
        self.same_store(handle)?;
        self.edit_scene(|scene| scene.set_stroke_cap(&handle.handle, cap))
    }

    #[pyo3(name = "queryMobjectStrokeCap")]
    fn query_stroke_cap(&self, target: &MobjectHandle) -> PyResult<&'static str> {
        self.require_active()?;
        self.same_store(target)?;
        self.scene
            .stroke_cap(&target.handle)
            .map(|cap| cap.as_str())
            .map_err(engine_error)
    }

    fn fade(&mut self, target: &MobjectHandle, darkness: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.fade(&target.handle, darkness))
    }

    #[pyo3(name = "fadeFamily")]
    fn fade_family(&mut self, target: &crate::family::FamilyHandle, darkness: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.fade_family(&target.family, darkness))
    }

    #[pyo3(name = "interpolateMobjectStyle")]
    fn interpolate_manim_style(
        &mut self,
        receiver: &MobjectHandle,
        start: &MobjectHandle,
        end: &MobjectHandle,
        alpha: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| {
            scene.interpolate_manim_style(&receiver.handle, &start.handle, &end.handle, alpha)
        })
    }

    #[pyo3(name = "interpolateMobject")]
    fn interpolate_manim(
        &mut self,
        receiver: &MobjectHandle,
        start: &MobjectHandle,
        end: &MobjectHandle,
        alpha: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| {
            scene.interpolate_manim(&receiver.handle, &start.handle, &end.handle, alpha)
        })
    }

    #[pyo3(name = "interpolateFamilyManimStyle")]
    fn interpolate_family_manim_style(
        &mut self,
        receiver: &crate::family::FamilyHandle,
        start: &crate::family::FamilyHandle,
        end: &crate::family::FamilyHandle,
        alpha: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| {
            scene.interpolate_family_manim_style(
                &receiver.family,
                &start.family,
                &end.family,
                alpha,
            )
        })
    }

    #[pyo3(name = "fadeTo")]
    fn fade_to(
        &mut self,
        target: &MobjectHandle,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        amount: f64,
    ) -> PyResult<()> {
        self.available()?;
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        self.edit_scene(|scene| scene.fade_to(&target.handle, color, amount))
    }

    #[pyo3(name = "fadeToFamily")]
    fn fade_to_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        amount: f64,
    ) -> PyResult<()> {
        self.available()?;
        let color = noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32);
        self.edit_scene(|scene| scene.fade_to_family(&target.family, color, amount))
    }

    #[pyo3(name = "matchStyle")]
    fn match_style(&mut self, receiver: &MobjectHandle, donor: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.scene
            .match_style(&receiver.handle, &donor.handle)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    #[pyo3(name = "matchFamilyStyle")]
    fn match_family_style(
        &mut self,
        receiver: &crate::family::FamilyHandle,
        donor: &crate::family::FamilyHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.scene
            .match_family_style(&receiver.family, &donor.family)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    #[pyo3(name = "applyMatrixFamily")]
    fn apply_matrix_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
        rows: usize,
        columns: usize,
        x: f64,
        y: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.apply_matrix_to_family(&target.family, &values, rows, columns, x, y)
        })
    }

    #[pyo3(name = "createManimArrow")]
    fn create_arrow(
        &mut self,
        py: Python<'_>,
        request: crate::arrow::ArrowRequest<'_>,
    ) -> PyResult<Py<PyAny>> {
        self.available()?;
        let running = self.has_execution();
        request.publish_in_scene(py, &mut self.scene, running)
    }

    #[pyo3(name = "scaleArrow")]
    fn scale_arrow(
        &mut self,
        arrow: &crate::arrow::ArrowHandle,
        factor: f64,
        scale_tips: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.scene
            .scale_arrow(&arrow.0, factor, scale_tips)
            .map_err(crate::arrow::scale_error)
    }

    #[pyo3(name = "bindClickIndicate")]
    #[allow(clippy::too_many_arguments)]
    fn bind_click_indicate(
        &mut self,
        target: &MobjectHandle,
        scale_factor: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        duration: f64,
        easing: &str,
        lag_ratio: f64,
    ) -> PyResult<()> {
        self.available()?;
        let color = callback_color(
            "indication color",
            Some(red),
            Some(green),
            Some(blue),
            Some(alpha),
        )?
        .expect("all indication color channels were supplied");
        let options = noon::AnimationOptions::new()
            .run_time(duration)
            .rate_func(crate::composition::rate(easing)?)
            .lag_ratio(lag_ratio);
        let running = self.has_execution();
        self.scene
            .on_click_indicate(
                &target.handle,
                noon::IndicateOptions::new(scale_factor, color),
                options,
            )
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    #[pyo3(name = "bindClickGlowPulse")]
    fn bind_click_glow_pulse(
        &mut self,
        target: &MobjectHandle,
        intensity: f64,
        duration: f64,
        easing: &str,
        lag_ratio: f64,
        reverse_rate_function: bool,
    ) -> PyResult<()> {
        self.available()?;
        let options = noon::AnimationOptions::new()
            .run_time(duration)
            .rate_func(crate::composition::rate(easing)?)
            .lag_ratio(lag_ratio)
            .reverse_rate_function(reverse_rate_function);
        let running = self.has_execution();
        self.scene
            .on_click_glow_pulse(&target.handle, intensity, options)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    #[pyo3(name = "clearClickAction")]
    fn clear_click_action(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        let running = self.has_execution();
        self.scene
            .on_click(&target.handle, None)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }

    #[pyo3(name = "stretchLayout")]
    fn stretch_layout(
        &mut self,
        target: &LayoutAnchor,
        factor: f64,
        dimension: u32,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        let dimension = dimension.try_into().map_err(engine_error)?;
        self.place(|scene| {
            scene.stretch(
                &target.anchor,
                factor,
                dimension,
                crate::mobject::pivot_3d(x, y, z, point),
            )
        })
    }
    #[pyo3(name = "prependFamilyMembers")]
    fn prepend_family_members(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let members = batch.add_targets()?;
        self.edit_scene(|scene| scene.prepend_family_members(&target.family, &members))
    }
    #[pyo3(name = "prependObjectMembers")]
    fn prepend_handle_members(
        &mut self,
        target: &MobjectHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_store(target)?;
        let members = batch.add_targets()?;
        self.edit_scene(|scene| scene.prepend_family_members(&target.handle, &members))
    }

    #[pyo3(name = "queryMobjectPathBoundaryPoints3D")]
    fn mobject_path_boundary_points_3d(&self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.observe_points((&target.handle).into(), Scene::path_boundary_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryMobjectVectorBoundaryPoints3D")]
    fn mobject_vector_boundary_points_3d(&self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.observe_points((&target.handle).into(), Scene::vector_boundary_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryMobjectAllPoints3D")]
    fn mobject_all_points_3d(&self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.observe_points((&target.handle).into(), Scene::all_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryMobjectExtremumAlongDim")]
    fn mobject_extremum_along_dim(
        &self,
        target: &MobjectHandle,
        dim: usize,
        key: f64,
    ) -> PyResult<f64> {
        self.observe_points((&target.handle).into(), |scene, target| {
            scene.extremum_along_dim(target, dim, key)
        })
    }

    #[pyo3(name = "queryMobjectLengthOverDim")]
    fn mobject_length_over_dim(&self, target: &MobjectHandle, dim: usize) -> PyResult<f64> {
        self.observe_points((&target.handle).into(), |scene, target| {
            scene.length_over_dim(target, dim)
        })
    }

    #[pyo3(name = "queryMobjectCenterOfMass")]
    fn mobject_center_of_mass(&self, target: &MobjectHandle) -> PyResult<(f64, f64)> {
        self.observe_points((&target.handle).into(), Scene::center_of_mass)
    }

    #[pyo3(name = "queryMobjectBoundaryPoint")]
    fn mobject_boundary_point(
        &self,
        target: &MobjectHandle,
        direction_x: f64,
        direction_y: f64,
    ) -> PyResult<(f64, f64)> {
        self.observe_points((&target.handle).into(), |scene, target| {
            scene.boundary_point(target, direction_x, direction_y)
        })
    }

    #[pyo3(name = "queryMobjectPathBoundaryExtremumAlongDim")]
    fn mobject_path_boundary_extremum_along_dim(
        &self,
        target: &MobjectHandle,
        dim: usize,
        key: f64,
    ) -> PyResult<f64> {
        self.observe_points((&target.handle).into(), |scene, target| {
            scene.path_boundary_extremum_along_dim(target, dim, key)
        })
    }

    #[pyo3(name = "queryMobjectPathBoundaryPoint")]
    fn mobject_path_boundary_point(
        &self,
        target: &MobjectHandle,
        direction_x: f64,
        direction_y: f64,
    ) -> PyResult<(f64, f64)> {
        self.observe_points((&target.handle).into(), |scene, target| {
            scene.path_boundary_point(target, direction_x, direction_y)
        })
    }

    #[pyo3(name = "queryFamilyPathBoundaryPoints3D")]
    fn family_path_boundary_points_3d(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<Vec<f64>> {
        self.observe_points((&target.family).into(), Scene::path_boundary_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryFamilyVectorBoundaryPoints3D")]
    fn family_vector_boundary_points_3d(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<Vec<f64>> {
        self.observe_points((&target.family).into(), Scene::vector_boundary_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryFamilyAllPoints3D")]
    fn family_all_points_3d(&self, target: &crate::family::FamilyHandle) -> PyResult<Vec<f64>> {
        self.observe_points((&target.family).into(), Scene::all_points_3d)
            .map(crate::path_input::flat_points_3d)
    }

    #[pyo3(name = "queryFamilyExtremumAlongDim")]
    fn family_extremum_along_dim(
        &self,
        target: &crate::family::FamilyHandle,
        dim: usize,
        key: f64,
    ) -> PyResult<f64> {
        self.observe_points((&target.family).into(), |scene, target| {
            scene.extremum_along_dim(target, dim, key)
        })
    }

    #[pyo3(name = "queryFamilyLengthOverDim")]
    fn family_length_over_dim(
        &self,
        target: &crate::family::FamilyHandle,
        dim: usize,
    ) -> PyResult<f64> {
        self.observe_points((&target.family).into(), |scene, target| {
            scene.length_over_dim(target, dim)
        })
    }

    #[pyo3(name = "queryFamilyCenterOfMass")]
    fn family_center_of_mass(&self, target: &crate::family::FamilyHandle) -> PyResult<(f64, f64)> {
        self.observe_points((&target.family).into(), Scene::center_of_mass)
    }

    #[pyo3(name = "queryFamilyBoundaryPoint")]
    fn family_boundary_point(
        &self,
        target: &crate::family::FamilyHandle,
        direction_x: f64,
        direction_y: f64,
    ) -> PyResult<(f64, f64)> {
        self.observe_points((&target.family).into(), |scene, target| {
            scene.boundary_point(target, direction_x, direction_y)
        })
    }

    #[pyo3(name = "prepareFamilySubsetDisplay")]
    fn prepare_family_subset_display(
        &mut self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.scene
            .prepare_family_subset_display(&target.family)
            .map(|_| ())
            .map_err(engine_error)
    }

    #[pyo3(name = "queryMobjectPointReductionTree")]
    fn mobject_point_reduction_tree<'py>(
        &self,
        py: Python<'py>,
        target: &MobjectHandle,
    ) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
        let tree = self.observe_points((&target.handle).into(), Scene::point_reduction_tree)?;
        crate::path_input::point_reduction_tree(py, tree)
    }

    #[pyo3(name = "queryFamilyPointReductionTree")]
    fn family_point_reduction_tree<'py>(
        &self,
        py: Python<'py>,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<Bound<'py, pyo3::types::PyTuple>> {
        let tree = self.observe_points((&target.family).into(), Scene::point_reduction_tree)?;
        crate::path_input::point_reduction_tree(py, tree)
    }

    #[pyo3(name = "queryMobjectPointCount")]
    fn point_count(&self, target: &MobjectHandle) -> PyResult<usize> {
        self.require_active()?;
        self.same_store(target)?;
        self.scene
            .own_point_count((&target.handle).into())
            .map_err(|error| {
                if self.has_execution() {
                    engine_error(noon::LiveSessionError::from(error))
                } else {
                    engine_error(error)
                }
            })
    }

    #[pyo3(name = "queryFamilyPointCount")]
    fn family_point_count(&self, target: &crate::family::FamilyHandle) -> PyResult<usize> {
        self.require_active()?;
        self.scene
            .own_point_count((&target.family).into())
            .map_err(engine_error)
    }

    #[pyo3(name = "queryMobjectPointArray")]
    fn object_point_array(&self, target: &MobjectHandle) -> PyResult<(Vec<f64>, Vec<usize>)> {
        self.observe_points((&target.handle).into(), Scene::point_array)
            .map(crate::path_input::point_array)
    }
    #[pyo3(name = "queryFamilyPointArray")]
    fn family_point_array(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<(Vec<f64>, Vec<usize>)> {
        self.observe_points((&target.family).into(), Scene::point_array)
            .map(crate::path_input::point_array)
    }

    #[pyo3(name="setFillImage",signature=(target,paint,opacity))]
    fn set_fill_image(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::ImagePaintOptions,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.edit_scene(|scene| {
            scene.set_fill_image(&target.handle, paint.options.clone(), opacity)
        })
    }

    #[pyo3(name="setFillImageMapping",signature=(target,paint,opacity))]
    fn set_fill_image_mapping(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::ImageMapping,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.edit_scene(|scene| scene.set_fill_image_mapping(&target.handle, paint.value, opacity))
    }

    #[pyo3(name="setFillGradient", signature=(target,paint,opacity))]
    fn set_fill_gradient(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::LinearGradient,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_gradient(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name="setFillRadialGradient", signature=(target,paint,opacity))]
    fn set_fill_radial_gradient(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::RadialGradient,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| {
            scene.set_fill_radial_gradient(&target.handle, paint.value.clone(), opacity)
        })
    }

    #[pyo3(name="setFillNoise", signature=(target,paint,opacity))]
    fn set_fill_noise(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::NoisePaint,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_noise(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name="setFillHatch", signature=(target,paint,opacity))]
    fn set_fill_hatch(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::HatchPaint,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_hatch(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name = "setGlow")]
    fn _set_glow(
        &mut self,
        target: &MobjectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.set_glow(&target.handle, update.value.clone()))
    }

    #[pyo3(name = "removeGlow")]
    fn _remove_glow(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.remove_glow(&target.handle))
    }

    #[pyo3(name = "addEffect")]
    fn _add_effect(
        &mut self,
        target: &MobjectHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.add_effect(&target.handle, definition.definition(), name))
    }

    #[pyo3(name = "setEffect")]
    fn _set_effect(
        &mut self,
        target: &MobjectHandle,
        name: &str,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.set_effect(&target.handle, name, update.value.clone()))
    }

    #[pyo3(name = "setEffectHandle")]
    fn _set_effect_handle(
        &mut self,
        target: &MobjectHandle,
        handle: &crate::effects::EffectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| {
            scene.set_effect(&target.handle, &handle.handle, update.value.clone())
        })
    }

    #[pyo3(name = "removeEffect")]
    fn _remove_effect(&mut self, target: &MobjectHandle, name: &str) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.remove_effect(&target.handle, name))
    }

    #[pyo3(name = "removeEffectHandle")]
    fn _remove_effect_handle(
        &mut self,
        target: &MobjectHandle,
        handle: &crate::effects::EffectHandle,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.remove_effect(&target.handle, &handle.handle))
    }

    #[pyo3(name = "setFamilyGlow")]
    fn family_set_glow(
        &mut self,
        target: &crate::family::FamilyHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.set_glow(&target.family, update.value.clone()))
    }

    #[pyo3(name = "removeFamilyGlow")]
    fn family_remove_glow(&mut self, target: &crate::family::FamilyHandle) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.remove_glow(&target.family))
    }

    #[pyo3(name = "addFamilyEffect")]
    fn family_add_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.add_effect(&target.family, definition.definition(), name))
    }

    #[pyo3(name = "setFamilyEffect")]
    fn family_set_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        name: &str,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.set_effect(&target.family, name, update.value.clone()))
    }

    #[pyo3(name = "setFamilyEffectHandle")]
    fn family_set_effect_handle(
        &mut self,
        target: &crate::family::FamilyHandle,
        handle: &crate::effects::EffectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| {
            scene.set_effect(&target.family, &handle.handle, update.value.clone())
        })
    }

    #[pyo3(name = "removeFamilyEffect")]
    fn family_remove_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        name: &str,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.remove_effect(&target.family, name))
    }

    #[pyo3(name = "removeFamilyEffectHandle")]
    fn family_remove_effect_handle(
        &mut self,
        target: &crate::family::FamilyHandle,
        handle: &crate::effects::EffectHandle,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.remove_effect(&target.family, &handle.handle))
    }

    #[pyo3(name = "queryLayoutDimensionLength")]
    fn layout_dimension_length(&mut self, source: &LayoutAnchor, dimension: u32) -> PyResult<f64> {
        self.require_active()?;
        let dimension = dimension.try_into().map_err(engine_error)?;
        self.scene
            .layout_dimension_length(&source.anchor, dimension)
            .map_err(|error| {
                if !self.has_execution() {
                    engine_error(error)
                } else if dimension == noon::LayoutDimension::Depth {
                    engine_error(AuthoringFailure::from_scene_mutation(error, true))
                } else {
                    engine_error(noon::LiveSessionError::from(error))
                }
            })
    }
    #[pyo3(name = "queryMobjectDimension")]
    fn mobject_dimension(&mut self, target: &MobjectHandle, dimension: u32) -> PyResult<f64> {
        self.same_store(target)?;
        self.layout_dimension_length(
            &LayoutAnchor::from_anchor(noon::LayoutAnchor::from(&target.handle), None),
            dimension,
        )
    }

    #[pyo3(name = "scaleLayout3d")]
    #[allow(clippy::too_many_arguments)]
    fn scale_layout_3d(
        &mut self,
        source: &LayoutAnchor,
        sx: f64,
        sy: f64,
        sz: f64,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.scale_layout_3d(
                &source.anchor,
                noon::SemanticVec3::new(sx, sy, sz),
                crate::mobject::pivot_3d(x, y, z, point),
            )
        })
    }

    #[pyo3(name = "queryMobjectRequiresContentBoundary")]
    fn mobject_requires_content_boundary(&self, target: &MobjectHandle) -> PyResult<bool> {
        self.observe_points((&target.handle).into(), Scene::requires_content_boundary)
    }

    #[pyo3(name = "queryMobjectCriticalPoint3D")]
    fn mobject_critical_point_3d(
        &self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        vector_boundary: bool,
    ) -> PyResult<(f64, f64, f64)> {
        self.critical_point_3d(
            (&target.handle).into(),
            noon::SemanticVec3::new(x, y, z),
            vector_boundary,
        )
    }

    #[pyo3(name = "queryFamilyRequiresContentBoundary")]
    fn family_requires_content_boundary(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<bool> {
        self.observe_points((&target.family).into(), Scene::requires_content_boundary)
    }

    #[pyo3(name = "queryFamilyCriticalPoint3D")]
    fn family_critical_point_3d(
        &self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        z: f64,
        vector_boundary: bool,
    ) -> PyResult<(f64, f64, f64)> {
        self.critical_point_3d(
            (&target.family).into(),
            noon::SemanticVec3::new(x, y, z),
            vector_boundary,
        )
    }

    #[pyo3(name = "queryLayoutContentCriticalPoint3D")]
    fn layout_content_critical_point_3d(
        &mut self,
        anchor: &LayoutAnchor,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<(f64, f64, f64)> {
        self.require_active()?;
        let direction = noon::SemanticVec3::new(x, y, z);
        self.scene
            .layout_critical_point_3d(&anchor.anchor, direction)
            .map(|p| (p.x, p.y, p.z))
            .map_err(|error| {
                if self.has_execution() {
                    engine_error(noon::LiveSessionError::from(error))
                } else {
                    engine_error(error)
                }
            })
    }

    fn prepare_view(&mut self) -> PyResult<crate::viewer::PreparedScene> {
        self.prepare_viewer().map(crate::viewer::PreparedScene::new)
    }

    #[pyo3(name = "setTranslationDragTargets")]
    fn set_translation_drag_targets(&mut self, batch: &MembershipBatch) -> PyResult<()> {
        self.available()?;
        let mut targets = Vec::new();
        let mut seen = HashSet::new();
        for target in batch.add_targets()? {
            let MobjectTarget::Object(object) = target else {
                return Err(engine_error("translation drag requires leaf mobjects"));
            };
            if !Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
                return Err(engine_error(noon::AuthoringError::ForeignStore));
            }
            object.validate().map_err(engine_error)?;
            let node = object.node_id();
            if !self.bound_ids.contains(&node) {
                return Err(engine_error(
                    "translation drag target is not bound to this canonical Scene",
                ));
            }
            if !seen.insert(node) {
                return Err(engine_error("translation drag targets must be unique"));
            }
            targets.push(object);
        }
        if let Some(mut live) = scene_execution::live(&mut self.scene) {
            live.set_translation_drag_targets(targets)
                .map_err(engine_error)?;
        } else {
            self.drag_targets = targets.into_iter().cloned().collect();
        }
        Ok(())
    }
    #[pyo3(name = "requiresLiveMutationRouting")]
    fn requires_live_mutation_routing(&self) -> PyResult<bool> {
        self.require_active()?;
        Ok(self.has_execution())
    }
    #[cfg(feature = "native-text")]
    #[pyo3(name = "createManimText", signature = (source, family, size, spacing, r, g, b, a, opacity, colors=None, face=None))]
    #[allow(clippy::too_many_arguments)]
    fn create_text(
        &mut self,
        source: &str,
        family: &str,
        size: f64,
        spacing: f64,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
        opacity: f64,
        colors: Option<PyRefMut<'_, crate::text::TextColorBatch>>,
        face: Option<PyRef<'_, crate::text::FontFace>>,
    ) -> PyResult<MobjectHandle> {
        crate::text::in_scene(
            self, source, family, size, spacing, r, g, b, a, opacity, colors, face, false,
        )
    }
    #[cfg(feature = "native-text")]
    #[pyo3(name = "createManimMarkupText", signature = (source, family, size, spacing, r, g, b, a, opacity, face=None))]
    #[allow(clippy::too_many_arguments)]
    fn create_markup_text(
        &mut self,
        source: &str,
        family: &str,
        size: f64,
        spacing: f64,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
        opacity: f64,
        face: Option<PyRef<'_, crate::text::FontFace>>,
    ) -> PyResult<MobjectHandle> {
        crate::text::in_scene(
            self, source, family, size, spacing, r, g, b, a, opacity, None, face, true,
        )
    }
    #[pyo3(name = "setAngleValue")]
    fn set_angle_value(&mut self, target: &MobjectHandle, radians: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene(|scene| scene.set_angle_value(&target.handle, radians))
    }
    #[cfg(feature = "typst")]
    #[pyo3(name = "createManimTypst")]
    #[allow(clippy::too_many_arguments)]
    fn typst(
        &mut self,
        source: &str,
        math: bool,
        size: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: f64,
    ) -> PyResult<MobjectHandle> {
        crate::typst::in_scene(self, source, math, size, red, green, blue, alpha, opacity)
    }
    #[pyo3(name = "liveExecutionOwnership")]
    fn ownership(&self) -> PyResult<&'static str> {
        self.require_active()?;
        Ok(if self.has_execution() {
            "active"
        } else {
            "none"
        })
    }
    #[pyo3(name = "requireActive")]
    fn check_active(&self) -> PyResult<()> {
        self.require_active()
    }
    #[pyo3(name = "nextSection")]
    fn next_section(&mut self, name: &str, skip: bool) -> PyResult<()> {
        self.available()?;
        let time = self.handoff().unwrap_or_else(|| self.scene.time());
        self.scene
            .next_section_at(
                name,
                if skip {
                    noon::SectionType::Skip
                } else {
                    noon::SectionType::Normal
                },
                time,
            )
            .map(|_| ())
            .map_err(engine_error)
    }
    // The shared facade decodes browser transport only; native rows remain typed.
    #[pyo3(name = "sectionsJson")]
    fn sections(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.require_active()?;
        self.scene
            .sections()
            .iter()
            .map(|section| {
                let row = pyo3::types::PyDict::new(py);
                row.set_item("name", &section.name)?;
                row.set_item(
                    "type",
                    match section.section_type {
                        noon::SectionType::Normal => "normal",
                        noon::SectionType::Skip => "skip",
                    },
                )?;
                row.set_item("time", section.time)?;
                Ok(row.into_any().unbind())
            })
            .collect()
    }
    #[pyo3(name = "authoredDuration")]
    fn duration(&self) -> f64 {
        self.handoff().unwrap_or_else(|| self.scene.time())
    }
    #[pyo3(name = "liveHandoffDuration")]
    fn handoff(&self) -> Option<f64> {
        scene_execution::session(&self.scene).map(|s| {
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
            bindings: Vec::new(),
            spatial_domain: None,
        }
    }
    #[pyo3(name = "editMembership")]
    fn edit_membership(
        &mut self,
        batch: &MembershipBatch,
    ) -> PyResult<Option<crate::effects::StagedEffectPublication>> {
        self.available()?;
        for handle in &batch.bindings {
            if !Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
                return Err(engine_error(noon::AuthoringError::ForeignStore));
            }
            handle.validate().map_err(engine_error)?;
        }
        let targets = batch.targets();
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
        if let Some(domain) = batch.spatial_domain {
            if batch.kind != "add" {
                return Err(engine_error(
                    "spatial composition requires an add membership batch",
                ));
            }
            domain
                .apply_scene(&mut self.scene, &targets)
                .map_err(engine_error)?;
        } else {
            let running = self.has_execution();
            match self.scene.edit_membership(request) {
                Ok(_) => {}
                Err(noon::AuthoringError::ExecutionPublication(
                    noon::ExecutionSessionPublicationError::ExternalEffectPreparationRequired,
                )) => {
                    let pending =
                        self.with_effect_preparation(|scene| scene.prepare_membership(request))?;
                    return self.staged(pending, batch.bindings.clone()).map(Some);
                }
                Err(error) => {
                    return Err(engine_error(AuthoringFailure::from_scene_mutation(
                        error, running,
                    )));
                }
            }
        }
        self.bound_ids
            .extend(batch.bindings.iter().map(|handle| handle.node_id()));
        Ok(None)
    }
    #[pyo3(name = "containsMobject")]
    fn contains(&self, target: &MobjectHandle) -> PyResult<bool> {
        self.require_active()?;
        self.scene.contains(&target.handle).map_err(engine_error)
    }
    #[pyo3(name = "containsDirectMobject")]
    fn contains_direct(&self, target: &MobjectHandle) -> PyResult<bool> {
        self.require_active()?;
        self.scene.contains_direct(&target.handle).map_err(|error| {
            engine_error(AuthoringFailure::from_scene_mutation(
                error,
                self.has_execution(),
            ))
        })
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
        self.edit_scene(|scene| scene.add(&target.handle))?;
        self.bound_ids.insert(target.handle.node_id());
        Ok(())
    }
    /// Match the browser value-only projection of the same shared observation.
    #[pyo3(name = "queryMobjectPath")]
    fn path_query(&self, target: &MobjectHandle) -> PyResult<crate::path_query::PathQuery> {
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        self.scene
            .path_query(&target.handle)
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryMobjectColor")]
    fn mobject_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<crate::family::ColorObservation> {
        self.observe_bound_mobject(target, Scene::manim_color)
            .map(Into::into)
    }
    #[pyo3(name = "createFamily")]
    fn create_family(
        &mut self,
        batch: &MembershipBatch,
        z_index: f64,
        draw_enabled: bool,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        let color = noon::integration::family_color(true, red, green, blue, alpha)
            .map_err(engine_error)?
            .expect("required family color");
        self.scene
            .family_with_presentation(&batch.add_targets()?, z_index, color, draw_enabled)
            .map(|family| crate::family::FamilyHandle { family })
            .map_err(engine_error)
    }
    #[pyo3(name = "promoteFamilyOwnPoints")]
    fn promote_family_own_points(
        &mut self,
        target: &crate::family::FamilyHandle,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.promote_own_points((&target.family).into(), references)
    }

    #[pyo3(name = "promoteMobjectOwnPoints")]
    fn promote_mobject_own_points(
        &mut self,
        target: &MobjectHandle,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.promote_own_points((&target.handle).into(), references)
    }

    #[pyo3(name = "copyFamily")]
    fn copy_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.available()?;
        self.same_family_store(target)?;
        if references.kind != "add" {
            return Err(engine_error("family copy references require an add batch"));
        }
        self.scene
            .copy_family_with_references(&target.family, &references.targets())
            .map(|copied| crate::family::FamilyCopy {
                copied: copied.into_entry_copy(),
            })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }
    #[pyo3(name = "cyclicReplaceTarget")]
    fn cyclic_replace_target(
        &mut self,
        target: &crate::family::FamilyHandle,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.available()?;
        self.scene
            .cyclic_replace_target_with_references(&target.family, &references.add_targets()?)
            .map(|copied| crate::family::FamilyCopy {
                copied: copied.into_entry_copy(),
            })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }
    #[pyo3(name = "editFamilyMembership")]
    fn edit_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<Vec<bool>> {
        self.same_family_store(target)?;
        self.edit_children((&target.family).into(), batch)
    }

    #[pyo3(name = "editObjectMembership")]
    fn edit_object_membership(
        &mut self,
        target: &MobjectHandle,
        batch: &MembershipBatch,
    ) -> PyResult<Vec<bool>> {
        self.same_store(target)?;
        self.edit_children((&target.handle).into(), batch)
    }

    #[pyo3(name = "insertFamilyMember")]
    fn insert_family_member(
        &mut self,
        target: &crate::family::FamilyHandle,
        index: usize,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let member = batch.single_member()?;
        self.edit_scene(|scene| scene.insert_family_member(&target.family, index, member))
    }

    #[pyo3(name = "insertObjectMember")]
    fn insert_object_member(
        &mut self,
        target: &MobjectHandle,
        index: usize,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_store(target)?;
        let member = batch.single_member()?;
        self.edit_scene(|scene| scene.insert_family_member(&target.handle, index, member))
    }

    #[pyo3(name = "replaceFamilyMembers")]
    fn replace_family_members(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let members = batch.add_targets()?;
        self.edit_scene(|scene| scene.replace_members((&target.family).into(), &members))
    }

    #[pyo3(name = "replaceObjectMembers")]
    fn replace_object_members(
        &mut self,
        target: &MobjectHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_store(target)?;
        let members = batch.add_targets()?;
        self.edit_scene(|scene| scene.replace_members((&target.handle).into(), &members))
    }

    #[pyo3(name = "invertFamilyMembers")]
    fn invert_family_members(
        &mut self,
        target: &crate::family::FamilyHandle,
        recursive: bool,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.invert_family_members(&target.family, recursive))
    }

    #[pyo3(name = "copyMobjectFamily")]
    fn copy_mobject_family(
        &mut self,
        target: &MobjectHandle,
        references: &MembershipBatch,
    ) -> PyResult<crate::family::FamilyCopy> {
        self.available()?;
        self.same_store(target)?;
        self.scene
            .copy_mobject_with_references(&target.handle, &references.add_targets()?)
            .map(|copied| crate::family::FamilyCopy {
                copied: copied.into_entry_copy(),
            })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    #[pyo3(name = "shiftFamily", signature = (target, x, y, z=0.0))]
    fn shift_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
        z: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| scene.shift_family_3d(&target.family, noon::SemanticVec3::new(x, y, z)))
    }
    #[pyo3(name = "setFamilyFill", signature = (target, has_color, red, green, blue, alpha, opacity=None))]
    #[allow(clippy::too_many_arguments)]
    fn family_fill(
        &mut self,
        target: &crate::family::FamilyHandle,
        has_color: bool,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        let color = noon::integration::family_color(has_color, red, green, blue, alpha)
            .map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_family_fill(&target.family, color, opacity))
    }
    #[pyo3(name = "setFamilyStroke", signature = (target, has_color, red, green, blue, alpha, width=None, opacity=None))]
    #[allow(clippy::too_many_arguments)]
    fn family_stroke(
        &mut self,
        target: &crate::family::FamilyHandle,
        has_color: bool,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        let color = noon::integration::family_color(has_color, red, green, blue, alpha)
            .map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_family_stroke(&target.family, color, width, opacity))
    }
    #[pyo3(name = "setFamilyOpacity")]
    fn family_opacity(
        &mut self,
        target: &crate::family::FamilyHandle,
        opacity: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.set_family_opacity(&target.family, opacity))
    }
    #[pyo3(name = "setFamilyColor")]
    fn family_color(
        &mut self,
        target: &crate::family::FamilyHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.set_family_color(&target.family, r, g, b, a))
    }
    #[pyo3(name = "setFamilyOwnColor")]
    fn family_own_color(
        &mut self,
        target: &crate::family::FamilyHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.edit_scene(|scene| scene.set_family_own_color(&target.family, r, g, b, a))
    }
    #[pyo3(name = "queryMobjectCenter")]
    fn center(&mut self, target: &MobjectHandle) -> PyResult<(f64, f64)> {
        let layout = self.layout(target)?;
        Ok(layout.center)
    }
    #[pyo3(name = "queryFamilyLayout")]
    fn family_layout(
        &mut self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<LayoutObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        self.scene
            .family_layout(&target.family)
            .map(Into::into)
            .map_err(|error| {
                if self.has_execution() {
                    engine_error(noon::LiveSessionError::from(error))
                } else {
                    engine_error(error)
                }
            })
    }
    #[pyo3(name = "queryMobjectLayout")]
    fn layout(&mut self, target: &MobjectHandle) -> PyResult<LayoutObservation> {
        self.require_active()?;
        self.same_store(target)?;
        self.observe_mobject(target, |scene, object| {
            scene.layout(object, noon::AuthoredLayoutCenter::Boundary)
        })
        .map(Into::into)
    }
    #[pyo3(name = "targetEditor")]
    fn target(&mut self, source: &MobjectHandle) -> PyResult<MobjectHandle> {
        self.available()?;
        self.scene
            .target_editor(&source.handle)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }
    #[pyo3(name = "createManimGeometry")]
    fn geometry(&mut self, options: &GeometryOptions) -> PyResult<MobjectHandle> {
        self.available()?;
        let running = self.has_execution();
        self.scene
            .geometry(options.options.clone())
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "createImage")]
    fn image(
        &mut self,
        mut options: PyRefMut<'_, crate::image::ImageOptions>,
    ) -> PyResult<MobjectHandle> {
        crate::image::create_in_scene(self, &mut options)
    }
    #[pyo3(name = "setUpdatingSuspended")]
    fn set_updating_suspended(
        &mut self,
        target: &MobjectHandle,
        suspended: bool,
        recursive: bool,
    ) -> PyResult<()> {
        self.edit_scene(|scene| scene.set_updating_suspended(&target.handle, suspended, recursive))
    }

    #[pyo3(name = "setFamilyUpdatingSuspended")]
    fn set_family_updating_suspended(
        &mut self,
        target: &crate::family::FamilyHandle,
        suspended: bool,
        recursive: bool,
    ) -> PyResult<()> {
        self.edit_scene(|scene| scene.set_updating_suspended(&target.family, suspended, recursive))
    }

    #[pyo3(name = "setImageSampling")]
    fn set_image_sampling(&mut self, target: &MobjectHandle, value: &str) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let sampling = noon::integration::image_sampling(value).map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_image_sampling(&target.handle, sampling))
    }
    #[pyo3(name = "changeAnchorMode")]
    fn change_anchor_mode(&mut self, target: &MobjectHandle, smooth: bool) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.change_anchor_mode(object, smooth)
        })
    }
    #[pyo3(name = "changeFamilyAnchorMode")]
    fn change_family_anchor_mode(
        &mut self,
        target: &crate::family::FamilyHandle,
        smooth: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.scene
            .change_family_anchor_mode(&target.family, smooth)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryFamilyOwnPath")]
    fn family_own_path(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::path_query::PathQuery> {
        self.require_active()?;
        self.scene
            .family_own_path_query(&target.family)
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
    #[pyo3(name = "rescaleToFit")]
    fn rescale_to_fit(
        &mut self,
        target: &LayoutAnchor,
        length: f64,
        dimension: u32,
        stretch: bool,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        let dimension = dimension.try_into().map_err(engine_error)?;
        self.place(|scene| {
            scene.rescale_to_fit_with_pivot(
                &target.anchor,
                length,
                dimension,
                stretch,
                crate::mobject::pivot_3d(x, y, z, point),
            )
        })
    }
    #[pyo3(name="startNewPath", signature=(target ,x,y,z))]
    fn start_new_path(&mut self, target: &MobjectHandle, x: f64, y: f64, z: f64) -> PyResult<()> {
        let point = crate::path_input::point_3d(x, y, z)?;
        self.edit_path(target, |scene, object| {
            scene.start_new_path_3d(object, point)
        })
    }
    #[pyo3(name="startNewPathWithStride", signature=(target ,x,y,z,stride))]
    fn start_new_path_stride(
        &mut self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        stride: i32,
    ) -> PyResult<()> {
        let point = crate::path_input::point_3d(x, y, z)?;
        self.edit_path(target, |scene, object| {
            scene.start_new_path_3d_with_stride(object, point, stride)
        })
    }
    #[pyo3(name="addLineTo", signature=(target ,x,y,z,stride=None))]
    fn add_line_to(
        &mut self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        stride: Option<i32>,
    ) -> PyResult<()> {
        let point = crate::path_input::point_3d(x, y, z)?;
        self.edit_path(target, |scene, object| {
            scene.add_line_to_3d_with_stride(object, point, stride.unwrap_or(4))
        })
    }
    #[pyo3(name="addLineToWithSamples", signature=(target ,x,y,z,stride,samples))]
    fn add_line_samples(
        &mut self,
        target: &MobjectHandle,
        x: f64,
        y: f64,
        z: f64,
        stride: i32,
        samples: Vec<f64>,
    ) -> PyResult<()> {
        let point = crate::path_input::point_3d(x, y, z)?;
        let samples: [f64; 4] = samples.try_into().map_err(|_| {
            engine_error("add_line_to requires four constructor interpolation samples")
        })?;
        self.edit_path(target, |scene, object| {
            scene.add_line_to_3d_with_stride_and_samples(object, point, stride, &samples)
        })
    }
    #[pyo3(name="setPoints3D", signature=(target ,values))]
    fn set_points_3d(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| scene.set_points_3d(object, &points))
    }
    #[pyo3(name="appendPoints3D", signature=(target ,values))]
    fn append_points_3d(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.append_points_3d(object, &points)
        })
    }
    #[pyo3(name="setPointsAsCorners", signature=(target ,values))]
    fn corners(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_points_as_corners_3d(object, &points)
        })
    }
    #[pyo3(name="addPointsAsCorners", signature=(target ,values))]
    fn append_corners(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.add_points_as_corners_3d(object, &points)
        })
    }
    #[pyo3(name="addCubicBezierCurves", signature=(target ,values))]
    fn cubics(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.add_cubic_bezier_curves_3d(object, &curves)
        })
    }
    #[pyo3(name="setCubicBezierCurves", signature=(target ,values))]
    fn set_cubics(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_cubic_bezier_curves_3d(object, &curves)
        })
    }
    #[pyo3(name="setPointsSmoothly", signature=(target ,values))]
    fn smooth_points(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_points_smoothly(object, &points)
        })
    }
    #[pyo3(name="setPointsAsCornersWithSamples", signature=(target ,values,samples))]
    fn corners_samples(
        &mut self,
        target: &MobjectHandle,
        values: Vec<f64>,
        samples: Vec<f64>,
    ) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        let samples: [f64; 4] = samples.try_into().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("set_points_as_corners requires four samples")
        })?;
        self.edit_path(target, |scene, object| {
            scene.set_points_as_corners_3d_with_samples(object, &points, &samples)
        })
    }
    #[pyo3(name="addPointsAsCornersWithStride", signature=(target ,values,stride,samples))]
    fn append_corners_stride(
        &mut self,
        target: &MobjectHandle,
        values: Vec<f64>,
        stride: i32,
        samples: Vec<f64>,
    ) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.add_points_as_corners_3d_with_stride(object, &points, stride, &samples)
        })
    }
    #[pyo3(name="clearPoints", signature=(target))]
    fn clear_points(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.clear_points(object))
    }
    #[pyo3(name="closePath", signature=(target))]
    fn close_path(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.close_path(object))
    }
    #[pyo3(name="reverseDirection", signature=(target))]
    fn reverse_direction(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.reverse_direction(object))
    }
    #[pyo3(name="reversePoints", signature=(target))]
    fn reverse_points(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.reverse_points(object))
    }
    #[pyo3(name="repeatPoints", signature=(target ,count))]
    fn repeat_points(&mut self, target: &MobjectHandle, count: usize) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.repeat_points(object, count))
    }
    #[pyo3(name="insertNCurves", signature=(target ,additional))]
    fn insert_curves(&mut self, target: &MobjectHandle, additional: usize) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.insert_n_curves(object, additional)
        })
    }
    #[pyo3(name="scaleHandleToAnchorDistances", signature=(target ,factor))]
    fn scale_handles(&mut self, target: &MobjectHandle, factor: f64) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.scale_handle_to_anchor_distances(object, factor)
        })
    }
    #[pyo3(name="closePathWithStride", signature=(target ,stride,tolerance,samples))]
    fn close_path_stride(
        &mut self,
        target: &MobjectHandle,
        stride: i32,
        tolerance: f64,
        samples: Vec<f64>,
    ) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.close_path_with_stride(object, stride, tolerance, &samples)
        })
    }
    #[pyo3(name="pointwiseBecomePartial", signature=(target ,source,a,b))]
    fn partial(
        &mut self,
        target: &MobjectHandle,
        source: &MobjectHandle,
        a: f64,
        b: f64,
    ) -> PyResult<()> {
        self.same_store(source)?;
        self.edit_path(target, |scene, object| {
            scene.pointwise_become_partial(object, &source.handle, a, b)
        })
    }
    #[pyo3(name="appendVectorizedMobject", signature=(target ,source))]
    fn append_vector(&mut self, target: &MobjectHandle, source: &MobjectHandle) -> PyResult<()> {
        self.same_store(source)?;
        self.edit_path(target, |scene, object| {
            scene.append_vectorized_mobject(object, &source.handle)
        })
    }
    #[pyo3(name="appendVectorizedMobjectWithStride", signature=(target ,source,stride))]
    fn append_vector_stride(
        &mut self,
        target: &MobjectHandle,
        source: &MobjectHandle,
        stride: i32,
    ) -> PyResult<()> {
        self.same_store(source)?;
        self.edit_path(target, |scene, object| {
            scene.append_vectorized_mobject_with_stride(object, &source.handle, stride)
        })
    }
    #[pyo3(name="addQuadraticBezierCurveTo", signature=(target ,control_x,control_y,control_z,anchor_x,anchor_y,anchor_z,stride=None))]
    fn path_quadratic(
        &mut self,
        target: &MobjectHandle,
        control_x: f64,
        control_y: f64,
        control_z: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_z: f64,
        stride: Option<i32>,
    ) -> PyResult<()> {
        let control = crate::path_input::point_3d(control_x, control_y, control_z)?;
        let anchor = crate::path_input::point_3d(anchor_x, anchor_y, anchor_z)?;
        self.edit_path(target, |scene, object| {
            scene.add_quadratic_bezier_curve_to_3d_with_stride(
                object,
                control,
                anchor,
                stride.unwrap_or(4),
            )
        })
    }
    #[pyo3(name="addCubicBezierCurveTo", signature=(target ,control1_x,control1_y,control1_z,control2_x,control2_y,control2_z,anchor_x,anchor_y,anchor_z,stride=None))]
    fn path_cubic(
        &mut self,
        target: &MobjectHandle,
        control1_x: f64,
        control1_y: f64,
        control1_z: f64,
        control2_x: f64,
        control2_y: f64,
        control2_z: f64,
        anchor_x: f64,
        anchor_y: f64,
        anchor_z: f64,
        stride: Option<i32>,
    ) -> PyResult<()> {
        let control1 = crate::path_input::point_3d(control1_x, control1_y, control1_z)?;
        let control2 = crate::path_input::point_3d(control2_x, control2_y, control2_z)?;
        let anchor = crate::path_input::point_3d(anchor_x, anchor_y, anchor_z)?;
        self.edit_path(target, |scene, object| {
            scene.add_cubic_bezier_curve_to_3d_with_stride(
                object,
                control1,
                control2,
                anchor,
                stride.unwrap_or(4),
            )
        })
    }
    #[pyo3(name="addCubicBezierCurve", signature=(target ,anchor1_x,anchor1_y,anchor1_z,handle1_x,handle1_y,handle1_z,handle2_x,handle2_y,handle2_z,anchor2_x,anchor2_y,anchor2_z))]
    fn path_curve(
        &mut self,
        target: &MobjectHandle,
        anchor1_x: f64,
        anchor1_y: f64,
        anchor1_z: f64,
        handle1_x: f64,
        handle1_y: f64,
        handle1_z: f64,
        handle2_x: f64,
        handle2_y: f64,
        handle2_z: f64,
        anchor2_x: f64,
        anchor2_y: f64,
        anchor2_z: f64,
    ) -> PyResult<()> {
        let anchor1 = crate::path_input::point_3d(anchor1_x, anchor1_y, anchor1_z)?;
        let handle1 = crate::path_input::point_3d(handle1_x, handle1_y, handle1_z)?;
        let handle2 = crate::path_input::point_3d(handle2_x, handle2_y, handle2_z)?;
        let anchor2 = crate::path_input::point_3d(anchor2_x, anchor2_y, anchor2_z)?;
        self.edit_path(target, |scene, object| {
            scene.add_cubic_bezier_curve_3d(object, anchor1, handle1, handle2, anchor2)
        })
    }
    #[pyo3(name="addSmoothCurveTo", signature=(target ,values,stride=None))]
    fn smooth_curve(
        &mut self,
        target: &MobjectHandle,
        values: Vec<f64>,
        stride: Option<i32>,
    ) -> PyResult<()> {
        let points = crate::path_input::points(&values)?;
        let (handle2, anchor) = match points.as_slice() {
            [anchor] => (None, *anchor),
            [handle2, anchor] => (Some(*handle2), *anchor),
            _ => {
                return Err(engine_error(
                    "Only call add_smooth_curve_to with 1 or 2 points",
                ));
            }
        };
        self.edit_path(target, |scene, object| {
            scene.add_smooth_curve_to_with_stride(object, handle2, anchor, stride.unwrap_or(4))
        })
    }
    #[pyo3(name = "setFamilyPoints3D")]
    fn set_family_points_3d(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let values = crate::path_input::points_3d(&values)?;
        self.publish_path_edit(|scene| scene.set_family_points_3d(&target.family, &values))
    }
    #[pyo3(name = "appendFamilyPoints3D")]
    fn append_family_points_3d(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let values = crate::path_input::points_3d(&values)?;
        self.publish_path_edit(|scene| scene.append_family_points_3d(&target.family, &values))
    }
    #[pyo3(name = "clearFamilyPoints")]
    fn clear_family_points(&mut self, target: &crate::family::FamilyHandle) -> PyResult<()> {
        self.same_family_store(target)?;
        self.publish_path_edit(|scene| scene.clear_family_points(&target.family))
    }
    #[pyo3(name = "setFamilyEmptyPointArray")]
    fn empty_family_array(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let shape = crate::path_input::point_array_shape(&values)?;
        self.publish_path_edit(|scene| scene.set_empty_point_array((&target.family).into(), shape))
    }
    #[pyo3(name = "setEmptyPointArray")]
    fn empty_point_array(&mut self, target: &MobjectHandle, dimensions: Vec<f64>) -> PyResult<()> {
        self.same_store(target)?;
        let shape = crate::path_input::point_array_shape(&dimensions)?;
        self.publish_path_edit(|scene| scene.set_empty_point_array((&target.handle).into(), shape))
    }
    #[pyo3(name = "subcurve")]
    fn subcurve(&mut self, target: &MobjectHandle, a: f64, b: f64) -> PyResult<MobjectHandle> {
        self.same_store(target)?;
        self.require_active()?;
        self.scene
            .subcurve(&target.handle, a, b)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(noon::LiveSessionError::from)
            .map_err(engine_error)
    }
    #[pyo3(name = "alignRootPoints")]
    fn align_root_points(&mut self, left: &LayoutAnchor, right: &LayoutAnchor) -> PyResult<()> {
        self.publish_path_edit(|scene| scene.align_path_points(&left.anchor, &right.anchor))
    }
    #[pyo3(name = "alignPoints")]
    fn align_points(&mut self, left: &MobjectHandle, right: &MobjectHandle) -> PyResult<()> {
        self.same_store(left)?;
        self.same_store(right)?;
        self.publish_path_edit(|scene| scene.align_points(&left.handle, &right.handle))
    }
    #[pyo3(name = "matchPoints")]
    fn match_points(&mut self, source: &MobjectHandle, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(source, |scene, source| {
            scene.match_points(source, &target.handle)
        })
    }
    #[pyo3(name = "reverseFamilyPoints")]
    fn reverse_family_points(&mut self, family: &crate::family::FamilyHandle) -> PyResult<()> {
        self.available()?;
        self.scene
            .reverse_family_points(&family.family)
            .map_err(engine_error)
    }
    #[pyo3(name = "repeatFamilyPoints")]
    fn repeat_family_points(
        &mut self,
        family: &crate::family::FamilyHandle,
        count: usize,
    ) -> PyResult<()> {
        self.available()?;
        self.scene
            .repeat_family_points(&family.family, count)
            .map_err(engine_error)
    }
    #[pyo3(name = "applyMatrix")]
    #[allow(clippy::too_many_arguments)]
    fn apply_matrix(
        &mut self,
        target: &MobjectHandle,
        values: Vec<f64>,
        rows: usize,
        columns: usize,
        x: f64,
        y: f64,
    ) -> PyResult<()> {
        self.same_store(target)?;
        self.place(|scene| scene.apply_matrix(&target.handle, &values, rows, columns, x, y))
    }
    #[pyo3(name = "shift", signature = (target, x, y, z = None))]
    fn shift(&mut self, target: &MobjectHandle, x: f64, y: f64, z: Option<f64>) -> PyResult<()> {
        self.edit_scene_property(|scene| {
            scene.shift_3d(
                &target.handle,
                noon::SemanticVec3::new(x, y, z.unwrap_or(0.0)),
            )
        })
    }
    #[pyo3(name = "setFill")]
    fn fill(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_fill_rgb(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "setFillColor")]
    fn fill_color(
        &mut self,
        target: &MobjectHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_fill_color(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "setFillOpacity")]
    fn fill_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_fill_opacity(&target.handle, a))
    }
    #[pyo3(name = "setColor")]
    fn color(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_color_rgba(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "setColorGradient")]
    fn set_color_gradient(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        self.available()?;
        let colors = noon::integration::gradient_colors(&values).map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_color_by_gradient(&target.handle, &colors))
    }

    #[pyo3(name = "setFamilyColorGradient")]
    fn set_family_color_gradient(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let colors = noon::integration::gradient_colors(&values).map_err(engine_error)?;
        self.edit_scene(|scene| scene.set_family_color_by_gradient(&target.family, &colors))
    }

    #[pyo3(name = "setColorRadialGradient")]
    fn set_color_radial_gradient(
        &mut self,
        target: &MobjectHandle,
        center_x: f64,
        center_y: f64,
        center_z: f64,
        radius: f64,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let (inner, outer) =
            noon::integration::radial_gradient_colors(&values).map_err(engine_error)?;
        self.edit_scene(|scene| {
            scene.set_color_by_radial_gradient(
                &target.handle,
                (center_x, center_y, center_z),
                radius,
                inner,
                outer,
            )
        })
    }

    #[pyo3(name = "setFamilyColorRadialGradient")]
    fn set_family_color_radial_gradient(
        &mut self,
        target: &crate::family::FamilyHandle,
        center_x: f64,
        center_y: f64,
        center_z: f64,
        radius: f64,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.available()?;
        let (inner, outer) =
            noon::integration::radial_gradient_colors(&values).map_err(engine_error)?;
        self.edit_scene(|scene| {
            scene.set_family_color_by_radial_gradient(
                &target.family,
                (center_x, center_y, center_z),
                radius,
                inner,
                outer,
            )
        })
    }

    #[pyo3(name = "setOpacity")]
    fn opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_opacity(&target.handle, a))
    }
    #[pyo3(name = "setObjectOpacity")]
    fn object_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_object_opacity(&target.handle, a))
    }
    #[pyo3(name = "zIndex")]
    fn z_index(&self, target: &LayoutAnchor) -> PyResult<f64> {
        self.require_active()?;
        self.scene.z_index(&target.anchor).map_err(engine_error)
    }
    #[pyo3(name = "setZIndex")]
    fn set_z_index(&mut self, target: &LayoutAnchor, value: f64, family: bool) -> PyResult<()> {
        self.available()?;
        self.scene
            .set_z_index(&target.anchor, value, family)
            .map_err(engine_error)
    }
    #[pyo3(name = "rotateLayout")]
    fn rotate(
        &mut self,
        target: &LayoutAnchor,
        angle: f64,
        x: f64,
        y: f64,
        point: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| scene.rotate_layout(&target.anchor, angle, pivot(x, y, point)))
    }
    #[pyo3(name = "flipLayout")]
    #[allow(clippy::too_many_arguments)]
    fn flip_layout(
        &mut self,
        source: &LayoutAnchor,
        axis_x: f64,
        axis_y: f64,
        axis_z: f64,
        x: f64,
        y: f64,
        point: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.flip_layout(
                &source.anchor,
                noon::SemanticVec3::new(axis_x, axis_y, axis_z),
                pivot(x, y, point),
            )
        })
    }

    #[pyo3(name = "replaceLayout")]
    fn replace_layout(
        &mut self,
        source: &LayoutAnchor,
        target: &LayoutAnchor,
        dimension: u32,
        stretch: bool,
    ) -> PyResult<()> {
        let dimension = dimension.try_into().map_err(engine_error)?;
        self.place(|scene| scene.replace_layout(&source.anchor, &target.anchor, dimension, stretch))
    }

    #[pyo3(name = "surroundCircle")]
    fn surround_circle(
        &mut self,
        source: &LayoutAnchor,
        target: &LayoutAnchor,
        dimension: u32,
        stretch: bool,
        buffer_factor: f64,
    ) -> PyResult<()> {
        self.place(|scene| {
            scene.surround_circle(
                &source.anchor,
                &target.anchor,
                dimension,
                stretch,
                buffer_factor,
            )
        })
    }

    #[pyo3(name = "matchDimSize")]
    #[allow(clippy::too_many_arguments)]
    fn match_dim_size(
        &mut self,
        source: &LayoutAnchor,
        target: &LayoutAnchor,
        dimension: u32,
        stretch: bool,
        x: f64,
        y: f64,
        z: f64,
        point: bool,
    ) -> PyResult<()> {
        let dimension = dimension.try_into().map_err(engine_error)?;
        self.place(|scene| {
            scene.match_dim_size_with_pivot(
                &source.anchor,
                &target.anchor,
                dimension,
                stretch,
                crate::mobject::pivot_3d(x, y, z, point),
            )
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
    #[pyo3(name = "prepareEffectAddition")]
    fn prepare_effect_addition(
        &mut self,
        target: &MobjectHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.available()?;
        self.same_store(target)?;
        let pending = self.with_effect_preparation(|scene| {
            scene.prepare_effect_attachments([(&target.handle, definition.definition(), name)])
        })?;
        self.staged(pending, Vec::new())
    }
    #[pyo3(name = "prepareFamilyMembership")]
    fn prepare_family_membership(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.prepare_children((&target.family).into(), batch)
    }

    #[pyo3(name = "prepareObjectMembership")]
    fn prepare_object_membership(
        &mut self,
        target: &MobjectHandle,
        batch: &MembershipBatch,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.prepare_children((&target.handle).into(), batch)
    }

    #[pyo3(name = "prepareFamilyEffectAddition")]
    fn prepare_family_effect_addition(
        &mut self,
        target: &crate::family::FamilyHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.available()?;
        self.same_family_store(target)?;
        let pending = self.with_effect_preparation(|scene| {
            scene.prepare_effect_attachments([(&target.family, definition.definition(), name)])
        })?;
        self.staged(pending, Vec::new())
    }
    #[pyo3(name = "prepareOrdinaryComposition")]
    fn prepare_composition(
        &mut self,
        candidate: &Composition,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.available()?;
        let pending = self.with_bootstrapped_live(|live| {
            live.prepare_composition(&candidate.request(), candidate.play_options)
        })?;
        self.staged(pending, candidate.bindings.clone())
    }
    fn publish_prepared_effects(
        &mut self,
        py: Python<'_>,
        mut staged: PyRefMut<'_, crate::effects::StagedEffectPublication>,
    ) -> PyResult<Option<Py<pyo3::types::PyDict>>> {
        self.available()?;
        let session = scene_execution::session(&self.scene)
            .ok_or_else(|| engine_error("no execution session"))?;
        if session.runtime_identity() != staged.runtime {
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "foreign_handle",
                "shader.foreign_preparation",
                "shader preparation belongs to another runtime",
            )));
        }
        let pending = staged
            .pending
            .as_ref()
            .ok_or_else(|| engine_error("effect publication is consumed"))?;
        if !pending.programs().is_empty() {
            if !self.prepare_shaders {
                return Err(engine_error(noon::integration::AuthoringFailure::new(
                    "unsupported_operation",
                    "shader.device_preparation_required",
                    "live shader publication requires native GPU preparation; enable prepare_shaders",
                )));
            }
            #[cfg(not(feature = "capture"))]
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "unsupported_operation",
                "shader.device_unavailable",
                "build the native extension with capture for live shader preparation",
            )));
            #[cfg(feature = "capture")]
            {
                if self.effect_preparer.is_none() {
                    self.effect_preparer = Some(
                        noon_export::SessionCapture::new(
                            session,
                            noon_export::CaptureOptions::new(1, 1),
                        )
                        .map_err(|e| engine_error(e.to_string()))?,
                    );
                }
                self.effect_preparer
                    .as_mut()
                    .expect("effect device prepared")
                    .prepare_effect_programs(session, pending.programs())
                    .map_err(|e| engine_error(e.to_string()))?;
            }
        }
        let pending = staged
            .pending
            .take()
            .expect("pending checked before device preparation");
        let segment = self.with_live(|live| live.publish_staged_effects(pending))?;
        self.bound_ids
            .extend(staged.bindings.iter().map(Mobject::node_id));
        if let Some(segment) = segment {
            self.segment = Some(segment);
            self.segments += 1;
            self.drive_impl(py).map(Some)
        } else {
            Ok(None)
        }
    }
    #[pyo3(name = "beginOrdinaryComposition")]
    fn begin(&mut self, candidate: &Composition) -> PyResult<()> {
        self.available()?;
        self.segment = Some(self.with_bootstrapped_live(|live| {
            live.declare_and_activate_composition(&candidate.request(), candidate.play_options)
        })?);
        self.bound_ids
            .extend(candidate.bindings.iter().map(|handle| handle.node_id()));
        self.segments += 1;
        Ok(())
    }
    #[pyo3(name = "declareLiveTransformTo")]
    fn declare_transform(
        &mut self,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
    ) -> PyResult<crate::composition::DeclaredAnimation> {
        self.available()?;
        self.same_store(source)?;
        self.same_store(target)?;
        let running = self.has_execution();
        self.scene
            .declare_transform_to(
                &source.handle,
                &target.handle,
                noon::AnimationOptions::new()
                    .run_time(duration)
                    .rate_func(crate::composition::rate(easing)?),
            )
            .map(|value| crate::composition::DeclaredAnimation { value })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, running)))
    }
    #[pyo3(name = "beginLiveExecution")]
    fn begin_live_execution(&mut self, duration: f64) -> PyResult<()> {
        self.require_active()?;
        if !duration.is_finite() || duration <= 0.0 {
            return Err(engine_error(
                "playback duration must be finite and positive",
            ));
        }
        if self.handoff().is_some_and(|end| duration < end) {
            return Err(engine_error(
                "playback duration is shorter than the live handoff",
            ));
        }
        if !self.has_execution() {
            let session = self.bootstrap()?;
            self.attach_session(session)?;
        }
        self.observation_horizon = Some(duration);
        Ok(())
    }
    #[pyo3(name = "livePlayAnimation")]
    fn play_declared(
        &mut self,
        animation: &crate::composition::DeclaredAnimation,
    ) -> PyResult<f64> {
        self.available()?;
        let segment = self.with_live(|live| live.play_animation(&animation.value))?;
        let end = segment.end_time();
        self.segment = Some(segment);
        self.segments += 1;
        Ok(end)
    }
    #[pyo3(name = "liveWait")]
    fn live_wait(&mut self, duration: f64) -> PyResult<f64> {
        self.available()?;
        let segment = self.with_live(|live| live.wait_segment(duration))?;
        let end = segment.end_time();
        self.segment = Some(segment);
        self.segments += 1;
        Ok(end)
    }
    #[pyo3(name = "liveAdvanceSegmentTo")]
    fn advance_segment(&mut self, time: f64) -> PyResult<bool> {
        self.require_active()?;
        let segment = self
            .segment
            .ok_or_else(|| engine_error("no pending segment"))?;
        self.with_live(|live| {
            live.advance_segment_to(segment, time)?;
            Ok(live.segment_state(segment).is_complete())
        })
    }
    #[pyo3(name = "liveCompleteSegment")]
    fn complete_segment(&mut self) -> PyResult<()> {
        self.require_active()?;
        let segment = self
            .segment
            .ok_or_else(|| engine_error("no pending segment"))?;
        self.with_live(|live| live.complete_segment(segment))?;
        self.segment = None;
        Ok(())
    }
    #[pyo3(name = "liveEvaluate")]
    fn evaluate(&mut self, time: f64) -> PyResult<()> {
        self.require_active()?;
        let horizon = self
            .observation_horizon
            .ok_or_else(|| engine_error("begin live execution before evaluating"))?;
        let horizon = horizon.max(self.handoff().unwrap_or(0.0));
        if !time.is_finite() || time < 0.0 || time > horizon {
            return Err(engine_error(noon::integration::AuthoringFailure::new(
                "invalid_input",
                "python.live_time_outside_range",
                "requested time is outside the live observation range",
            )));
        }
        scene_execution::evaluate(&mut self.scene, time)
            .map_err(|error| engine_error(error.to_string()))
    }
    #[pyo3(name = "queryEffectiveTranslation")]
    fn query_effective_translation(&self, target: &MobjectHandle) -> PyResult<(f64, f64)> {
        self.require_active()?;
        self.same_store(target)?;
        self.scene
            .effective(&target.handle)
            .map(|value| {
                (
                    value.transform.translation.x.into(),
                    value.transform.translation.y.into(),
                )
            })
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }
    #[pyo3(name = "setTranslation")]
    fn set_translation(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene_property(|scene| scene.set_translation(&target.handle, x, y))
    }
    #[pyo3(name = "setScale")]
    fn set_scale(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene_property(|scene| scene.set_scale(&target.handle, x, y))
    }
    #[pyo3(name = "setRotation")]
    fn set_rotation(&mut self, target: &MobjectHandle, angle: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.edit_scene_property(|scene| scene.set_rotation(&target.handle, angle))
    }
    #[pyo3(name = "replaceContent")]
    fn replace_content(&mut self, target: &MobjectHandle, source: &MobjectHandle) -> PyResult<()> {
        self.same_store(target)?;
        self.same_store(source)?;
        self.edit_scene_property(|scene| scene.replace_content(&target.handle, &source.handle))
    }
    #[pyo3(name = "becomeMobject")]
    fn become_mobject(
        &mut self,
        target: &MobjectHandle,
        source: &MobjectHandle,
        match_height: bool,
        match_width: bool,
        match_center: bool,
        stretch: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.same_store(source)?;
        let options = noon::ManimBecomeOptions {
            match_height,
            match_width,
            match_center,
            stretch,
        };
        self.scene
            .become_mobject(&target.handle, &source.handle, options)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }
    #[pyo3(name = "becomeFamily")]
    fn become_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        source: &crate::family::FamilyHandle,
        match_height: bool,
        match_width: bool,
        match_center: bool,
        stretch: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.same_family_store(source)?;
        self.scene
            .become_family(
                &target.family,
                &source.family,
                noon::ManimBecomeOptions {
                    match_height,
                    match_width,
                    match_center,
                    stretch,
                },
            )
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_capture(error)))
    }

    #[pyo3(name = "beginOrdinaryWait")]
    fn wait(&mut self, duration: f64) -> PyResult<()> {
        self.available()?;
        self.segment = Some(self.with_bootstrapped_live(|live| live.wait_segment(duration))?);
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
        let callback = id
            .parse()
            .map_err(|_| engine_error("invalid callback ID"))?;
        self.edit_scene(|scene| {
            scene_execution::add_updater(
                scene,
                &target.handle,
                HostCallbackId::new(callback),
                time,
                position,
            )
        })
    }

    #[pyo3(name = "removeUpdater")]
    fn remove_updater(&mut self, target: &MobjectHandle, id: &str, time: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let callback = id
            .parse()
            .map_err(|_| engine_error("invalid callback ID"))?;
        self.edit_scene(|scene| {
            scene_execution::remove_updater(
                scene,
                &target.handle,
                HostCallbackId::new(callback),
                time,
            )
        })
    }

    #[pyo3(name = "clearUpdaters")]
    fn clear_updaters(&mut self, target: &MobjectHandle, time: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.edit_scene(|scene| scene_execution::clear_updaters(scene, &target.handle, time))
    }

    #[pyo3(signature = (sample_hz, retain_history=false, prepare_shaders=true))]
    fn configure(
        &mut self,
        sample_hz: f64,
        retain_history: bool,
        prepare_shaders: bool,
    ) -> PyResult<()> {
        if !sample_hz.is_finite() || sample_hz <= 0.0 {
            return Err(engine_error("sample rate must be positive and finite"));
        }
        if self.has_execution() {
            return Err(engine_error("configure sample rate before execution"));
        }
        self.sample_hz = sample_hz;
        self.retain_history = retain_history;
        self.prepare_shaders = prepare_shaders;
        Ok(())
    }
    fn retire(&mut self) -> PyResult<()> {
        if self.terminal {
            return Ok(());
        }
        scene_execution::retire(&mut self.scene).map_err(|error| match error {
            scene_execution::SceneRetirementError::Callback(error) => engine_error(error),
            error => engine_error(error.to_string()),
        })?;
        #[cfg(feature = "capture")]
        {
            self.effect_preparer = None;
        }
        self.terminal = true;
        self.segment = None;
        self.pending_ack = None;
        self.callback_membership = None;
        self.requested_sample = None;
        Ok(())
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

    #[pyo3(name = "queryMobjectLineEndpoints")]
    fn query_line_endpoints(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<crate::mobject::LineEndpoints> {
        let value = self.observe_bound_mobject(target, Scene::line_endpoints)?;
        Ok(crate::mobject::LineEndpoints { value })
    }

    #[pyo3(name = "queryMobjectFillColor")]
    fn query_fill_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<Option<crate::family::ColorObservation>> {
        self.observe_bound_mobject(target, Scene::fill_color)
            .map(|color| color.map(Into::into))
    }
    #[pyo3(name = "queryMobjectStrokeColor")]
    fn query_stroke_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<Option<crate::family::ColorObservation>> {
        self.observe_bound_mobject(target, Scene::stroke_color)
            .map(|color| color.map(Into::into))
    }
    #[pyo3(name = "queryMobjectBackgroundStrokeColor")]
    fn query_background_stroke_color(
        &self,
        target: &MobjectHandle,
    ) -> PyResult<Option<crate::family::ColorObservation>> {
        self.observe_bound_mobject(target, Scene::background_stroke_color)
            .map(|color| color.map(Into::into))
    }
    #[pyo3(name = "queryMobjectBackgroundStrokeWidth")]
    fn query_background_stroke_width(&self, target: &MobjectHandle) -> PyResult<f64> {
        self.observe_bound_mobject(target, Scene::background_stroke_width)
    }
    #[pyo3(name = "queryMobjectBackgroundStrokeOpacity")]
    fn query_background_stroke_opacity(&self, target: &MobjectHandle) -> PyResult<f64> {
        self.observe_bound_mobject(target, Scene::background_stroke_opacity)
    }
    #[pyo3(name = "queryMobjectFillOpacity")]
    fn query_fill_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
            self.require_active()?;
        }
        self.observe_mobject(target, Scene::fill_opacity)
    }
    #[pyo3(name = "queryMobjectStrokeOpacity")]
    fn query_stroke_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
            self.require_active()?;
        }
        self.observe_mobject(target, Scene::stroke_opacity)
    }
    #[pyo3(name = "queryMobjectStrokeWidth")]
    fn query_stroke_width(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
            self.require_active()?;
        }
        self.observe_mobject(target, Scene::stroke_width)
    }
    #[pyo3(name = "moveToPoint")]
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
        self.place(|scene| {
            scene.move_to(
                &target.handle,
                noon::LayoutTarget::Point(x, y),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "moveToLayout")]
    fn move_to_layout(
        &mut self,
        source: &MobjectHandle,
        target: &LayoutAnchor,
        edge_x: f64,
        edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.move_to(
                &source.handle,
                noon::LayoutTarget::Anchor(&target.anchor),
                (edge_x, edge_y),
                (mask_x, mask_y),
            )
        })
    }
    #[pyo3(name = "alignMobjectOnFrame")]
    fn align_mobject_on_frame(
        &mut self,
        source: &MobjectHandle,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.place(|scene| {
            scene.align_mobject_on_frame(&source.handle, (direction_x, direction_y), buff)
        })
    }
    #[pyo3(name = "moveToMobject")]
    fn move_to_mobject(
        &mut self,
        target: &MobjectHandle,
        reference: &MobjectHandle,
        ex: f64,
        ey: f64,
        mx: f64,
        my: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.same_store(reference)?;
        self.place(|scene| {
            scene.move_to(
                &target.handle,
                noon::LayoutTarget::Mobject(&reference.handle),
                (ex, ey),
                (mx, my),
            )
        })
    }
    fn runtime_identity(&self) -> PyResult<Option<u64>> {
        self.require_active()?;
        Ok(scene_execution::session(&self.scene).map(|s| s.runtime_identity().get()))
    }
    #[pyo3(name = "setStrokeColor")]
    fn set_stroke_color(
        &mut self,
        target: &MobjectHandle,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_stroke_color(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "setStrokeOpacity")]
    fn set_stroke_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_scene(|scene| scene.set_stroke_opacity(&target.handle, a))
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
    fn stage_callback_provisional_geometry(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        options: &GeometryOptions,
    ) -> PyResult<crate::callback::ProvisionalMobject> {
        self.stage_provisional_geometry(py, token, options.options.clone())
    }
    fn stage_callback_provisional_shift(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        object: &crate::callback::ProvisionalMobject,
        x: f64,
        y: f64,
    ) -> PyResult<()> {
        self.with_provisional(py, token, object, |collector, store, local| {
            collector.stage_provisional_shift(store, local, x, y)
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn stage_callback_provisional_fill(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        object: &crate::callback::ProvisionalMobject,
        rgba: [f64; 4],
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.with_provisional(py, token, object, |collector, store, local| {
            collector.stage_provisional_fill(store, local, rgba, opacity)
        })
    }
    fn callback_provisional_center(
        &mut self,
        py: Python<'_>,
        token: &Bound<'_, PyAny>,
        object: &crate::callback::ProvisionalMobject,
    ) -> PyResult<crate::callback::ProvisionalPoint> {
        let (x, y) = self.with_provisional(py, token, object, |collector, store, local| {
            collector.callback_provisional_center(store, local)
        })?;
        Ok(crate::callback::ProvisionalPoint { x, y })
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

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
use noon::integration::{
    scene_execution, AuthoringFailure, HostCallbackId, SemanticMutationTransaction, SemanticStore,
};
use noon::{
    ExecutionSegment, ExecutionSession, Mobject, MobjectTarget, Scene, SceneMembershipRequest,
};
use noon::{Transform2D, Vec2};
use pyo3::prelude::*;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct Store {
    pub(crate) semantics: Rc<RefCell<SemanticStore>>,
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
}
impl Context {
    fn new(scene: Scene) -> Self {
        Self {
            scene,
            bound_ids: HashSet::new(),
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
        let mut session = self
            .scene
            .execution_session()
            .map_err(|e| engine_error(e.to_string()))?;
        session.require_effect_resource_preparation();
        if self.retain_history {
            session
                .begin_replay_retention(Default::default())
                .map_err(|e| engine_error(e.to_string()))?;
        }
        Ok(session)
    }

    fn attach_session(&mut self, session: ExecutionSession) -> PyResult<()> {
        scene_execution::attach(&mut self.scene, session)
            .map_err(|error| engine_error(error.to_string()))
    }

    fn with_bootstrapped_live<T>(
        &mut self,
        operation: impl FnOnce(&mut noon::LiveSession<'_>) -> Result<T, noon::LiveSessionError>,
    ) -> PyResult<T> {
        if self.has_execution() {
            return self.with_live(operation);
        }
        let mut session = self.bootstrap()?;
        let result = operation(&mut self.scene.live(&mut session)).map_err(engine_error)?;
        self.attach_session(session)?;
        Ok(result)
    }

    pub(crate) fn prepare_viewer(&mut self) -> PyResult<noon::SceneExecution> {
        self.available()?;
        if !self.has_execution() {
            let session = self.bootstrap()?;
            self.attach_session(session)?;
        }
        let (_, _, session) = scene_execution::parts_mut(&mut self.scene)
            .expect("viewer bootstrap installed execution");
        if self.retain_history {
            session.seal_replay_for_viewer().map_err(|error| {
                engine_error(noon::integration::AuthoringFailure::new(
                    "unsupported_operation",
                    "python.preview_history",
                    error,
                ))
            })?;
        } else if !session.supports_immutable_playback() {
            return Err(engine_error(
                "prepare the source with retain_history=True before native preview",
            ));
        }
        // Transfer the exact arena/session pair at the authored frontier. Only
        // then select history; no source rerun, scene clone or relowering.
        let scene = std::mem::take(&mut self.scene);
        #[cfg(feature = "capture")]
        {
            self.effect_preparer = None;
        }
        self.terminal = true;
        let mut execution = scene
            .into_execution()
            .map_err(|e| engine_error(e.to_string()))?;
        execution
            .seek(0.0)
            .map_err(|e| engine_error(e.to_string()))?;
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
        let (store, root, session) = scene_execution::parts_mut(&mut self.scene)?;
        Some(noon::LiveSession::new(store, root, session))
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

    fn place<T>(
        &mut self,
        operation: impl FnOnce(&mut Scene) -> Result<T, noon::AuthoringError>,
    ) -> PyResult<()> {
        self.require_active()?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        operation(&mut self.scene)
            .map(|_| ())
            .map_err(noon::LiveSessionError::from)
            .map_err(engine_error)
    }

    fn edit_live_style(
        &mut self,
        edit: impl FnOnce(&mut Scene) -> Result<(), noon::AuthoringError>,
    ) -> PyResult<()> {
        self.require_active()?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        edit(&mut self.scene)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }

    fn edit_path(
        &mut self,
        target: &MobjectHandle,
        edit: impl FnOnce(&mut Scene, &noon::Mobject) -> Result<(), noon::AuthoringError>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        edit(&mut self.scene, &target.handle).map_err(engine_error)
    }
    fn publish(&mut self, transaction: SemanticMutationTransaction) -> PyResult<()> {
        self.require_active()?;
        if self.has_execution() {
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
    #[pyo3(name="liveSetStyle",signature=(source,fill_enabled,fill_red,fill_green,fill_blue,fill_alpha,fill_opacity,stroke_enabled,stroke_red,stroke_green,stroke_blue,stroke_alpha,stroke_width,stroke_opacity,background_stroke_enabled,background_stroke_red,background_stroke_green,background_stroke_blue,background_stroke_alpha,background_stroke_width,background_stroke_opacity))]
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
        self.edit_live_style(|scene| scene.set_style(&source.handle, update))
    }
    #[allow(clippy::too_many_arguments)]
    #[pyo3(name="liveSetFamilyStyle",signature=(source,fill_enabled,fill_red,fill_green,fill_blue,fill_alpha,fill_opacity,stroke_enabled,stroke_red,stroke_green,stroke_blue,stroke_alpha,stroke_width,stroke_opacity,background_stroke_enabled,background_stroke_red,background_stroke_green,background_stroke_blue,background_stroke_alpha,background_stroke_width,background_stroke_opacity))]
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
        self.with_live(|live| live.set_family_style(&source.family, update).map(|_| ()))
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
        if self.has_execution() {
            return Err(engine_error(
                "3D camera must be declared before execution starts",
            ));
        }
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
        if self.has_execution() {
            return Err(engine_error(
                "3D camera must be declared before execution starts",
            ));
        }
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
        if self.has_execution() {
            return Err(engine_error(
                "3D point lights must be created before execution starts",
            ));
        }
        let position = noon::integration::spatial_input::vec3(&position, "light position")
            .map_err(engine_error)?;
        let [r, g, b, a] = rgba.as_slice() else {
            return Err(engine_error("point light color requires RGBA components"));
        };
        let color =
            noon::integration::spatial_input::color(*r, *g, *b, *a).map_err(engine_error)?;
        self.scene
            .point_light_3d(position, color, intensity)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(engine_error)
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
        let value = if self.has_execution() {
            self.scene
                .effective_world_transform(&target.handle)
                .map_err(noon::LiveSessionError::from)
                .map_err(engine_error)
        } else {
            target.handle.world_transform().map_err(engine_error)
        }?;
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
            .map_err(|error| {
                if running {
                    engine_error(noon::LiveSessionError::from(error))
                } else {
                    engine_error(error)
                }
            })?;
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
    #[pyo3(name = "createValueTracker")]
    fn create_tracker(&mut self, initial: f64) -> PyResult<crate::signals::ValueTrackerHandle> {
        self.available()?;
        let value =
            if let Some((store, root, execution)) = scene_execution::parts_mut(&mut self.scene) {
                noon::integration::publish_value_tracker_creation(store, root, execution, initial)
            } else {
                self.scene.value_tracker(initial)
            };
        value
            .map(|value| crate::signals::ValueTrackerHandle { value })
            .map_err(engine_error)
    }
    #[pyo3(name = "associateValueTracker")]
    fn associate_tracker(&mut self, tracker: &crate::signals::ValueTrackerHandle) -> PyResult<()> {
        self.available()?;
        if let Some((store, root, execution)) = scene_execution::parts_mut(&mut self.scene) {
            noon::integration::publish_value_tracker_association(
                store,
                root,
                execution,
                &tracker.value,
            )
        } else {
            self.scene.associate_value_tracker(&tracker.value)
        }
        .map_err(engine_error)
    }
    #[pyo3(name = "valueTrackerValue")]
    fn tracker_value(&self, tracker: &crate::signals::ValueTrackerHandle) -> PyResult<f64> {
        self.require_active()?;
        if !tracker.value.is_in_store(self.scene.integration_store()) {
            return Err(engine_error(noon::AuthoringError::ForeignStore));
        }
        self.scene
            .integration_store()
            .borrow()
            .semantic_signal_state(tracker.value.node_id())
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)?;
        if let Some(execution) = scene_execution::session(&self.scene) {
            match execution
                .effective_signal_value(tracker.value.node_id())
                .ok_or_else(|| {
                    engine_error("ValueTracker is not lowered into this execution session")
                })? {
                noon_core::ReactiveValue::Scalar(value) => Ok(f64::from(*value)),
                _ => Err(engine_error("ValueTracker runtime signal is not scalar")),
            }
        } else {
            self.scene
                .value_tracker_value(&tracker.value)
                .map_err(engine_error)
        }
    }
    #[pyo3(name = "setValueTracker")]
    fn set_tracker(
        &mut self,
        tracker: &crate::signals::ValueTrackerHandle,
        value: f64,
    ) -> PyResult<()> {
        self.available()?;
        if self.has_execution() {
            self.with_live(|live| live.set_value(&tracker.value, value))
        } else {
            self.scene
                .set_value(&tracker.value, value)
                .map_err(engine_error)
        }
    }
    #[pyo3(name = "trackerPosition")]
    fn tracker_position(
        &self,
        tracker: &crate::signals::ValueTrackerHandle,
        dx: f64,
        dy: f64,
        x: f64,
        y: f64,
    ) -> PyResult<crate::signals::TrackerPositionHandle> {
        self.available()?;
        if self.has_execution() {
            return Err(engine_error(
                "native signal bindings must be authored before execution",
            ));
        }
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
        &self,
        target: &MobjectHandle,
        position: &crate::signals::TrackerPositionHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if self.has_execution() {
            return Err(engine_error(
                "native signal bindings must be authored before execution",
            ));
        }
        self.scene
            .bind_position(&target.handle, &position.value)
            .map_err(engine_error)
    }
    #[pyo3(name = "bindRotation")]
    fn bind_tracker_rotation(
        &self,
        target: &MobjectHandle,
        tracker: &crate::signals::ValueTrackerHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if self.has_execution() {
            return Err(engine_error(
                "native signal bindings must be authored before execution",
            ));
        }
        self.scene
            .bind_rotation(&target.handle, &tracker.value)
            .map_err(engine_error)
    }
    #[pyo3(name = "bindOpacity")]
    fn bind_tracker_opacity(
        &self,
        target: &MobjectHandle,
        tracker: &crate::signals::ValueTrackerHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if self.has_execution() {
            return Err(engine_error(
                "native signal bindings must be authored before execution",
            ));
        }
        self.scene
            .bind_opacity(&target.handle, &tracker.value)
            .map_err(engine_error)
    }

    #[pyo3(name = "liveNextLayoutTo")]
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
                noon::LiveLayoutTarget::Anchor(&target.anchor),
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
    #[pyo3(name = "liveNextLayoutToPoint")]
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
                noon::LiveLayoutTarget::Point(x, y),
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
    #[pyo3(name = "liveNextLayoutToEdgeInFrame")]
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
        self.with_live(|live| {
            live.next_layout_to_edge_in_frame(
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
    #[pyo3(name = "liveArrangeFamily")]
    fn arrange_family_with_options(
        &mut self,
        target: &crate::family::FamilyHandle,
        options: &crate::family::FamilyArrangeOptions,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| scene.arrange_family_with_options(&target.family, &options.options))
    }
    #[pyo3(name = "liveArrangeFamilyInGrid")]
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
    #[pyo3(name = "liveMoveFamilyToPoint")]
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
                noon::LiveLayoutTarget::Point(x, y),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "liveMoveFamilyToMobject")]
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
                noon::LiveLayoutTarget::Mobject(&other.handle),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "liveMoveFamilyToFamily")]
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
                noon::LiveLayoutTarget::Family(&other.family),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "liveAlignFamilyToPoint")]
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
            scene.align_family_to(
                &target.family,
                noon::LiveLayoutTarget::Point(x, y),
                (ex, ey),
            )
        })
    }
    #[pyo3(name = "liveAlignFamilyToMobject")]
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
                noon::LiveLayoutTarget::Mobject(&other.handle),
                (ex, ey),
            )
        })
    }
    #[pyo3(name = "liveAlignFamilyToFamily")]
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
                noon::LiveLayoutTarget::Family(&other.family),
                (ex, ey),
            )
        })
    }
    #[pyo3(name = "liveAlignFamilyOnFrame")]
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

    #[pyo3(name = "liveSetFamilyObjectOpacity")]
    fn family_object_opacity(
        &mut self,
        target: &crate::family::FamilyHandle,
        value: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| {
            live.set_family_object_opacity(&target.family, value)
                .map(|_| ())
        })
    }

    #[pyo3(name = "createCameraFrame")]
    fn camera_frame(&mut self, _id: &str) -> PyResult<MobjectHandle> {
        self.available()?;
        if self.has_execution() {
            return Err(engine_error("camera initialization must precede execution"));
        }
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
    #[pyo3(name = "liveSetEmission")]
    fn emission(&mut self, target: &MobjectHandle, value: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.edit_live_style(|scene| scene.set_emission(&target.handle, value))
    }
    #[pyo3(name = "liveSetFamilyEmission")]
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
        self.with_live(|live| {
            live.set_family_style(
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
    fn effective_number_plane_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::AxesObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        let plane = noon::ManimNumberPlane::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let value = if self.has_execution() {
            self.scene.effective_number_plane_frame(&plane)
        } else {
            plane.authored_frame()
        };
        value
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryPolarPlaneFrame")]
    fn effective_polar_plane_frame(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::PolarObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        let plane = noon::ManimPolarPlane::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let value = if self.has_execution() {
            self.scene.effective_polar_plane_frame(&plane)
        } else {
            plane.authored_polar_frame()
        };
        value
            .map(|frame| crate::coordinates::PolarObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }

    #[pyo3(name = "queryNumberLineFrame")]
    fn effective_number_line_frame(
        &mut self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::NumberLineObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        let coordinates = noon::ManimNumberLine::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let value = if self.has_execution() {
            self.scene.effective_number_line_frame(&coordinates)
        } else {
            coordinates.authored_frame()
        };
        value
            .map(|frame| crate::coordinates::NumberLineObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryAxesFrame")]
    fn effective_axes_frame(
        &mut self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::AxesObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        let coordinates = noon::ManimAxes::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let value = if self.has_execution() {
            self.scene.effective_axes_frame(&coordinates)
        } else {
            coordinates.authored_frame()
        };
        value
            .map(|frame| crate::coordinates::AxesObservation { frame })
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryThreeDAxesFrame")]
    fn effective_three_d_axes_frame(
        &mut self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<crate::coordinates::ThreeDAxesObservation> {
        self.require_active()?;
        self.same_family_store(target)?;
        let coordinates = noon::ManimThreeDAxes::from_family(target.family.clone())
            .map_err(noon::integration::coordinate_failure)
            .map_err(engine_error)?;
        let value = if self.has_execution() {
            self.scene
                .effective_three_d_axes_frame(&coordinates)
                .map_err(noon::integration::live_coordinate_failure)
        } else {
            coordinates
                .authored_frame()
                .map_err(noon::integration::coordinate_failure)
        };
        value
            .map(|frame| crate::coordinates::ThreeDAxesObservation { frame })
            .map_err(engine_error)
    }

    #[pyo3(name = "liveCreateCoordinates")]
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
        let execution = scene_execution::session(&self.scene).ok_or_else(|| {
            engine_error("published association requires the local completed execution player")
        })?;
        if execution.publication_context().scene_revision() != self.scene.revision() {
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

    #[pyo3(name = "liveStretchLayout")]
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
        self.with_live(|live| {
            live.stretch(
                &target.anchor,
                factor,
                dimension,
                crate::mobject::pivot_3d(x, y, z, point),
            )
            .map(|_| ())
        })
    }
    #[pyo3(name = "livePrependFamilyMembers")]
    fn prepend_family_members(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        if batch.kind != "add" {
            return Err(engine_error("member prepend requires an add batch"));
        }
        self.with_live(|live| live.prepend_family_members(&target.family, &batch.targets()))
    }
    #[pyo3(name = "livePrependObjectMembers")]
    fn prepend_handle_members(
        &mut self,
        target: &MobjectHandle,
        batch: &MembershipBatch,
    ) -> PyResult<()> {
        self.same_store(target)?;
        if batch.kind != "add" {
            return Err(engine_error("member prepend requires an add batch"));
        }
        self.with_live(|live| live.prepend_family_members(&target.handle, &batch.targets()))
    }

    #[pyo3(name = "queryMobjectVectorBoundaryPoints3D")]
    fn mobject_vector_boundary_points_3d(&self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_store(target)?;
        let target = noon::MobjectTarget::from(&target.handle);
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_vector_boundary_points_3d(
                self.scene.integration_store(),
                execution,
                target,
            )
        } else {
            target.vector_boundary_points_3d()
        };
        value
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryMobjectAllPoints3D")]
    fn mobject_all_points_3d(&self, target: &MobjectHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_store(target)?;
        let target = noon::MobjectTarget::from(&target.handle);
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_all_points_3d(
                self.scene.integration_store(),
                execution,
                target,
            )
        } else {
            target.all_points_3d()
        };
        value
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryFamilyVectorBoundaryPoints3D")]
    fn family_vector_boundary_points_3d(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_family_store(target)?;
        let target = noon::MobjectTarget::from(&target.family);
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_vector_boundary_points_3d(
                self.scene.integration_store(),
                execution,
                target,
            )
        } else {
            target.vector_boundary_points_3d()
        };
        value
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryFamilyAllPoints3D")]
    fn family_all_points_3d(&self, target: &crate::family::FamilyHandle) -> PyResult<Vec<f64>> {
        self.require_active()?;
        self.same_family_store(target)?;
        let target = noon::MobjectTarget::from(&target.family);
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_all_points_3d(
                self.scene.integration_store(),
                execution,
                target,
            )
        } else {
            target.all_points_3d()
        };
        value
            .map(crate::path_input::flat_points_3d)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryMobjectPointCount")]
    fn point_count(&mut self, target: &MobjectHandle) -> PyResult<usize> {
        self.require_active()?;
        self.same_store(target)?;
        if self.has_execution() {
            self.scene
                .effective_mobject_point_count(&target.handle)
                .map_err(noon::LiveSessionError::from)
                .map_err(engine_error)
        } else {
            target.handle.get_num_points().map_err(engine_error)
        }
    }

    #[pyo3(name = "queryMobjectPointArray")]
    fn object_point_array(&self, target: &MobjectHandle) -> PyResult<(Vec<f64>, Vec<usize>)> {
        self.require_active()?;
        self.same_store(target)?;
        let target = noon::MobjectTarget::from(&target.handle);
        let value = if self.has_execution() {
            self.scene.effective_point_array(target)
        } else {
            target.point_array()
        };
        value
            .map(crate::path_input::point_array)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryFamilyPointArray")]
    fn family_point_array(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<(Vec<f64>, Vec<usize>)> {
        self.require_active()?;
        self.same_family_store(target)?;
        let target = noon::MobjectTarget::from(&target.family);
        let value = if self.has_execution() {
            self.scene.effective_point_array(target)
        } else {
            target.point_array()
        };
        value
            .map(crate::path_input::point_array)
            .map_err(engine_error)
    }

    #[pyo3(name="liveSetFillImage",signature=(target,paint,opacity))]
    fn live_fill_image(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::ImagePaintOptions,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .set_fill_image(&target.handle, paint.options.clone(), opacity)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }

    #[pyo3(name="liveSetFillImageMapping",signature=(target,paint,opacity))]
    fn live_fill_image_mapping(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::ImageMapping,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .set_fill_image_mapping(&target.handle, paint.value, opacity)
            .map(|_| ())
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }

    #[pyo3(name="liveSetFillGradient", signature=(target,paint,opacity))]
    fn live_fill_gradient(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::LinearGradient,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_gradient(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name="liveSetFillRadialGradient", signature=(target,paint,opacity))]
    fn live_fill_radial_gradient(
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

    #[pyo3(name="liveSetFillNoise", signature=(target,paint,opacity))]
    fn live_fill_noise(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::NoisePaint,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_noise(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name="liveSetFillHatch", signature=(target,paint,opacity))]
    fn live_fill_hatch(
        &mut self,
        target: &MobjectHandle,
        paint: &crate::paints::HatchPaint,
        opacity: Option<f64>,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.place(|scene| scene.set_fill_hatch(&target.handle, paint.value.clone(), opacity))
    }

    #[pyo3(name = "liveSetGlow")]
    fn _set_glow(
        &mut self,
        target: &MobjectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.set_glow(&target.handle, update.value.clone()))
    }

    #[pyo3(name = "liveRemoveGlow")]
    fn _remove_glow(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.remove_glow(&target.handle))
    }

    #[pyo3(name = "liveAddEffect")]
    fn _add_effect(
        &mut self,
        target: &MobjectHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.add_effect(&target.handle, definition.definition(), name))
    }

    #[pyo3(name = "liveSetEffect")]
    fn _set_effect(
        &mut self,
        target: &MobjectHandle,
        name: &str,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.set_effect(&target.handle, name, update.value.clone()))
    }

    #[pyo3(name = "liveSetEffectHandle")]
    fn _set_effect_handle(
        &mut self,
        target: &MobjectHandle,
        handle: &crate::effects::EffectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.set_effect(&target.handle, &handle.handle, update.value.clone()))
    }

    #[pyo3(name = "liveRemoveEffect")]
    fn _remove_effect(&mut self, target: &MobjectHandle, name: &str) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.remove_effect(&target.handle, name))
    }

    #[pyo3(name = "liveRemoveEffectHandle")]
    fn _remove_effect_handle(
        &mut self,
        target: &MobjectHandle,
        handle: &crate::effects::EffectHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.remove_effect(&target.handle, &handle.handle))
    }

    #[pyo3(name = "liveSetFamilyGlow")]
    fn family_set_glow(
        &mut self,
        target: &crate::family::FamilyHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.set_glow(&target.family, update.value.clone()))
    }

    #[pyo3(name = "liveRemoveFamilyGlow")]
    fn family_remove_glow(&mut self, target: &crate::family::FamilyHandle) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.remove_glow(&target.family))
    }

    #[pyo3(name = "liveAddFamilyEffect")]
    fn family_add_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.add_effect(&target.family, definition.definition(), name))
    }

    #[pyo3(name = "liveSetFamilyEffect")]
    fn family_set_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        name: &str,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.set_effect(&target.family, name, update.value.clone()))
    }

    #[pyo3(name = "liveSetFamilyEffectHandle")]
    fn family_set_effect_handle(
        &mut self,
        target: &crate::family::FamilyHandle,
        handle: &crate::effects::EffectHandle,
        update: &crate::effects::EffectUpdate,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.set_effect(&target.family, &handle.handle, update.value.clone()))
    }

    #[pyo3(name = "liveRemoveFamilyEffect")]
    fn family_remove_effect(
        &mut self,
        target: &crate::family::FamilyHandle,
        name: &str,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.remove_effect(&target.family, name))
    }

    #[pyo3(name = "liveRemoveFamilyEffectHandle")]
    fn family_remove_effect_handle(
        &mut self,
        target: &crate::family::FamilyHandle,
        handle: &crate::effects::EffectHandle,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.remove_effect(&target.family, &handle.handle))
    }

    #[pyo3(name = "queryLayoutDimensionLength")]
    fn layout_dimension_length(&mut self, source: &LayoutAnchor, dimension: u32) -> PyResult<f64> {
        self.require_active()?;
        let dimension = dimension.try_into().map_err(engine_error)?;
        if self.has_execution() {
            self.with_live(|live| live.dimension_length(&source.anchor, dimension))
        } else {
            source
                .anchor
                .dimension_length(dimension)
                .map_err(engine_error)
        }
    }
    #[pyo3(name = "queryMobjectDimension")]
    fn mobject_dimension(&mut self, target: &MobjectHandle, dimension: u32) -> PyResult<f64> {
        self.same_store(target)?;
        self.layout_dimension_length(
            &LayoutAnchor::from_anchor(noon::LayoutAnchor::from(&target.handle), None),
            dimension,
        )
    }

    #[pyo3(name = "liveScaleLayout3d")]
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
        self.with_live(|live| {
            live.scale_layout_3d(
                &source.anchor,
                noon::SemanticVec3::new(sx, sy, sz),
                crate::mobject::pivot_3d(x, y, z, point),
            )
            .map(|_| ())
        })
    }

    #[pyo3(name = "queryMobjectRequiresContentBoundary")]
    fn mobject_requires_content_boundary(&self, target: &MobjectHandle) -> PyResult<bool> {
        self.require_active()?;
        self.same_store(target)?;
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_requires_content_boundary(
                self.scene.integration_store(),
                execution,
                noon::MobjectTarget::from(&target.handle),
            )
        } else {
            noon::MobjectTarget::from(&target.handle).requires_content_boundary()
        };
        value.map_err(engine_error)
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
        self.require_active()?;
        self.same_store(target)?;
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_critical_point_3d(
                self.scene.integration_store(),
                execution,
                noon::MobjectTarget::from(&target.handle),
                noon::SemanticVec3::new(x, y, z),
                vector_boundary,
            )
        } else {
            {
                let target = noon::MobjectTarget::from(&target.handle);
                let direction = noon::SemanticVec3::new(x, y, z);
                if vector_boundary {
                    target.vector_critical_point_3d(direction)
                } else {
                    target.critical_point_3d(direction)
                }
            }
        };
        value.map(|p| (p.x, p.y, p.z)).map_err(engine_error)
    }

    #[pyo3(name = "queryFamilyRequiresContentBoundary")]
    fn family_requires_content_boundary(
        &self,
        target: &crate::family::FamilyHandle,
    ) -> PyResult<bool> {
        self.require_active()?;
        self.same_family_store(target)?;
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_requires_content_boundary(
                self.scene.integration_store(),
                execution,
                noon::MobjectTarget::from(&target.family),
            )
        } else {
            noon::MobjectTarget::from(&target.family).requires_content_boundary()
        };
        value.map_err(engine_error)
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
        self.require_active()?;
        self.same_family_store(target)?;
        let value = if let Some(execution) = scene_execution::session(&self.scene) {
            noon::integration::effective_critical_point_3d(
                self.scene.integration_store(),
                execution,
                noon::MobjectTarget::from(&target.family),
                noon::SemanticVec3::new(x, y, z),
                vector_boundary,
            )
        } else {
            {
                let target = noon::MobjectTarget::from(&target.family);
                let direction = noon::SemanticVec3::new(x, y, z);
                if vector_boundary {
                    target.vector_critical_point_3d(direction)
                } else {
                    target.critical_point_3d(direction)
                }
            }
        };
        value.map(|p| (p.x, p.y, p.z)).map_err(engine_error)
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
        if self.has_execution() {
            self.with_live(|live| live.layout_critical_point_3d(&anchor.anchor, direction))
                .map(|p| (p.x, p.y, p.z))
        } else {
            anchor
                .anchor
                .critical_point_3d(direction)
                .map(|p| (p.x, p.y, p.z))
                .map_err(engine_error)
        }
    }

    fn prepare_view(&mut self) -> PyResult<crate::viewer::PreparedScene> {
        self.prepare_viewer().map(crate::viewer::PreparedScene::new)
    }
    #[pyo3(name = "requiresLiveMutationRouting")]
    fn requires_live_mutation_routing(&self) -> PyResult<bool> {
        self.require_active()?;
        Ok(self.has_execution())
    }
    #[cfg(feature = "native-text")]
    #[pyo3(name = "liveCreateManimText", signature = (source, family, size, spacing, r, g, b, a, opacity, colors=None, face=None))]
    #[allow(clippy::too_many_arguments)]
    fn live_text(
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
        crate::text::live(
            self, source, family, size, spacing, r, g, b, a, opacity, colors, face, false,
        )
    }
    #[cfg(feature = "native-text")]
    #[pyo3(name = "liveCreateManimMarkupText", signature = (source, family, size, spacing, r, g, b, a, opacity, face=None))]
    #[allow(clippy::too_many_arguments)]
    fn live_markup_text(
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
        crate::text::live(
            self, source, family, size, spacing, r, g, b, a, opacity, None, face, true,
        )
    }
    #[pyo3(name = "liveSetAngleValue")]
    fn set_angle_value(&mut self, target: &MobjectHandle, radians: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        if self.has_execution() {
            self.with_live(|live| live.set_angle_value(&target.handle, radians))
                .map(|_| ())
        } else {
            target
                .handle
                .clone()
                .set_angle_value(radians)
                .map_err(engine_error)
        }
    }
    #[cfg(feature = "typst")]
    #[pyo3(name = "liveCreateManimTypst")]
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
        crate::typst::live(self, source, math, size, red, green, blue, alpha, opacity)
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
        } else if let Some(mut live) = self.live_session() {
            match live.edit_membership(request) {
                Ok(_) => {}
                Err(noon::LiveSessionError::Publication(
                    noon::ExecutionSessionPublicationError::ExternalEffectPreparationRequired,
                )) => {
                    let pending = live.prepare_membership(request).map_err(engine_error)?;
                    return self.staged(pending, batch.bindings.clone()).map(Some);
                }
                Err(error) => return Err(engine_error(error)),
            }
        } else {
            self.scene.edit_membership(request).map_err(engine_error)?;
        }
        self.bound_ids
            .extend(batch.bindings.iter().map(|handle| handle.node_id()));
        Ok(None)
    }
    #[pyo3(name = "containsMobject")]
    fn contains(&mut self, target: &MobjectHandle) -> PyResult<bool> {
        self.require_active()?;
        self.same_store(target)?;
        if self.has_execution() {
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
        self.scene.add(&target.handle).map_err(engine_error)?;
        self.bound_ids.insert(target.handle.node_id());
        Ok(())
    }
    #[pyo3(name = "liveAdd")]
    fn live_add(&mut self, _id: &str, target: &MobjectHandle) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.add(&target.handle).map(|_| ()))?;
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
        let query = if self.has_execution() {
            self.scene.effective_path_query(&target.handle)
        } else {
            target.handle.path_query()
        };
        query
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
    #[pyo3(name = "queryMobjectColor")]
    fn mobject_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<crate::family::ColorObservation> {
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        let color = if self.has_execution() {
            self.with_live(|live| live.effective_manim_color(&target.handle))?
        } else {
            target.handle.manim_color().map_err(engine_error)?
        };
        Ok(color.into())
    }
    #[pyo3(name = "liveCreateFamily")]
    fn create_family(
        &mut self,
        batch: &MembershipBatch,
        z_index: f64,
        draw_enabled: bool,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.create_family_with_color(batch, z_index, 1.0, 1.0, 1.0, 1.0, draw_enabled)
    }
    #[pyo3(name = "liveCreateFamilyWithColor")]
    #[allow(clippy::too_many_arguments)]
    fn create_family_with_color(
        &mut self,
        batch: &MembershipBatch,
        z_index: f64,
        r: f64,
        g: f64,
        b: f64,
        a: f64,
        draw_enabled: bool,
    ) -> PyResult<crate::family::FamilyHandle> {
        self.available()?;
        if batch.kind != "add" {
            return Err(engine_error("family creation requires an add batch"));
        }
        let (store, root, execution) = scene_execution::parts_mut(&mut self.scene)
            .ok_or_else(|| engine_error("no execution session"))?;
        noon::integration::publish_family_creation_with_presentation(
            store,
            root,
            execution,
            &batch.targets(),
            z_index,
            noon::Color::rgba(r as f32, g as f32, b as f32, a as f32),
            draw_enabled,
        )
        .map(|family| crate::family::FamilyHandle { family })
        .map_err(engine_error)
    }
    #[pyo3(name = "liveCopyFamily")]
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
        self.with_live(|live| {
            live.copy_family_with_references(&target.family, &references.targets())
        })
        .map(|copied| crate::family::FamilyCopy { copied })
    }
    #[pyo3(name = "liveEditFamilyMembership")]
    fn edit_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        batch: &MembershipBatch,
    ) -> PyResult<Vec<bool>> {
        self.available()?;
        self.same_family_store(target)?;
        match batch.kind.as_str() {
            "add" => {
                self.with_live(|live| live.add_family_members(&target.family, &batch.targets()))
            }
            "remove" => {
                self.with_live(|live| live.remove_family_members(&target.family, &batch.targets()))
            }
            _ => Err(engine_error("unsupported family membership operation")),
        }
    }
    #[pyo3(name = "liveShiftFamily")]
    fn shift_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.place(|scene| scene.shift_family(&target.family, x, y))
    }
    #[pyo3(name = "liveScaleFamily")]
    fn scale_family(
        &mut self,
        target: &crate::family::FamilyHandle,
        x: f64,
        y: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.scale_family(&target.family, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveSetFamilyFill", signature = (target, has_color, red, green, blue, alpha, opacity=None))]
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
        self.with_live(|live| {
            live.set_family_fill(&target.family, color, opacity)
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveSetFamilyStroke", signature = (target, has_color, red, green, blue, alpha, width=None, opacity=None))]
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
        self.with_live(|live| {
            live.set_family_stroke(&target.family, color, width, opacity)
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveSetFamilyOpacity")]
    fn family_opacity(
        &mut self,
        target: &crate::family::FamilyHandle,
        opacity: f64,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        self.with_live(|live| live.set_family_opacity(&target.family, opacity).map(|_| ()))
    }
    #[pyo3(name = "liveSetFamilyColor")]
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
        self.with_live(|live| {
            live.set_family_color(&target.family, r, g, b, a)
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveSetFamilyOwnColor")]
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
        self.with_live(|live| {
            live.set_family_own_color(&target.family, r, g, b, a)
                .map(|_| ())
        })
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
        if self.has_execution() {
            self.with_live(|live| live.effective_family_layout(&target.family))
                .map(LayoutObservation::from_effective)
        } else {
            let layout = target.family.layout().map_err(engine_error)?;
            Ok(LayoutObservation {
                center: layout.center(),
                width: layout.width(),
                height: layout.height(),
            })
        }
    }
    #[pyo3(name = "queryMobjectLayout")]
    fn layout(&mut self, target: &MobjectHandle) -> PyResult<LayoutObservation> {
        self.require_active()?;
        self.same_store(target)?;
        if self.has_execution() {
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
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .geometry(options.options.clone())
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }
    #[pyo3(name = "liveCreateImage")]
    fn image(
        &mut self,
        mut options: PyRefMut<'_, crate::image::ImageOptions>,
    ) -> PyResult<MobjectHandle> {
        crate::image::live(self, &mut options)
    }
    #[pyo3(name = "liveSetImageSampling")]
    fn set_image_sampling(&mut self, target: &MobjectHandle, value: &str) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        let sampling = noon::integration::image_sampling(value).map_err(engine_error)?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .set_image_sampling(&target.handle, sampling)
            .map_err(|error| engine_error(AuthoringFailure::from_scene_mutation(error, true)))
    }
    #[pyo3(name = "liveChangeAnchorMode")]
    fn change_anchor_mode(&mut self, target: &MobjectHandle, smooth: bool) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.change_anchor_mode(object, smooth)
        })
    }
    #[pyo3(name = "liveChangeFamilyAnchorMode")]
    fn change_family_anchor_mode(
        &mut self,
        target: &crate::family::FamilyHandle,
        smooth: bool,
    ) -> PyResult<()> {
        self.available()?;
        self.same_family_store(target)?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
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
        self.same_family_store(target)?;
        let value = if self.has_execution() {
            self.scene.effective_family_own_path_query(&target.family)
        } else {
            target.family.own_path_query()
        };
        value
            .map(crate::path_query::PathQuery::new)
            .map_err(engine_error)
    }
    #[pyo3(name = "liveRescaleToFit")]
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
        self.with_live(|live| {
            live.rescale_to_fit_with_pivot(
                &target.anchor,
                length,
                dimension,
                stretch,
                crate::mobject::pivot_3d(x, y, z, point),
            )
        })
    }
    #[pyo3(name="liveStartNewPath", signature=(target ,x,y,z))]
    fn start_new_path(&mut self, target: &MobjectHandle, x: f64, y: f64, z: f64) -> PyResult<()> {
        let point = crate::path_input::point_3d(x, y, z)?;
        self.edit_path(target, |scene, object| {
            scene.start_new_path_3d(object, point)
        })
    }
    #[pyo3(name="liveStartNewPathWithStride", signature=(target ,x,y,z,stride))]
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
    #[pyo3(name="liveAddLineTo", signature=(target ,x,y,z,stride=None))]
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
    #[pyo3(name="liveAddLineToWithSamples", signature=(target ,x,y,z,stride,samples))]
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
    #[pyo3(name="liveSetPoints", signature=(target ,values))]
    fn set_points(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_f64(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_points_f64(object, &points)
        })
    }
    #[pyo3(name="liveSetPoints3D", signature=(target ,values))]
    fn set_points_3d(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| scene.set_points_3d(object, &points))
    }
    #[pyo3(name="liveAppendPoints", signature=(target ,values))]
    fn append_points(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_f64(&values)?;
        self.edit_path(target, |scene, object| {
            scene.append_points_f64(object, &points)
        })
    }
    #[pyo3(name="liveAppendPoints3D", signature=(target ,values))]
    fn append_points_3d(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.append_points_3d(object, &points)
        })
    }
    #[pyo3(name="liveSetPointsAsCorners", signature=(target ,values))]
    fn corners(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_points_as_corners_3d(object, &points)
        })
    }
    #[pyo3(name="liveAddPointsAsCorners", signature=(target ,values))]
    fn append_corners(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.add_points_as_corners_3d(object, &points)
        })
    }
    #[pyo3(name="liveAddCubicBezierCurves", signature=(target ,values))]
    fn cubics(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.add_cubic_bezier_curves_3d(object, &curves)
        })
    }
    #[pyo3(name="liveSetCubicBezierCurves", signature=(target ,values))]
    fn set_cubics(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let curves = crate::path_input::cubic_curves_3d(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_cubic_bezier_curves_3d(object, &curves)
        })
    }
    #[pyo3(name="liveSetPointsSmoothly", signature=(target ,values))]
    fn smooth_points(&mut self, target: &MobjectHandle, values: Vec<f64>) -> PyResult<()> {
        let points = crate::path_input::points(&values)?;
        self.edit_path(target, |scene, object| {
            scene.set_points_smoothly(object, &points)
        })
    }
    #[pyo3(name="liveSetPointsAsCornersWithSamples", signature=(target ,values,samples))]
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
    #[pyo3(name="liveAddPointsAsCornersWithStride", signature=(target ,values,stride,samples))]
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
    #[pyo3(name="liveClearPoints", signature=(target))]
    fn clear_points(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.clear_points(object))
    }
    #[pyo3(name="liveClosePath", signature=(target))]
    fn close_path(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.close_path(object))
    }
    #[pyo3(name="liveReverseDirection", signature=(target))]
    fn reverse_direction(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.reverse_direction(object))
    }
    #[pyo3(name="liveReversePoints", signature=(target))]
    fn reverse_points(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.reverse_points(object))
    }
    #[pyo3(name="liveMakeSmooth", signature=(target))]
    fn make_smooth(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.change_anchor_mode(object, true)
        })
    }
    #[pyo3(name="liveMakeJagged", signature=(target))]
    fn make_jagged(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.change_anchor_mode(object, false)
        })
    }
    #[pyo3(name="liveRepeatPoints", signature=(target ,count))]
    fn repeat_points(&mut self, target: &MobjectHandle, count: usize) -> PyResult<()> {
        self.edit_path(target, |scene, object| scene.repeat_points(object, count))
    }
    #[pyo3(name="liveInsertNCurves", signature=(target ,additional))]
    fn insert_curves(&mut self, target: &MobjectHandle, additional: usize) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.insert_n_curves(object, additional)
        })
    }
    #[pyo3(name="liveScaleHandleToAnchorDistances", signature=(target ,factor))]
    fn scale_handles(&mut self, target: &MobjectHandle, factor: f64) -> PyResult<()> {
        self.edit_path(target, |scene, object| {
            scene.scale_handle_to_anchor_distances(object, factor)
        })
    }
    #[pyo3(name="liveClosePathWithStride", signature=(target ,stride,tolerance,samples))]
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
    #[pyo3(name="livePointwiseBecomePartial", signature=(target ,source,a,b))]
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
    #[pyo3(name="liveAppendVectorizedMobject", signature=(target ,source))]
    fn append_vector(&mut self, target: &MobjectHandle, source: &MobjectHandle) -> PyResult<()> {
        self.same_store(source)?;
        self.edit_path(target, |scene, object| {
            scene.append_vectorized_mobject(object, &source.handle)
        })
    }
    #[pyo3(name="liveAppendVectorizedMobjectWithStride", signature=(target ,source,stride))]
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
    #[pyo3(name="liveAddQuadraticBezierCurveTo", signature=(target ,control_x,control_y,control_z,anchor_x,anchor_y,anchor_z,stride=None))]
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
    #[pyo3(name="liveAddCubicBezierCurveTo", signature=(target ,control1_x,control1_y,control1_z,control2_x,control2_y,control2_z,anchor_x,anchor_y,anchor_z,stride=None))]
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
    #[pyo3(name="liveAddCubicBezierCurve", signature=(target ,anchor1_x,anchor1_y,anchor1_z,handle1_x,handle1_y,handle1_z,handle2_x,handle2_y,handle2_z,anchor2_x,anchor2_y,anchor2_z))]
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
    #[pyo3(name="liveAddSmoothCurveTo", signature=(target ,values,stride=None))]
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
                ))
            }
        };
        self.edit_path(target, |scene, object| {
            scene.add_smooth_curve_to_with_stride(object, handle2, anchor, stride.unwrap_or(4))
        })
    }
    #[pyo3(name = "liveSetFamilyPoints3D")]
    fn set_family_points_3d(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let values = crate::path_input::points_3d(&values)?;
        self.place(|scene| scene.set_family_points_3d(&target.family, &values))
    }
    #[pyo3(name = "liveAppendFamilyPoints3D")]
    fn append_family_points_3d(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let values = crate::path_input::points_3d(&values)?;
        self.place(|scene| scene.append_family_points_3d(&target.family, &values))
    }
    #[pyo3(name = "liveClearFamilyPoints")]
    fn clear_family_points(&mut self, target: &crate::family::FamilyHandle) -> PyResult<()> {
        self.same_family_store(target)?;
        self.place(|scene| scene.clear_family_points(&target.family))
    }
    #[pyo3(name = "liveSetFamilyEmptyPointArray")]
    fn empty_family_array(
        &mut self,
        target: &crate::family::FamilyHandle,
        values: Vec<f64>,
    ) -> PyResult<()> {
        self.same_family_store(target)?;
        let shape = crate::path_input::point_array_shape(&values)?;
        self.place(|scene| scene.set_empty_point_array((&target.family).into(), shape))
    }
    #[pyo3(name = "liveSetEmptyPointArray")]
    fn empty_point_array(&mut self, target: &MobjectHandle, dimensions: Vec<f64>) -> PyResult<()> {
        self.same_store(target)?;
        let shape = crate::path_input::point_array_shape(&dimensions)?;
        self.place(|scene| scene.set_empty_point_array((&target.handle).into(), shape))
    }
    #[pyo3(name = "liveSubcurve")]
    fn subcurve(&mut self, target: &MobjectHandle, a: f64, b: f64) -> PyResult<MobjectHandle> {
        self.same_store(target)?;
        self.require_active()?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .subcurve(&target.handle, a, b)
            .map(MobjectHandle::from_semantic_mobject)
            .map_err(noon::LiveSessionError::from)
            .map_err(engine_error)
    }
    #[pyo3(name = "liveAlignRootPoints")]
    fn align_root_points(&mut self, left: &LayoutAnchor, right: &LayoutAnchor) -> PyResult<()> {
        self.place(|scene| scene.align_path_points(&left.anchor, &right.anchor))
    }
    #[pyo3(name = "liveAlignPoints")]
    fn align_points(&mut self, left: &MobjectHandle, right: &MobjectHandle) -> PyResult<()> {
        self.same_store(left)?;
        self.same_store(right)?;
        self.place(|scene| scene.align_points(&left.handle, &right.handle))
    }
    #[pyo3(name = "liveApplyMatrix")]
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
    #[pyo3(name = "liveShift")]
    fn shift(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.available()?;
        self.with_live(|live| live.shift(&target.handle, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveScale")]
    fn scale_mobject(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.available()?;
        self.same_store(target)?;
        self.with_live(|live| live.manim_scale(&target.handle, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveSetFill")]
    fn fill(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_fill_rgb(&target.handle, r, g, b, a))
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
        self.edit_live_style(|scene| scene.set_fill_color(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "liveSetFillOpacity")]
    fn fill_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_fill_opacity(&target.handle, a))
    }
    #[pyo3(name = "liveSetColor")]
    fn color(&mut self, target: &MobjectHandle, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_color_rgba(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "liveSetOpacity")]
    fn opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_opacity(&target.handle, a))
    }
    #[pyo3(name = "liveSetObjectOpacity")]
    fn object_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_object_opacity(&target.handle, a))
    }
    #[pyo3(name = "liveZIndex")]
    fn z_index(&self, target: &LayoutAnchor) -> PyResult<f64> {
        self.require_active()?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .effective_z_index(&target.anchor)
            .map_err(engine_error)
    }
    #[pyo3(name = "liveSetZIndex")]
    fn set_z_index(&mut self, target: &LayoutAnchor, value: f64, family: bool) -> PyResult<()> {
        self.available()?;
        if !self.has_execution() {
            return Err(engine_error("no execution session"));
        }
        self.scene
            .set_z_index(&target.anchor, value, family)
            .map_err(engine_error)
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
    #[pyo3(name = "prepareEffectAddition")]
    fn prepare_effect_addition(
        &mut self,
        target: &MobjectHandle,
        definition: &crate::effects::EffectDefinition,
        name: &str,
    ) -> PyResult<crate::effects::StagedEffectPublication> {
        self.available()?;
        self.same_store(target)?;
        let pending = self.with_live(|live| {
            live.prepare_effect_attachments([(&target.handle, definition.definition(), name)])
        })?;
        self.staged(pending, Vec::new())
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
        let pending = self.with_live(|live| {
            live.prepare_effect_attachments([(&target.family, definition.definition(), name)])
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
                return Err(engine_error(noon::integration::AuthoringFailure::new("unsupported_operation", "shader.device_preparation_required", "live shader publication requires native GPU preparation; enable prepare_shaders")));
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
        &self,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
    ) -> PyResult<crate::composition::DeclaredAnimation> {
        self.available()?;
        self.same_store(source)?;
        self.same_store(target)?;
        if self.has_execution() {
            return Err(engine_error(
                "declare live animations before beginning execution",
            ));
        }
        self.scene
            .declare_transform_to(
                &source.handle,
                &target.handle,
                noon::AnimationOptions::new()
                    .run_time(duration)
                    .rate_func(crate::composition::rate(easing)?),
            )
            .map(|value| crate::composition::DeclaredAnimation { value })
            .map_err(engine_error)
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
        let (store, _, session) = scene_execution::parts_mut(&mut self.scene)
            .ok_or_else(|| engine_error("no execution session"))?;
        if time < session.frame().time {
            session
                .seek_with_store(&mut store.borrow_mut(), time)
                .map_err(|e| engine_error(e.to_string()))?;
        } else {
            session
                .advance_to(time)
                .map_err(|e| engine_error(e.to_string()))?;
        }
        Ok(())
    }
    #[pyo3(name = "liveEffectiveMobject")]
    fn effective_mobject(&mut self, target: &MobjectHandle) -> PyResult<CallbackTransform> {
        self.same_store(target)?;
        self.with_live(|live| live.effective(&target.handle))
            .map(|value| CallbackTransform {
                transform: value.transform,
            })
    }
    #[pyo3(name = "liveSetTranslation")]
    fn set_translation(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.with_live(|live| live.set_translation(&target.handle, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveSetScale")]
    fn set_scale(&mut self, target: &MobjectHandle, x: f64, y: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.with_live(|live| live.set_scale(&target.handle, x, y).map(|_| ()))
    }
    #[pyo3(name = "liveSetRotation")]
    fn set_rotation(&mut self, target: &MobjectHandle, angle: f64) -> PyResult<()> {
        self.same_store(target)?;
        self.with_live(|live| live.set_rotation(&target.handle, angle).map(|_| ()))
    }
    #[pyo3(name = "liveReplaceContent")]
    fn replace_content(&mut self, target: &MobjectHandle, source: &MobjectHandle) -> PyResult<()> {
        self.same_store(target)?;
        self.same_store(source)?;
        self.with_live(|live| {
            live.replace_content(&target.handle, &source.handle)
                .map(|_| ())
        })
    }
    #[pyo3(name = "liveRemove")]
    fn live_remove(&mut self, target: &MobjectHandle) -> PyResult<()> {
        self.same_store(target)?;
        self.with_live(|live| live.remove(&target.handle).map(|_| ()))
    }
    #[pyo3(name = "liveBecomeMobject")]
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
        self.with_live(|live| {
            live.become_mobject(&target.handle, &source.handle, options)
                .map(|_| ())
        })
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
        if let Some((_, _, session)) = scene_execution::parts_mut(&mut self.scene) {
            if let Some(token) = session.pending_callback_token() {
                session
                    .interrupt_required_callback_phase(token)
                    .map_err(engine_error)?;
            }
        }
        scene_execution::retire(&mut self.scene).map_err(|e| engine_error(e.to_string()))?;
        #[cfg(feature = "capture")]
        {
            self.effect_preparer = None;
        }
        self.terminal = true;
        self.segment = None;
        self.pending_ack = None;
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
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        let value = if self.has_execution() {
            self.with_live(|live| live.effective_line_endpoints(&target.handle))?
        } else {
            target.handle.manim_line_endpoints().map_err(engine_error)?
        };
        Ok(crate::mobject::LineEndpoints { value })
    }

    #[pyo3(name = "queryMobjectFillColor")]
    fn query_fill_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<Option<crate::family::ColorObservation>> {
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        let color = if self.has_execution() {
            self.with_live(|live| live.effective_fill_color(&target.handle))?
        } else {
            target.handle.fill_color().map_err(engine_error)?
        };
        Ok(color.map(Into::into))
    }
    #[pyo3(name = "queryMobjectStrokeColor")]
    fn query_stroke_color(
        &mut self,
        target: &MobjectHandle,
    ) -> PyResult<Option<crate::family::ColorObservation>> {
        self.require_active()?;
        self.same_store(target)?;
        if !self.bound_ids.contains(&target.handle.node_id()) {
            return Err(engine_error("mobject is not bound to this canonical Scene"));
        }
        let color = if self.has_execution() {
            self.with_live(|live| live.effective_stroke_color(&target.handle))?
        } else {
            target.handle.stroke_color().map_err(engine_error)?
        };
        Ok(color.map(Into::into))
    }
    #[pyo3(name = "queryMobjectFillOpacity")]
    fn query_fill_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
            self.with_live(|live| live.effective(&target.handle))
                .map(|v| v.fill_opacity())
        } else {
            target.handle.fill_opacity().map_err(engine_error)
        }
    }
    #[pyo3(name = "queryMobjectStrokeOpacity")]
    fn query_stroke_opacity(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
            self.with_live(|live| live.effective(&target.handle))
                .map(|v| v.stroke_opacity())
        } else {
            target.handle.stroke_opacity().map_err(engine_error)
        }
    }
    #[pyo3(name = "queryMobjectStrokeWidth")]
    fn query_stroke_width(&mut self, target: &MobjectHandle) -> PyResult<f64> {
        self.same_store(target)?;
        if self.has_execution() {
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
        self.place(|scene| {
            scene.move_to(
                &target.handle,
                noon::LiveLayoutTarget::Point(x, y),
                (ex, ey),
                (mx, my),
            )
        })
    }
    #[pyo3(name = "liveMoveToMobject")]
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
                noon::LiveLayoutTarget::Mobject(&reference.handle),
                (ex, ey),
                (mx, my),
            )
        })
    }
    fn runtime_identity(&self) -> PyResult<Option<u64>> {
        self.require_active()?;
        Ok(scene_execution::session(&self.scene).map(|s| s.runtime_identity().get()))
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
        self.edit_live_style(|scene| scene.set_stroke(&target.handle, r, g, b, a))
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
        self.edit_live_style(|scene| scene.set_stroke_color(&target.handle, r, g, b, a))
    }
    #[pyo3(name = "liveSetStrokeOpacity")]
    fn set_stroke_opacity(&mut self, target: &MobjectHandle, a: f64) -> PyResult<()> {
        self.available()?;
        self.edit_live_style(|scene| scene.set_stroke_opacity(&target.handle, a))
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

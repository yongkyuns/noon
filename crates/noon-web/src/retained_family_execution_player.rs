use noon::{LoweredRetainedFamilyAnimation, RetainedScene};
use noon_compile::{RetainedCompileError, RetainedCompiledScene};
use noon_core::{
    Camera2DState, FamilyAnimationSpec, ObjectId, RetainedFamilyAnimationPlan, TrackDefinition,
};
use noon_runtime::{
    RetainedFamilyFrame, RetainedFamilyPlanRuntimeError, RetainedFamilyPlanSceneInstance,
    RetainedFrameState,
};

use crate::{
    RetainedFamilyExecutionDeltaEncoder, RetainedFamilyExecutionDeltaEnvelope,
    RetainedFamilyExecutionEncodeError, RetainedResourceBundle, RetainedResourceTransportError,
};

/// Production owner for one prepared retained semantic-family animation.
///
/// Authoring and semantic lowering end before this boundary. The player receives the
/// authoritative retained scene plus one prepared family plan/spec, compiles the
/// ordinary retained timeline once, captures renderer resources once, and then drives
/// the shared family runtime directly into the family-aware retained transport.
/// Object identity, resource identity, camera state, and transport sequencing therefore
/// stay on the same path as ordinary retained playback.
#[derive(Clone, Debug)]
pub struct RetainedFamilyExecutionPlayer {
    scene: RetainedScene,
    runtime: RetainedFamilyPlanSceneInstance,
    encoder: RetainedFamilyExecutionDeltaEncoder,
    resource_bundle: Vec<u8>,
    camera_object: Option<ObjectId>,
    snapshot_sent: bool,
}

impl RetainedFamilyExecutionPlayer {
    pub fn new(
        scene: RetainedScene,
        plan: RetainedFamilyAnimationPlan,
        spec: FamilyAnimationSpec,
        camera_object: Option<ObjectId>,
        session: u32,
    ) -> Result<Self, RetainedFamilyExecutionPlayerError> {
        let tracks = scene.tracks().to_vec();
        Self::new_with_tracks(scene, &tracks, plan, spec, camera_object, session)
    }

    /// Construct from a retained materialization whose validated timeline is carried
    /// separately from its resource/object arena.
    ///
    /// Canonical mixed SceneSpec lowering currently materializes geometry first and
    /// therefore cannot install text-targeting tracks into that temporary seed scene.
    /// The canonical handoff nevertheless validates one exact track set after all
    /// objects exist. Accept it here explicitly so family-aware execution cannot drop
    /// ordinary transform/style/lifecycle animation while switching schedulers.
    pub fn new_with_tracks(
        scene: RetainedScene,
        tracks: &[TrackDefinition],
        plan: RetainedFamilyAnimationPlan,
        spec: FamilyAnimationSpec,
        camera_object: Option<ObjectId>,
        session: u32,
    ) -> Result<Self, RetainedFamilyExecutionPlayerError> {
        let compiled = RetainedCompiledScene::compile(scene.objects(), tracks)?;
        let bundle = RetainedResourceBundle::capture(
            scene
                .objects()
                .iter()
                .filter_map(|object| object.content.text()),
            scene.texts(),
            scene.geometries(),
            scene.fonts(),
        )?;
        let resource_bundle = bundle.encode_binary()?;
        let runtime = RetainedFamilyPlanSceneInstance::new(compiled, plan, spec)?;
        Ok(Self {
            scene,
            runtime,
            encoder: RetainedFamilyExecutionDeltaEncoder::new(session),
            resource_bundle,
            camera_object,
            snapshot_sent: false,
        })
    }

    /// Enter production execution directly from the Rust-owned authoring lowering result.
    pub fn from_lowered(
        scene: RetainedScene,
        lowered: LoweredRetainedFamilyAnimation,
        camera_object: Option<ObjectId>,
        session: u32,
    ) -> Result<Self, RetainedFamilyExecutionPlayerError> {
        let (plan, spec) = lowered.into_parts();
        Self::new(scene, plan, spec, camera_object, session)
    }

    pub const fn scene(&self) -> &RetainedScene {
        &self.scene
    }

    pub fn frame(&self) -> &RetainedFrameState {
        self.runtime.inner().frame()
    }

    pub fn family_frame(&self) -> RetainedFamilyFrame<'_> {
        self.runtime.frame()
    }

    pub fn plan(&self) -> &RetainedFamilyAnimationPlan {
        self.runtime.plan()
    }

    pub const fn spec(&self) -> FamilyAnimationSpec {
        self.runtime.spec()
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.camera_object
    }

    pub fn resource_bundle_bytes(&self) -> &[u8] {
        &self.resource_bundle
    }

    /// Evaluate one absolute scene time and encode the renderer-facing family delta.
    ///
    /// The first call is always an authoritative snapshot carrying the immutable plan.
    /// Later calls are sparse: ordinary retained dirtiness and family-state changes are
    /// merged by the shared runtime before the shared retained encoder assigns sequence.
    /// A backward seek naturally promotes to a complete snapshot through `FrameChanges`.
    pub fn evaluate_delta(
        &mut self,
        time: f64,
    ) -> Result<Option<RetainedFamilyExecutionDeltaEnvelope>, RetainedFamilyExecutionPlayerError>
    {
        self.runtime.evaluate(time)?;
        let camera = self.camera_state()?;
        let changes = self.runtime.take_frame_changes();
        let frame = self.runtime.frame();
        let plans = std::slice::from_ref(self.runtime.plan());

        if !self.snapshot_sent {
            let delta = self.encoder.encode_snapshot(&frame, plans, camera)?;
            self.snapshot_sent = true;
            return Ok(Some(delta));
        }

        Ok(self
            .encoder
            .encode_incremental(&frame, plans, &changes, camera)?)
    }

    fn camera_state(&self) -> Result<Camera2DState, RetainedFamilyExecutionPlayerError> {
        let Some(camera_object) = self.camera_object else {
            return Ok(Camera2DState::default());
        };
        let object = self
            .runtime
            .inner()
            .frame()
            .objects
            .iter()
            .find(|object| object.id == camera_object)
            .ok_or(RetainedFamilyExecutionPlayerError::InvalidCameraObject(
                camera_object,
            ))?;
        let geometry =
            object
                .geometry()
                .ok_or(RetainedFamilyExecutionPlayerError::InvalidCameraObject(
                    camera_object,
                ))?;
        Camera2DState::from_frame_object(geometry, object.transform).ok_or(
            RetainedFamilyExecutionPlayerError::InvalidCameraObject(camera_object),
        )
    }
}

#[derive(Debug)]
pub enum RetainedFamilyExecutionPlayerError {
    Compile(RetainedCompileError),
    Resource(RetainedResourceTransportError),
    Runtime(RetainedFamilyPlanRuntimeError),
    Transport(RetainedFamilyExecutionEncodeError),
    InvalidCameraObject(ObjectId),
}

impl std::fmt::Display for RetainedFamilyExecutionPlayerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compile(error) => error.fmt(formatter),
            Self::Resource(error) => error.fmt(formatter),
            Self::Runtime(error) => error.fmt(formatter),
            Self::Transport(error) => error.fmt(formatter),
            Self::InvalidCameraObject(object) => write!(
                formatter,
                "retained family camera object {} is missing or not a supported 2D frame",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyExecutionPlayerError {}

impl From<RetainedCompileError> for RetainedFamilyExecutionPlayerError {
    fn from(value: RetainedCompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<RetainedResourceTransportError> for RetainedFamilyExecutionPlayerError {
    fn from(value: RetainedResourceTransportError) -> Self {
        Self::Resource(value)
    }
}

impl From<RetainedFamilyPlanRuntimeError> for RetainedFamilyExecutionPlayerError {
    fn from(value: RetainedFamilyPlanRuntimeError) -> Self {
        Self::Runtime(value)
    }
}

impl From<RetainedFamilyExecutionEncodeError> for RetainedFamilyExecutionPlayerError {
    fn from(value: RetainedFamilyExecutionEncodeError) -> Self {
        Self::Transport(value)
    }
}

#[cfg(test)]
mod tests {
    use noon::{RetainedFamilyAnimationLoweringSession, RetainedScene, Text};
    use noon_core::{
        FamilyAnimationMode, GeometryRef, ObjectId, RateFunction, SceneDefinition, SemanticStore,
    };

    use crate::{InstalledRetainedExecutionMirror, RetainedTransportApplyOutcome};

    use super::*;

    #[test]
    fn mixed_text_geometry_family_survives_runtime_wire_and_installed_mirror() {
        let mut legacy = SceneDefinition::new();
        let circle_id = legacy.add(GeometryRef::circle(1.0));
        let text_id = ObjectId::new(1_u64 << 52);
        let mut scene = RetainedScene::from_legacy(&legacy).unwrap();
        scene
            .insert_native_text_at(0, text_id, Text::new("AB"))
            .unwrap();
        assert_eq!(scene.objects()[0].id, text_id);
        assert_eq!(scene.objects()[1].id, circle_id);

        let mut semantics = SemanticStore::new();
        let text_leaf = semantics.insert_authoring_object();
        let circle_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, text_leaf).unwrap();
        semantics.add_member(family, circle_leaf).unwrap();
        let spec = FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            1.0,
            2.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        let mut lowering =
            RetainedFamilyAnimationLoweringSession::begin(&semantics, family, spec).unwrap();
        // Retained materialization order must never become semantic animation order.
        lowering.bind_leaf(circle_leaf, circle_id).unwrap();
        lowering.bind_leaf(text_leaf, text_id).unwrap();
        let lowered = lowering.finish(&scene).unwrap();

        let mut player =
            RetainedFamilyExecutionPlayer::from_lowered(scene, lowered, None, 41).unwrap();
        let mut mirror =
            InstalledRetainedExecutionMirror::from_bundle_bytes(player.resource_bundle_bytes())
                .unwrap();

        let initial = player.evaluate_delta(0.0).unwrap().unwrap();
        assert!(initial.retained.snapshot);
        assert_eq!(initial.retained.sequence, 0);
        assert_eq!(initial.family_plans.len(), 1);
        let initial_json = serde_json::to_string(&initial).unwrap();
        assert!(!initial_json.contains("glyph"));
        let (outcome, changes) = mirror.apply_json(&initial_json).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert!(changes.is_all());

        let midpoint = player.evaluate_delta(2.0).unwrap().unwrap();
        assert!(!midpoint.retained.snapshot);
        assert_eq!(midpoint.retained.sequence, 1);
        assert!(midpoint.family_plans.is_empty());
        assert_eq!(midpoint.family_states.len(), 2);
        let midpoint_json = serde_json::to_string(&midpoint).unwrap();
        assert!(!midpoint_json.contains("glyph"));
        let (outcome, changes) = mirror.apply_json(&midpoint_json).unwrap();
        assert_eq!(outcome, RetainedTransportApplyOutcome::Applied);
        assert_eq!(changes.object_indices(), &[0, 1]);

        let installed_plan = mirror.family_plan().unwrap().unwrap();
        assert_eq!(installed_plan.leaves()[0].span().object, text_id);
        assert_eq!(installed_plan.leaves()[1].span().object, circle_id);
        let installed_frame = mirror.family_frame().unwrap().unwrap();
        let text = installed_frame
            .planned_family_leaf(installed_plan, 0)
            .unwrap()
            .unwrap();
        let circle = installed_frame
            .planned_family_leaf(installed_plan, 1)
            .unwrap()
            .unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }
}

use std::collections::BTreeMap;
#[cfg(any(target_arch = "wasm32", test))]
use std::collections::BTreeSet;

use noon_core::{
    Color, FamilyAnimationRequest, ObjectId, ObjectSnapshot, SemanticObjectState, SemanticPaint,
    SemanticStyle, SemanticTransform2_5D, Style, TextSourceKind, TrackDefinition, Transform2D,
    Vec2,
};
#[cfg(any(target_arch = "wasm32", test))]
use noon_core::{HostCallbackId, SemanticFadeDirection, SemanticMutationTransaction, SemanticVec3};
use noon_ir::{ObjectSpec, SceneSpec, TextSpec};
#[cfg(target_arch = "wasm32")]
use noon_ir::{ObjectSpecContent, TextSpecKind, TextSpecOptions};

use crate::{
    materialize_retained_tracks, RetainedTextAuthoringSpec, RetainedTextBackendSpec,
    RetainedTrackAuthoringSpec,
};

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone)]
enum OrdinaryCompositionChild {
    TransformTo {
        entering_id: Option<ObjectId>,
        source: noon::Mobject,
        target: noon::Mobject,
        interpolation: noon_core::SemanticTransformInterpolation,
        options: noon_core::AnimationOptions,
    },
    Rotate {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        angle: f64,
        options: noon_core::AnimationOptions,
    },
}

/// One scene family in the worker's shared semantic store.
/// Geometry bindings retain identity only. Source-level text remains a deletion-owned
/// export adapter (#959); it cannot enter geometry-only typed execution silently.
pub struct CanonicalAuthoringScene {
    scene: noon::Scene,
    bindings: BTreeMap<ObjectId, noon_core::SemanticNodeId>,
    identities: BTreeMap<noon_core::SemanticNodeId, ObjectId>,
    text_adapters: BTreeMap<noon_core::SemanticNodeId, ObjectSpec>,
    retained_scale_factors: BTreeMap<ObjectId, Vec2>,
    #[cfg(any(target_arch = "wasm32", test))]
    live_player: Option<crate::SemanticExecutionPlayer>,
    #[cfg(any(target_arch = "wasm32", test))]
    live_player_transferred: bool,
    /// Whether the locally held player was returned by a presentation lease.
    /// Only that dormant runtime may be superseded by later direct authoring.
    #[cfg(any(target_arch = "wasm32", test))]
    live_player_returned: bool,
}

impl Default for CanonicalAuthoringScene {
    fn default() -> Self {
        Self::with_store(std::rc::Rc::new(std::cell::RefCell::new(
            noon_core::SemanticStore::new(),
        )))
    }
}

impl CanonicalAuthoringScene {
    pub fn with_store(
        semantics: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    ) -> Self {
        let scene = noon::Scene::with_store(semantics);
        Self {
            scene,
            bindings: BTreeMap::new(),
            identities: BTreeMap::new(),
            text_adapters: BTreeMap::new(),
            retained_scale_factors: BTreeMap::new(),
            #[cfg(any(target_arch = "wasm32", test))]
            live_player: None,
            #[cfg(any(target_arch = "wasm32", test))]
            live_player_transferred: false,
            #[cfg(any(target_arch = "wasm32", test))]
            live_player_returned: false,
        }
    }

    pub fn bind_mobject(&mut self, id: ObjectId, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        self.bind_node(id, handle.node_id())
    }

    /// Create and bind this scene's camera frame through the shared semantic transaction.
    ///
    /// The returned handle is only an alias of the scene-owned semantic identity. It carries no
    /// camera state or frontend allocation authority.
    pub fn create_camera_frame(&mut self, id: ObjectId) -> Result<noon::Mobject, String> {
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let frame = self.scene.camera_frame()?;
        let node = frame.node_id();
        debug_assert!(!self.identities.contains_key(&node));
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(frame)
    }

    fn bind_node(&mut self, id: ObjectId, node: noon_core::SemanticNodeId) -> Result<(), String> {
        if self.bindings.contains_key(&id) || self.identities.contains_key(&node) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        transaction.add_member(self.scene.root(), node);
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(())
    }

    /// Snapshot import is an explicit compatibility boundary, never the typed bind path.
    pub fn bind_geometry(&mut self, id: ObjectId, snapshot: ObjectSnapshot) -> Result<(), String> {
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(self.scene.store()),
            snapshot,
        )?;
        self.bind_mobject(id, &handle)
    }

    pub fn update_geometry(
        &mut self,
        id: ObjectId,
        snapshot: ObjectSnapshot,
    ) -> Result<(), String> {
        let node = self.node(id)?;
        if self.text_adapters.contains_key(&node) {
            return Err(format!(
                "canonical object {} is not geometry-backed",
                id.get()
            ));
        }
        let mut handle = noon::Mobject::from_node(std::rc::Rc::clone(self.scene.store()), node)?;
        noon::legacy::replace_mobject_snapshot(&mut handle, snapshot)
    }

    pub fn bind_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let scale_factor = retained_scale_factor(&text);
        let object = canonical_text_object(id, text)?;
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let node = self.scene.store().borrow_mut().insert_authoring_object();
        // The explicit retained-text export adapter still uses its historical
        // identity-only node; it is never admitted to typed geometry execution.
        self.scene
            .store()
            .borrow_mut()
            .add_member(self.scene.root(), node)
            .map_err(|e| e.to_string())?;
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        self.text_adapters.insert(node, object);
        self.retained_scale_factors.insert(id, scale_factor);
        Ok(())
    }

    pub fn update_text(
        &mut self,
        id: ObjectId,
        text: RetainedTextAuthoringSpec,
    ) -> Result<(), String> {
        let node = self.node(id)?;
        if !self.text_adapters.contains_key(&node) {
            return Err(format!("canonical object {} is not text-backed", id.get()));
        }
        let scale_factor = retained_scale_factor(&text);
        let object = canonical_text_object(id, text)?;
        self.text_adapters.insert(node, object);
        self.retained_scale_factors.insert(id, scale_factor);
        Ok(())
    }

    fn members(&self) -> Result<Vec<noon_core::SemanticNodeId>, String> {
        self.scene
            .store()
            .borrow()
            .node(self.scene.root())
            .map(|node| node.members().to_vec())
            .ok_or_else(|| "semantic scene root is no longer live".into())
    }

    pub fn checkpoint(&self) -> usize {
        self.bindings.len()
    }

    pub fn restore(&mut self, checkpoint: usize) -> Result<(), String> {
        let members = self.members()?;
        if checkpoint > members.len() {
            return Err(format!(
                "canonical authoring checkpoint {checkpoint} exceeds object count {}",
                members.len()
            ));
        }
        let removed = &members[checkpoint..];
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        for node in removed {
            if !self.text_adapters.contains_key(node) {
                transaction.remove_member(self.scene.root(), *node);
            }
        }
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        for node in removed {
            if self.text_adapters.contains_key(node) {
                self.scene
                    .store()
                    .borrow_mut()
                    .remove_member(self.scene.root(), *node)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.bindings.retain(|id, node| {
            if removed.contains(node) {
                self.retained_scale_factors.remove(id);
                false
            } else {
                true
            }
        });
        for node in removed {
            self.text_adapters.remove(node);
            self.identities.remove(node);
        }
        Ok(())
    }

    pub fn lower_execution(&self) -> Result<noon::ExecutionSession, String> {
        if !self.text_adapters.is_empty() {
            return Err("retained text requires the explicit retained execution adapter".into());
        }
        self.scene
            .execution_session()
            .map_err(|error| error.to_string())
    }

    /// Author one host-owned callable occurrence into this scene's semantic store.
    ///
    /// The callback ID has no semantic meaning: Python resolves it only after the
    /// compiler selects this occurrence. Semantic identity, activation interval,
    /// occurrence order, lowering, and session publication remain Rust-owned.
    #[cfg(any(target_arch = "wasm32", test))]
    fn add_updater(
        &mut self,
        handle: &noon::Mobject,
        callback: HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) -> Result<(), String> {
        self.require_pre_execution_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(handle.node_id(), callback, active_from, position);
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Close the first open occurrence for this host callback at an exclusive
    /// authored time. The store validates the complete mutation before commit.
    #[cfg(target_arch = "wasm32")]
    fn remove_updater(
        &mut self,
        handle: &noon::Mobject,
        callback: HostCallbackId,
        inactive_from: f64,
    ) -> Result<(), String> {
        self.require_pre_execution_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_updater(handle.node_id(), callback, inactive_from);
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Close every open callback occurrence on this target at an exclusive
    /// authored time before the canonical execution session exists.
    #[cfg(target_arch = "wasm32")]
    fn clear_updaters(&mut self, handle: &noon::Mobject, inactive_from: f64) -> Result<(), String> {
        self.require_pre_execution_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.clear_updaters(handle.node_id(), inactive_from);
        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_pre_execution_updater_target(&self, handle: &noon::Mobject) -> Result<(), String> {
        if self.live_player.is_some() || self.live_player_transferred {
            return Err(
                "callback registrations must be authored before canonical execution begins".into(),
            );
        }
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        if !self.identities.contains_key(&handle.node_id()) {
            return Err("callback target is not bound to this canonical Scene".into());
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_player(
        &mut self,
        duration: f64,
    ) -> Result<&mut crate::SemanticExecutionPlayer, String> {
        self.prepare_local_player_for_run()?;
        if let Some(player) = self.live_player.as_mut() {
            player.set_loop_duration(duration)?;
        } else {
            self.live_player = Some(self.build_live_player(duration, 0)?);
        }
        self.live_player_returned = false;
        Ok(self.live_player.as_mut().expect("live player initialized"))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn build_live_player(
        &self,
        duration: f64,
        transport_session: u32,
    ) -> Result<crate::SemanticExecutionPlayer, String> {
        crate::SemanticExecutionPlayer::from_live_session(
            self.lower_execution()?,
            std::rc::Rc::clone(self.scene.store()),
            self.scene.root(),
            duration,
            transport_session,
        )
    }

    /// Refresh a dormant presentation runtime only at an explicit run or lease
    /// boundary. Direct edits during an active live authoring session remain an
    /// error rather than silently replacing that session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn prepare_local_player_for_run(&mut self) -> Result<(), String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        let authored_revision = self.scene.store().borrow().scene_revision();
        let stale = self
            .live_player
            .as_ref()
            .is_some_and(|player| player.scene_revision() != authored_revision);
        if stale && !self.live_player_returned {
            return Err("authored scene changed while live execution is active".into());
        }
        if stale {
            self.live_player = None;
            self.live_player_returned = false;
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn returned_player_is_stale(&self) -> bool {
        self.live_player_returned
            && self.live_player.as_ref().is_some_and(|player| {
                player.scene_revision() != self.scene.store().borrow().scene_revision()
            })
    }

    /// Begin an explicit authoring-run publication boundary.
    ///
    /// Renderer recovery returns its player to this context and therefore keeps
    /// its effective runtime. A subsequent Python run may mutate authored state
    /// directly before registration; only this boundary is allowed to discard a
    /// now-stale returned runtime and lower a fresh one on attach.
    #[cfg(any(target_arch = "wasm32", test))]
    fn prepare_execution_run(&mut self) -> Result<(), String> {
        self.prepare_local_player_for_run()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn active_live_player(&mut self) -> Result<&mut crate::SemanticExecutionPlayer, String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        self.live_player
            .as_mut()
            .ok_or_else(|| "begin live execution before reading or mutating it".into())
    }

    /// Ordinary continuation declarations reuse an existing local lease as-is.
    /// Only the first declaration bootstraps a player; an explicit new run uses
    /// `live_player`/`prepare_execution_run` and owns any recovery transition.
    #[cfg(any(target_arch = "wasm32", test))]
    fn active_or_bootstrap_live_player(
        &mut self,
        duration: f64,
    ) -> Result<&mut crate::SemanticExecutionPlayer, String> {
        if self.live_player.is_none() {
            self.live_player(duration)?;
        }
        self.active_live_player()
    }

    /// Report only the Rust-owned lifecycle of this context's retained player.
    /// Python uses this derived observation to choose its wrapper dispatch; it
    /// never records or advances lifecycle state itself.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_execution_ownership(&self) -> &'static str {
        if self.live_player_transferred {
            "transferred"
        } else if self.live_player_returned {
            "returned"
        } else if self.live_player.is_some() {
            "active"
        } else {
            "none"
        }
    }

    /// Query one bound object's authored layout before bootstrap or its coherent
    /// effective layout while this context owns the live runtime. A transferred
    /// player must be returned before authoring can observe it again.
    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_layout(&mut self, handle: &noon::Mobject) -> Result<(f64, f64, f64, f64), String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        if !self.identities.contains_key(&handle.node_id()) {
            return Err("mobject is not bound to this canonical Scene".into());
        }
        if self.returned_player_is_stale() {
            return authored_mobject_layout(handle);
        }
        if let Some(player) = self.live_player.as_mut() {
            let observed = player.live_effective_layout(handle)?;
            return Ok((
                observed.center.0,
                observed.center.1,
                observed.width,
                observed.height,
            ));
        }
        authored_mobject_layout(handle)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_pre_execution_signal_authoring(&self) -> Result<(), String> {
        if self.live_player.is_some() || self.live_player_transferred {
            return Err(
                "signal declarations and bindings must be authored before canonical execution begins"
                    .into(),
            );
        }
        Ok(())
    }

    /// Create one scalar signal in this context's shared semantic store.
    #[cfg(any(target_arch = "wasm32", test))]
    fn create_value_tracker(&mut self, initial: f64) -> Result<noon::ValueTracker, String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        match self.live_player.as_mut() {
            Some(player) => player.live_value_tracker(initial),
            None => self.scene.value_tracker(initial),
        }
    }

    /// Associate one store-owned detached tracker with this Scene.
    #[cfg(any(target_arch = "wasm32", test))]
    fn associate_value_tracker(&mut self, tracker: &noon::ValueTracker) -> Result<(), String> {
        if self.live_player_transferred {
            return Err("live execution session is running in the semantic engine".into());
        }
        match self.live_player.as_mut() {
            Some(player) => player.live_associate_value_tracker(tracker),
            None => self.scene.associate_value_tracker(tracker),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn pointer_position_signal(&self) -> Result<noon::NativeVectorSignal, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.pointer_position_signal()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn viewport_size_signal(&self) -> Result<noon::NativeVectorSignal, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.viewport_size_signal()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn wheel_delta_signal(&self) -> Result<noon::NativeVectorSignal, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.wheel_delta_signal()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn key_state_signal(
        &self,
        code: String,
        initial: bool,
    ) -> Result<noon::NativeBoolSignal, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.key_state_signal(code, initial)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn control_signal(&self, name: String, initial: f64) -> Result<noon::ValueTracker, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.control_signal(name, initial)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn pointer_down_events(&self, button: u8) -> Result<noon::ValueTracker, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.pointer_down_events(button)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn wheel_events(&self) -> Result<noon::ValueTracker, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.wheel_events()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn control_commit_events(&self, name: String) -> Result<noon::ValueTracker, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.control_commit_events(name)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_native_translation(
        &self,
        object: &noon::Mobject,
        signal: &noon::NativeVectorSignal,
    ) -> Result<(), String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.bind_native_translation(object, signal)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_rotation(
        &self,
        object: &noon::Mobject,
        signal: &noon::ValueTracker,
    ) -> Result<(), String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.bind_rotation(object, signal)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_opacity(
        &self,
        object: &noon::Mobject,
        signal: &noon::ValueTracker,
    ) -> Result<(), String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.bind_opacity(object, signal)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_presence(
        &self,
        object: &noon::Mobject,
        signal: &noon::NativeBoolSignal,
    ) -> Result<(), String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.bind_presence(object, signal)
    }

    /// Build only the common `offset + tracker * direction` semantic expression.
    #[cfg(any(target_arch = "wasm32", test))]
    fn tracker_position(
        &self,
        tracker: &noon::ValueTracker,
        direction: SemanticVec3,
        offset: SemanticVec3,
    ) -> Result<noon::TrackerPosition, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.position_from_tracker(tracker, direction, offset)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_tracker_position(
        &self,
        object: &noon::Mobject,
        position: &noon::TrackerPosition,
    ) -> Result<(), String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.bind_position(object, position)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn declare_tracker_play(
        &mut self,
        tracker: &noon::ValueTracker,
        target: f64,
        duration: f64,
        rate_function: noon_core::RateFunction,
    ) -> Result<f64, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .play_value(tracker, target)
            .rate_func(rate_function)
            .run_time(duration)?;
        Ok(self.scene.time())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn tracker_value(&mut self, tracker: &noon::ValueTracker) -> Result<f64, String> {
        if self.live_player_transferred {
            return Err("semantic execution session is running in the semantic engine".into());
        }
        match self.live_player.as_mut() {
            Some(player) => player.live_effective_signal(tracker),
            None => self.scene.value_tracker_value(tracker),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn set_tracker_value(
        &mut self,
        tracker: &noon::ValueTracker,
        value: f64,
    ) -> Result<(), String> {
        if self.live_player_transferred {
            return Err("semantic execution session is running in the semantic engine".into());
        }
        match self.live_player.as_mut() {
            Some(player) => player.live_set_signal(tracker, value),
            None => self.scene.set_value(tracker, value),
        }
    }

    /// Atomically activate one ordinary scalar tracker play without advancing it.
    /// The one retained execution player owns the segment and any required callback
    /// barriers; Python receives only the shared segment endpoint.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_value_tracker_play(
        &mut self,
        tracker: &noon::ValueTracker,
        target: f64,
        duration: f64,
        rate_function: noon_core::RateFunction,
    ) -> Result<f64, String> {
        if !tracker.is_in_store(self.scene.store()) {
            return Err("ValueTracker belongs to another scene store".into());
        }
        self.scene
            .store()
            .borrow()
            .semantic_signal_state(tracker.node_id())
            .map_err(|error| error.to_string())?;
        if !target.is_finite() || target.abs() > f32::MAX as f64 {
            return Err("ValueTracker target must fit the runtime scalar range".into());
        }
        if !duration.is_finite() || duration <= 0.0 {
            return Err("ValueTracker duration must be finite and positive".into());
        }
        if self.live_player.is_none() {
            if self.scene.time() != 0.0 {
                return Err(
                    "ordinary ValueTracker play cannot follow pre-execution canonical timing"
                        .into(),
                );
            }
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(duration.max(1.0), 0)?;
            let end_time = player.live_declare_and_activate_value_tracker(
                tracker,
                target,
                duration,
                rate_function,
            )?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            return Ok(end_time);
        }
        self.active_live_player()?
            .live_declare_and_activate_value_tracker(tracker, target, duration, rate_function)
    }

    /// The authored scalar-track endpoint used for handoff before a player exists.
    #[cfg(any(target_arch = "wasm32", test))]
    fn authored_duration(&self) -> f64 {
        self.live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
    }

    /// Advance the shared Rust authoring cursor without declaring legacy timing.
    #[cfg(any(target_arch = "wasm32", test))]
    fn authored_wait(&mut self, duration: f64) -> Result<f64, String> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.wait(duration)?;
        Ok(self.scene.time())
    }

    /// Complete one canonical continuation wait in the retained live session.
    ///
    /// Before bootstrap this remains the existing Rust authored-cursor wait for
    /// scalar-track authoring.  Once ordinary live execution exists, the wait is
    /// a real session segment and its endpoint is reconciled before returning to
    /// Python continuation code.
    #[cfg(any(target_arch = "wasm32", test))]
    fn ordinary_wait(&mut self, duration: f64) -> Result<f64, String> {
        if self.live_player.is_none() {
            return self.authored_wait(duration);
        }
        if self.active_live_player()?.has_required_callbacks() {
            return Err(
                "ordinary endpoint-only wait cannot execute required callbacks; use a continuation"
                    .into(),
            );
        }
        let end_time = self.begin_ordinary_wait(duration)?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Begin one ordinary wait without advancing it.
    ///
    /// This exists for the async worker continuation path. The returned endpoint is derived
    /// from the player-owned segment; no Python or JavaScript cursor is created.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_wait(&mut self, duration: f64) -> Result<f64, String> {
        if self.live_player.is_none() {
            if self.scene.time() != 0.0 {
                return Err(
                    "ordinary asynchronous wait cannot follow pre-execution canonical timing"
                        .into(),
                );
            }
            // A wait has no animation extent, but the presentation clock still needs a
            // positive valid range before its session-derived deadline replaces it.
            self.live_player(duration.max(1.0))?;
        }
        let player = self.active_live_player()?;
        player.live_wait(duration)
    }

    /// Read only the live runtime's authored handoff duration.
    ///
    /// Static authoring has no live session, so its existing authored-duration
    /// projection remains the fallback at the Python export boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_handoff_duration(&self) -> Option<f64> {
        self.live_player
            .as_ref()
            .and_then(crate::SemanticExecutionPlayer::live_handoff_duration)
    }

    /// Add replayable animation meaning before a live session is created.
    #[cfg(any(target_arch = "wasm32", test))]
    fn declare_live_transform_to(
        &self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<noon::DeclaredAnimation, String> {
        if self.live_player.is_some() || self.live_player_transferred {
            return Err("declare live animations before beginning execution".into());
        }
        self.scene.declare_transform_to(source, target, options)
    }

    /// Run one basic ordinary leaf fade through the retained live session.
    ///
    /// Rust owns lifecycle membership, appearance tracks, activation, and
    /// completion. The object ID only records this wrapper's derived binding
    /// after the shared fade has succeeded; it is never a semantic identity.
    #[cfg(target_arch = "wasm32")]
    fn ordinary_play_fade(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        direction: SemanticFadeDirection,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let end_time = self.begin_ordinary_fade(id, target, direction, options)?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Run one ordinary single-leaf Create through the retained live session.
    #[cfg(target_arch = "wasm32")]
    fn ordinary_play_create(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let end_time = self.begin_ordinary_create(id, target, options)?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Run one ordinary single-leaf Uncreate through the retained live session.
    #[cfg(target_arch = "wasm32")]
    fn ordinary_play_uncreate(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let end_time = self.begin_ordinary_uncreate(id, target, options)?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Atomically bind, introduce, and activate one detached leaf's Create reveal.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_create(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
            return Err("ordinary Create mobject belongs to another authoring store".into());
        }
        target.validate()?;
        let node = target.node_id();
        if self.bindings.contains_key(&id) || self.identities.contains_key(&node) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        if self.live_player.is_none() && self.scene.time() != 0.0 {
            return Err("ordinary Create cannot follow pre-execution canonical timing".into());
        }
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let end_time = if self.live_player.is_none() {
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            let end_time = player.live_declare_and_activate_create(target, options)?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            end_time
        } else {
            self.active_live_player()?
                .live_declare_and_activate_create(target, options)?
        };
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(end_time)
    }

    /// Atomically bind, admit, and activate one detached leaf's reverse reveal.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_uncreate(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
            return Err("ordinary Uncreate mobject belongs to another authoring store".into());
        }
        target.validate()?;
        let node = target.node_id();
        if self.bindings.contains_key(&id) || self.identities.contains_key(&node) {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        if self.live_player.is_none() && self.scene.time() != 0.0 {
            return Err("ordinary Uncreate cannot follow pre-execution canonical timing".into());
        }
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let end_time = if self.live_player.is_none() {
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            let end_time = player.live_declare_and_activate_uncreate(target, options)?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            end_time
        } else {
            self.active_live_player()?
                .live_declare_and_activate_uncreate(target, options)?
        };
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(end_time)
    }

    /// Run one flat parallel Create through the retained live session.
    #[cfg(target_arch = "wasm32")]
    fn ordinary_play_create_parallel(
        &mut self,
        children: &[(ObjectId, noon::Mobject, noon_core::AnimationOptions)],
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let end_time = self.begin_ordinary_create_parallel(children, play_options)?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Atomically bind detached leaves and activate one flat parallel Create segment.
    ///
    /// Derived wrapper bindings are recorded only after the shared session accepted the
    /// complete transaction. The candidate owns no semantic identity, timing, or runtime state.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_create_parallel(
        &mut self,
        children: &[(ObjectId, noon::Mobject, noon_core::AnimationOptions)],
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if children.is_empty() {
            return Err("ordinary parallel Create requires at least one detached leaf".into());
        }
        if self.live_player.is_none() && self.scene.time() != 0.0 {
            return Err("ordinary Create cannot follow pre-execution canonical timing".into());
        }

        let mut object_ids = BTreeSet::new();
        let mut nodes = BTreeSet::new();
        for (id, target, _) in children {
            if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
                return Err("ordinary Create mobject belongs to another authoring store".into());
            }
            target.validate()?;
            let node = target.node_id();
            if !object_ids.insert(*id) || !nodes.insert(node) {
                return Err("ordinary parallel Create requires distinct detached leaves".into());
            }
            if self.bindings.contains_key(id) || self.identities.contains_key(&node) {
                return Err(format!("canonical object {} is already bound", id.get()));
            }
        }

        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(play_options.run_time.unwrap_or(0.0))
            .max(
                children
                    .iter()
                    .filter_map(|(_, _, options)| options.run_time)
                    .fold(0.0, f64::max),
            );
        let requests = children
            .iter()
            .map(|(_, target, options)| (target, *options))
            .collect::<Vec<_>>();
        let end_time = if self.live_player.is_none() {
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            let end_time =
                player.live_declare_and_activate_create_parallel(&requests, play_options)?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            end_time
        } else {
            self.active_live_player()?
                .live_declare_and_activate_create_parallel(&requests, play_options)?
        };
        for (id, target, _) in children {
            let node = target.node_id();
            self.bindings.insert(*id, node);
            self.identities.insert(node, *id);
        }
        Ok(end_time)
    }

    /// Atomically declare and activate one basic ordinary fade without advancing it.
    ///
    /// A FadeIn may bind an existing detached semantic handle. A FadeOut retains
    /// its derived binding so that the exact same handle can later re-enter via
    /// `Scene.add`; membership itself remains entirely in the shared session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_fade(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        direction: SemanticFadeDirection,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
            return Err("ordinary fade mobject belongs to another authoring store".into());
        }
        target.validate()?;
        let node = target.node_id();
        let new_binding = match (
            direction,
            self.bindings.get(&id),
            self.identities.get(&node),
        ) {
            (SemanticFadeDirection::In, None, None) => true,
            (_, Some(bound_node), Some(bound_id)) if *bound_node == node && *bound_id == id => {
                false
            }
            (SemanticFadeDirection::Out, None, None) => {
                return Err("ordinary FadeOut target is not bound to this canonical Scene".into());
            }
            _ => return Err(format!("canonical object {} is already bound", id.get())),
        };
        if self.live_player.is_none() && self.scene.time() != 0.0 {
            return Err("ordinary fade cannot follow pre-execution canonical timing".into());
        }

        // The retained player needs a valid presentation extent before activation.
        // An existing returned continuation is preserved exactly; first bootstrap
        // remains provisional until the shared activation succeeds.
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let end_time = if self.live_player.is_none() {
            // Build the initial runtime provisionally. A failed shared preflight
            // must not install a player or change context lease ownership.
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            let end_time = player.live_declare_and_activate_fade(target, direction, options)?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            end_time
        } else {
            // `declare_and_activate_fade` is the shared atomic preflight: required
            // callbacks, bindings, membership, options, and lifecycle conflicts all
            // fail before its semantic/runtime publication.
            self.active_live_player()?
                .live_declare_and_activate_fade(target, direction, options)?
        };
        if new_binding {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(end_time)
    }

    /// Read the shared session's direct-root membership for one retained wrapper.
    ///
    /// This lets Python update only its derived wrapper attachment after a completed
    /// FadeOut without storing lifecycle state or adding metadata to the player receipt.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_contains_mobject(&mut self, target: &noon::Mobject) -> Result<bool, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        target.validate()?;
        if !self.identities.contains_key(&target.node_id()) {
            return Err("live Mobject is not bound to this canonical Scene".into());
        }
        self.active_live_player()?.live_contains(target)
    }

    /// Run one supported ordinary leaf TransformTo through the one retained
    /// session. Declaration, activation, endpoint evaluation, and completion
    /// remain Rust/session operations; Python only supplies typed handles and
    /// resolved options.
    #[cfg(any(target_arch = "wasm32", test))]
    fn ordinary_play_transform_to(
        &mut self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !self.can_ordinary_transform_to(source, target, options)? {
            return Err("ordinary affine animation payload is not yet supported".into());
        }
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let bootstrapped = self.live_player.is_none();
        if bootstrapped {
            self.live_player(bootstrap_duration)?;
        }
        if self.active_live_player()?.has_required_callbacks() {
            if bootstrapped {
                self.live_player = None;
                self.live_player_returned = false;
            }
            return Err(
                "ordinary endpoint-only animation cannot execute required callbacks; use a continuation"
                    .into(),
            );
        }
        let end_time = self.begin_ordinary_transform_to(source, target, options)?;
        let player = self.active_live_player()?;
        // Reaching the endpoint still has completion reconciliation pending.
        // The shared completion operation validates time and callback coherence.
        player.live_advance_segment_to(end_time)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    /// Atomically declare and activate one ordinary leaf transform without advancing it.
    ///
    /// The retained player stores the existing shared execution segment. A worker may lease
    /// that player and use its Rust-owned wake/drive/completion methods without rebuilding the
    /// context or manufacturing a frontend segment identity.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_transform_to(
        &mut self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        if !self.can_ordinary_transform_to(source, target, options)? {
            return Err("ordinary affine animation payload is not yet supported".into());
        }

        // `live_player` needs a valid presentation extent before activation. The
        // player replaces it with the exact returned segment endpoint below.
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let player = self.active_or_bootstrap_live_player(bootstrap_duration)?;
        player.live_declare_and_activate_transform_to(source, target, options)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn ordinary_play_mixed_composition(
        &mut self,
        kind: noon_core::SemanticAnimationCompositionKind,
        children: &[OrdinaryCompositionChild],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let end = self.activate_ordinary_mixed_composition(
            kind,
            children,
            composition_options,
            play_options,
            false,
        )?;
        let player = self.active_live_player()?;
        player.live_advance_segment_to(end)?;
        player.live_complete_segment()?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".into())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_mixed_composition(
        &mut self,
        kind: noon_core::SemanticAnimationCompositionKind,
        children: &[OrdinaryCompositionChild],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        self.activate_ordinary_mixed_composition(
            kind,
            children,
            composition_options,
            play_options,
            true,
        )
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn activate_ordinary_mixed_composition(
        &mut self,
        kind: noon_core::SemanticAnimationCompositionKind,
        children: &[OrdinaryCompositionChild],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
        allow_required_callbacks: bool,
    ) -> Result<f64, String> {
        self.validate_ordinary_mixed_composition(children, composition_options, play_options)?;
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(
                play_options
                    .run_time
                    .or(composition_options.run_time)
                    .unwrap_or(1.0),
            );
        let requests = children
            .iter()
            .map(|child| match child {
                OrdinaryCompositionChild::TransformTo {
                    source,
                    target,
                    interpolation,
                    options,
                    ..
                } => {
                    let request = match interpolation {
                        noon_core::SemanticTransformInterpolation::Affine => {
                            noon::TransformToRequest::new(source, target, *options)
                        }
                        noon_core::SemanticTransformInterpolation::PointCorrespondence => {
                            noon::TransformToRequest::point_correspondence(source, target, *options)
                        }
                    };
                    noon::AnimationCompositionRequest::TransformTo(request)
                }
                OrdinaryCompositionChild::Rotate {
                    target,
                    angle,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::Rotate {
                    target,
                    angle: *angle,
                    options: *options,
                },
            })
            .collect::<Vec<_>>();
        let end = if self.live_player.is_none() {
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            if !allow_required_callbacks && player.has_required_callbacks() {
                return Err("ordinary composition with required callbacks needs an asynchronous continuation".into());
            }
            let end = player.live_declare_and_activate_animation_composition(
                kind,
                &requests,
                composition_options,
                play_options,
            )?;
            self.live_player = Some(player);
            self.live_player_returned = false;
            end
        } else {
            let player = self.active_live_player()?;
            if !allow_required_callbacks && player.has_required_callbacks() {
                return Err("ordinary composition with required callbacks needs an asynchronous continuation".into());
            }
            player.live_declare_and_activate_animation_composition(
                kind,
                &requests,
                composition_options,
                play_options,
            )?
        };
        for child in children {
            let (Some(id), target) = (match child {
                OrdinaryCompositionChild::TransformTo {
                    entering_id,
                    source,
                    ..
                } => (*entering_id, source),
                OrdinaryCompositionChild::Rotate {
                    entering_id,
                    target,
                    ..
                } => (*entering_id, target),
            }) else {
                continue;
            };
            self.bindings.insert(id, target.node_id());
            self.identities.insert(target.node_id(), id);
        }
        Ok(end)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn can_ordinary_transform_to(
        &self,
        source: &noon::Mobject,
        target: &noon::Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<bool, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), source.store())
            || !std::rc::Rc::ptr_eq(self.scene.store(), target.store())
        {
            return Err(
                "ordinary affine animation mobjects belong to another authoring store".into(),
            );
        }
        source.validate()?;
        target.validate()?;
        if !self.identities.contains_key(&source.node_id()) {
            return Err(
                "ordinary affine animation source is not bound to this canonical Scene".into(),
            );
        }
        if self.identities.contains_key(&target.node_id()) {
            return Err("ordinary affine animation target must be a detached Mobject".into());
        }
        if self.live_player.is_none() && self.scene.time() != 0.0 {
            return Err(
                "ordinary affine animation cannot follow pre-execution canonical timing".into(),
            );
        }
        self.scene
            .can_ordinary_transform_to(source, target, options)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn validate_ordinary_mixed_composition(
        &self,
        children: &[OrdinaryCompositionChild],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    ) -> Result<(), String> {
        if children.is_empty() {
            return Err("ordinary composition requires at least one child".into());
        }
        noon_core::resolve_animation_options(
            noon_core::AnimationDefaults::MANIM,
            composition_options,
            play_options,
        )
        .map_err(|error| error.to_string())?;
        let mut ids = BTreeSet::new();
        let mut entering_nodes = BTreeSet::new();
        for child in children {
            let (entering_id, target, options) = match child {
                OrdinaryCompositionChild::TransformTo {
                    entering_id,
                    source,
                    target,
                    options,
                    ..
                } => {
                    if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
                        return Err(
                            "ordinary composition target belongs to another authoring store".into(),
                        );
                    }
                    target.validate()?;
                    if self.identities.contains_key(&target.node_id()) {
                        return Err(
                            "ordinary composition TransformTo target state must be detached".into(),
                        );
                    }
                    (*entering_id, source, *options)
                }
                OrdinaryCompositionChild::Rotate {
                    entering_id,
                    target,
                    angle,
                    options,
                } => {
                    if !angle.is_finite() {
                        return Err("ordinary Rotate angle must be finite".into());
                    }
                    (*entering_id, target, *options)
                }
            };
            if !std::rc::Rc::ptr_eq(self.scene.store(), target.store()) {
                return Err(
                    "ordinary composition target belongs to another authoring store".into(),
                );
            }
            target.validate()?;
            noon_core::resolve_animation_options(
                noon_core::AnimationDefaults::MANIM,
                options,
                noon_core::AnimationOptions::new(),
            )
            .map_err(|error| error.to_string())?;
            match entering_id {
                Some(id) => {
                    if self.bindings.contains_key(&id)
                        || self.identities.contains_key(&target.node_id())
                        || !ids.insert(id)
                        || !entering_nodes.insert(target.node_id())
                    {
                        return Err("ordinary composition requires unique detached targets and wrapper identities".into());
                    }
                }
                None if !self.identities.contains_key(&target.node_id()) => {
                    return Err("ordinary composition bound target has no wrapper identity".into());
                }
                None => {}
            }
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_target_editor(&mut self, source: &noon::Mobject) -> Result<noon::Mobject, String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), source.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        source.validate()?;
        match self.live_execution_ownership() {
            "none" => source.target_editor(),
            "active" | "returned" => self.active_live_player()?.live_target_editor(source),
            "transferred" => Err("live execution session is running in the semantic engine".into()),
            _ => unreachable!("canonical live ownership has one closed set of states"),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_add_mobject(&mut self, id: ObjectId, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate()?;
        let node = handle.node_id();
        let new_binding = match (self.bindings.get(&id), self.identities.get(&node)) {
            (None, None) => true,
            (Some(bound_node), Some(bound_id)) if *bound_node == node && *bound_id == id => false,
            _ => return Err(format!("canonical object {} is already bound", id.get())),
        };
        self.active_live_player()?.live_add(handle)?;
        if new_binding {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_remove_mobject(&mut self, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), handle.store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        let node = handle.node_id();
        let id = *self
            .identities
            .get(&node)
            .ok_or("live Mobject is not bound to this Scene")?;
        self.active_live_player()?.live_remove(handle)?;
        self.identities.remove(&node);
        self.bindings.remove(&id);
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_replace_content(
        &mut self,
        target: &noon::Mobject,
        source: &noon::Mobject,
    ) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.store(), target.store())
            || !std::rc::Rc::ptr_eq(self.scene.store(), source.store())
        {
            return Err("mobject belongs to another authoring store".into());
        }
        self.active_live_player()?
            .live_replace_content(target, source)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn take_execution_player(
        &mut self,
        duration: f64,
        transport_session: u32,
    ) -> Result<crate::SemanticExecutionPlayer, String> {
        self.prepare_local_player_for_run()?;
        if let Some(player) = self.live_player.as_mut() {
            player.rebind_transport(duration, transport_session)?;
            let player = self.live_player.take().expect("live player initialized");
            self.live_player_transferred = true;
            self.live_player_returned = false;
            return Ok(player);
        }
        let player = self.build_live_player(duration, transport_session)?;
        self.live_player_transferred = true;
        self.live_player_returned = false;
        Ok(player)
    }

    /// Return a player after endpoint setup or renderer recovery. This preserves
    /// the one runtime so reattachment never lowers a parallel session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn return_execution_player(
        &mut self,
        player: crate::SemanticExecutionPlayer,
    ) -> Result<(), String> {
        if !self.live_player_transferred || self.live_player.is_some() {
            return Err("semantic execution player is not leased by this context".into());
        }
        self.live_player = Some(player);
        self.live_player_transferred = false;
        self.live_player_returned = true;
        Ok(())
    }

    /// Resume the exact returned player for a newly-authored continuation segment.
    ///
    /// Unlike a new endpoint/recovery handoff, this keeps the existing transport encoder,
    /// resource bundle, session sequence, and snapshot state. The authoring continuation
    /// may only resume a player after it has declared one supported pending segment.
    #[cfg(any(target_arch = "wasm32", test))]
    fn resume_execution_player(&mut self) -> Result<crate::SemanticExecutionPlayer, String> {
        if self.live_player_transferred || !self.live_player_returned {
            return Err("semantic continuation player is not returned to this context".into());
        }
        let player = self
            .live_player
            .as_ref()
            .ok_or("semantic continuation context has no returned player")?;
        if !player.has_pending_live_segment() {
            return Err("semantic continuation has no pending segment to resume".into());
        }
        player.require_callback_progression_available()?;
        let player = self
            .live_player
            .take()
            .expect("validated returned player must remain installed");
        self.live_player_transferred = true;
        self.live_player_returned = false;
        Ok(player)
    }

    /// Encode final authored changes through the returned player's existing worker
    /// transport. This neither leases nor advances the completed runtime.
    #[cfg(any(target_arch = "wasm32", test))]
    fn drain_returned_publication_json(&mut self) -> Result<Option<String>, String> {
        if self.live_player_transferred || !self.live_player_returned {
            return Err("final publication requires a returned execution player".into());
        }
        let player = self
            .live_player
            .as_mut()
            .ok_or("returned player is absent")?;
        player.require_callback_progression_available()?;
        if player.has_pending_live_segment() {
            return Err("final publication requires a completed continuation segment".into());
        }
        player.drain_delta_json()
    }

    /// Derive the migration/export document from live semantic state at the boundary.
    pub fn finalize(
        &self,
        geometry_tracks: Vec<TrackDefinition>,
        retained_tracks: Vec<RetainedTrackAuthoringSpec>,
        family_animations: Vec<FamilyAnimationRequest>,
        camera_object: Option<ObjectId>,
    ) -> Result<SceneSpec, String> {
        let mut objects = Vec::with_capacity(self.identities.len());
        for node in self.members()? {
            if let Some(text) = self.text_adapters.get(&node) {
                objects.push(text.clone());
                continue;
            }
            let handle = noon::Mobject::from_node(std::rc::Rc::clone(self.scene.store()), node)?;
            let state = handle.state()?;
            if state.content.text().is_some() {
                objects.push(canonical_text_export(
                    &self.scene.store().borrow(),
                    *self
                        .identities
                        .get(&node)
                        .ok_or("unbound semantic scene member")?,
                    &state,
                )?);
                continue;
            }
            let snapshot = noon::legacy::export_mobject_snapshot(&handle)?;
            let mut object = ObjectSpec::geometry(
                *self
                    .identities
                    .get(&node)
                    .ok_or("unbound semantic scene member")?,
                snapshot.geometry,
            );
            object.transform = snapshot.transform;
            object.style = snapshot.style;
            objects.push(object);
        }
        let tracks = materialize_retained_tracks(
            &geometry_tracks,
            retained_tracks,
            &self.retained_scale_factors,
        )
        .map_err(|error| error.to_string())?;
        let mut spec = SceneSpec::new(objects, tracks).map_err(|error| error.to_string())?;
        spec.family_animations = family_animations;
        spec.camera_object = camera_object;
        spec.validate().map_err(|error| error.to_string())?;
        Ok(spec)
    }

    fn node(&self, id: ObjectId) -> Result<noon_core::SemanticNodeId, String> {
        self.bindings
            .get(&id)
            .copied()
            .ok_or_else(|| format!("unknown canonical object {}", id.get()))
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn authored_mobject_layout(handle: &noon::Mobject) -> Result<(f64, f64, f64, f64), String> {
    let Some(bounds) = handle.layout_bounds()? else {
        let (center_x, center_y) = handle.center()?;
        return Ok((center_x, center_y, 0.0, 0.0));
    };
    Ok((
        (bounds.min_x + bounds.max_x) * 0.5,
        (bounds.min_y + bounds.max_y) * 0.5,
        bounds.width(),
        bounds.height(),
    ))
}

/// Reconstruct the legacy source document only when the normal semantic session
/// is unavailable (for example, callback/timeline execution). This #959 export
/// seam reads immutable content and presentation from the shared store; Python
/// wrappers never provide a parallel Text source or transform representation.
fn canonical_text_export(
    store: &noon_core::SemanticStore,
    id: ObjectId,
    state: &SemanticObjectState,
) -> Result<ObjectSpec, String> {
    let text = state
        .content
        .text()
        .ok_or("canonical text export requires text content")?;
    let resource = store
        .text_resources()
        .get(text)
        .ok_or("canonical text export references an unknown text resource")?;
    let (text, transform) = match resource.kind {
        TextSourceKind::Plain => {
            let run = resource
                .runs
                .first()
                .ok_or("canonical native text resource has no shaped run")?;
            (
                TextSpec::native_plain(
                    resource.source.as_ref(),
                    run.font.family.as_ref(),
                    run.font_size,
                    native_line_spacing(resource)?,
                ),
                text_export_transform(
                    state.transform,
                    f64::from(noon::NATIVE_POINT_TO_SCENE_SCALE),
                )?,
            )
        }
        TextSourceKind::Typst => (
            TextSpec::typst(resource.source.as_ref(), noon::DEFAULT_TYPST_FONT_SIZE),
            text_export_transform(
                state.transform,
                f64::from(noon::DEFAULT_TYPST_FONT_SIZE * noon::SCALE_FACTOR_PER_FONT_POINT),
            )?,
        ),
        TextSourceKind::MathTypst => (
            TextSpec::math_typst(resource.source.as_ref(), noon::DEFAULT_TYPST_FONT_SIZE),
            text_export_transform(
                state.transform,
                f64::from(noon::DEFAULT_TYPST_FONT_SIZE * noon::SCALE_FACTOR_PER_FONT_POINT),
            )?,
        ),
        kind => {
            return Err(format!(
                "canonical text export does not support {kind:?} source"
            ))
        }
    };
    let mut object = ObjectSpec::text(id, text);
    object.transform = transform;
    object.style = legacy_style(&state.style)?;
    Ok(object)
}

/// Derive the temporary #959 Text authoring codec from shared semantic
/// state. This is only consumed when the normal live session cannot run; Python
/// wrappers never retain a second text source or presentation model.
#[cfg(target_arch = "wasm32")]
pub(crate) fn canonical_text_authoring_spec(
    store: &noon_core::SemanticStore,
    state: &SemanticObjectState,
) -> Result<RetainedTextAuthoringSpec, String> {
    let object = canonical_text_export(store, ObjectId::new(0), state)?;
    let ObjectSpec {
        content: ObjectSpecContent::Text(text),
        transform,
        style,
        ..
    } = object
    else {
        return Err("canonical text export produced non-text content".into());
    };
    let TextSpec {
        kind,
        source,
        font_size,
        options,
    } = text;
    let mut spec = match (kind, options) {
        (
            TextSpecKind::Plain,
            TextSpecOptions::NativePlain {
                font_family,
                line_spacing,
            },
        ) => RetainedTextAuthoringSpec::native(source, font_family, font_size, line_spacing)?,
        (TextSpecKind::Typst, TextSpecOptions::Default) => {
            RetainedTextAuthoringSpec::new(source, false, font_size)?
        }
        (TextSpecKind::MathTypst, TextSpecOptions::Default) => {
            RetainedTextAuthoringSpec::new(source, true, font_size)?
        }
        (kind, _) => {
            return Err(format!(
                "canonical text export produced unsupported {kind:?} retained content"
            ));
        }
    };
    spec.transform = transform;
    spec.color = style.fill.unwrap_or(noon_core::WHITE);
    spec.opacity = style.opacity;
    Ok(spec)
}

fn text_export_transform(
    transform: SemanticTransform2_5D,
    point_scale: f64,
) -> Result<Transform2D, String> {
    Ok(Transform2D {
        translation: Vec2::new(
            legacy_f32("text translation x", transform.translation.x)?,
            legacy_f32("text translation y", transform.translation.y)?,
        ),
        scale: Vec2::new(
            legacy_f32("text scale x", transform.scale.x / point_scale)?,
            legacy_f32("text scale y", transform.scale.y / point_scale)?,
        ),
        rotation: legacy_f32("text rotation", transform.rotation_z)?,
    })
}

fn native_line_spacing(resource: &noon_core::TextResource) -> Result<f32, String> {
    let Some(first) = resource.runs.first() else {
        return Err("canonical native text resource has no shaped run".into());
    };
    if resource.runs.len() < 2 {
        // A single line has no observable line advance. Preserve Manim's ordinary
        // default spelling rather than manufacturing wrapper-side metadata.
        return Ok(-1.0);
    }
    let second = &resource.runs[1];
    let advance = first.transform.ty - second.transform.ty;
    let spacing = advance / first.font_size - 1.0;
    if !spacing.is_finite() || spacing < -1.0 {
        return Err("canonical native text resource has invalid line spacing".into());
    }
    Ok(spacing)
}

fn legacy_style(style: &SemanticStyle) -> Result<Style, String> {
    Ok(Style {
        fill: legacy_color(style.fill.as_ref(), style.fill_opacity)?,
        stroke: legacy_color(style.stroke.as_ref(), style.stroke_opacity)?,
        stroke_width: legacy_f32("text stroke width", style.stroke_width)?,
        stroke_width_mode: style.stroke_width_mode,
        stroke_join: style.stroke_join,
        stroke_cap: style.stroke_cap,
        opacity: legacy_f32("text object opacity", style.object_opacity)?,
    })
}

fn legacy_color(paint: Option<&SemanticPaint>, opacity: f64) -> Result<Option<Color>, String> {
    let Some(paint) = paint else {
        return Ok(None);
    };
    let SemanticPaint::Solid(color) = paint else {
        return Err("legacy text export does not support resource-backed paint".into());
    };
    Ok(Some(Color {
        alpha: legacy_f32("text paint opacity", f64::from(color.alpha) * opacity)?,
        ..*color
    }))
}

fn legacy_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn retained_scale_factor(text: &RetainedTextAuthoringSpec) -> Vec2 {
    let factor = match &text.backend {
        RetainedTextBackendSpec::Native { .. } => noon::NATIVE_POINT_TO_SCENE_SCALE,
        RetainedTextBackendSpec::Typst { .. } => text.font_size * noon::SCALE_FACTOR_PER_FONT_POINT,
    };
    Vec2::new(factor, factor)
}

fn canonical_text_object(
    id: ObjectId,
    text: RetainedTextAuthoringSpec,
) -> Result<ObjectSpec, String> {
    text.validate()?;
    let RetainedTextAuthoringSpec {
        source,
        backend,
        font_size,
        transform,
        color,
        opacity,
    } = text;
    let text = match backend {
        RetainedTextBackendSpec::Native {
            font_family,
            line_spacing,
        } => TextSpec::native_plain(source, font_family, font_size, line_spacing),
        RetainedTextBackendSpec::Typst { math: false } => TextSpec::typst(source, font_size),
        RetainedTextBackendSpec::Typst { math: true } => TextSpec::math_typst(source, font_size),
    };
    let mut object = ObjectSpec::text(id, text);
    object.transform = transform;
    object.style = Style {
        fill: Some(color),
        stroke: None,
        stroke_width: 0.0,
        opacity,
        ..Style::default()
    };
    Ok(object)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use serde::de::DeserializeOwned;
    use wasm_bindgen::prelude::*;

    use super::*;

    fn js_error(error: impl ToString) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn parse_json<T: DeserializeOwned>(label: &str, json: &str) -> Result<T, JsValue> {
        serde_json::from_str(json).map_err(|error| js_error(format!("invalid {label}: {error}")))
    }

    fn parse_object_id(label: &str, value: &str) -> Result<ObjectId, JsValue> {
        value
            .parse::<u64>()
            .map(ObjectId::new)
            .map_err(|error| js_error(format!("invalid {label} {value:?}: {error}")))
    }

    fn parse_callback_id(value: &str) -> Result<HostCallbackId, JsValue> {
        value
            .parse::<u64>()
            .map(HostCallbackId::new)
            .map_err(|error| js_error(format!("invalid callback ID {value:?}: {error}")))
    }

    fn parse_button(value: u32) -> Result<u8, JsValue> {
        u8::try_from(value).map_err(|_| js_error("button must be in the range 0..255"))
    }

    fn parse_fade_direction(value: &str) -> Result<SemanticFadeDirection, JsValue> {
        match value {
            "in" => Ok(SemanticFadeDirection::In),
            "out" => Ok(SemanticFadeDirection::Out),
            _ => Err(js_error(format!(
                "ordinary fade direction must be \"in\" or \"out\", got {value:?}"
            ))),
        }
    }

    fn callback_color(
        label: &str,
        red: Option<f64>,
        green: Option<f64>,
        blue: Option<f64>,
        alpha: Option<f64>,
    ) -> Result<Option<Color>, JsValue> {
        match (red, green, blue, alpha) {
            (None, None, None, None) => Ok(None),
            (Some(red), Some(green), Some(blue), Some(alpha)) => {
                let channel = |name: &str, value: f64| {
                    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
                        Err(js_error(format!(
                            "{label}.{name} must be a finite f32-compatible number"
                        )))
                    } else {
                        Ok(value as f32)
                    }
                };
                if !(0.0..=1.0).contains(&alpha) {
                    return Err(js_error(format!("{label}.alpha must be between 0 and 1")));
                }
                Ok(Some(Color::rgba(
                    channel("red", red)?,
                    channel("green", green)?,
                    channel("blue", blue)?,
                    alpha as f32,
                )))
            }
            _ => Err(js_error(format!(
                "{label} must provide either all RGBA channels or none"
            ))),
        }
    }

    fn callback_paint_style(fill: Option<Color>, stroke: Option<Color>) -> Style {
        Style {
            fill,
            stroke,
            ..Style::default()
        }
    }

    fn callback_paint_result(style: Style) -> WasmCallbackPaint {
        WasmCallbackPaint {
            fill: style.fill,
            stroke: style.stroke,
        }
    }

    #[wasm_bindgen]
    pub struct CanonicalAuthoringSceneContext {
        inner: CanonicalAuthoringScene,
    }

    #[wasm_bindgen]
    pub struct WasmLiveMobjectState {
        state: noon::EffectiveMobjectState,
    }

    #[wasm_bindgen]
    pub struct WasmMobjectLayoutObservation {
        center_x: f64,
        center_y: f64,
        width: f64,
        height: f64,
    }

    /// Pure derived result of one shared callback property operation.
    #[wasm_bindgen]
    pub struct WasmCallbackTransform {
        transform: Transform2D,
    }

    /// Pure derived paint result for one shared callback style operation.
    #[wasm_bindgen]
    pub struct WasmCallbackPaint {
        fill: Option<Color>,
        stroke: Option<Color>,
    }

    /// Callback-local analytic Line operand. It owns only validated endpoints and
    /// allocates no semantic identity or authored store node.
    #[wasm_bindgen]
    pub struct WasmCallbackLineTarget {
        start: Vec2,
        end: Vec2,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    /// Opaque JS/Python wrapper over a replayable shared semantic declaration.
    #[wasm_bindgen]
    pub struct WasmDeclaredAnimationHandle {
        declaration: noon::DeclaredAnimation,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    /// Consumed, inert input for one flat ordinary transform composition.
    ///
    /// This owns opaque shared handles and unresolved semantic options only. It
    /// contains no semantic IDs, resolved intervals, execution tracks, or clock.
    #[wasm_bindgen]
    pub struct WasmOrdinaryTransformCompositionBuilder {
        kind: noon_core::SemanticAnimationCompositionKind,
        children: Vec<OrdinaryCompositionChild>,
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    }

    /// Consumed, inert input for one flat ordinary parallel Create request.
    ///
    /// It carries only wrapper-derived IDs, opaque shared handles, and unresolved
    /// options. The shared Rust session owns admission, schedule, reveal tracks,
    /// and execution identity when this candidate is consumed.
    #[wasm_bindgen]
    pub struct WasmOrdinaryCreateParallelBuilder {
        children: Vec<(ObjectId, noon::Mobject, noon_core::AnimationOptions)>,
        play_options: noon_core::AnimationOptions,
    }

    /// Opaque Python/JS identity for one canonical scalar input signal.
    #[wasm_bindgen]
    pub struct WasmValueTrackerHandle {
        tracker: noon::ValueTracker,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    /// Opaque JS/Python identity for one canonical native vector source.
    #[wasm_bindgen]
    pub struct WasmNativeVectorSignalHandle {
        signal: noon::NativeVectorSignal,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    /// Opaque JS/Python identity for one canonical native boolean source.
    #[wasm_bindgen]
    pub struct WasmNativeBoolSignalHandle {
        signal: noon::NativeBoolSignal,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    /// Opaque derived position expression; evaluation stays in the session.
    #[wasm_bindgen]
    pub struct WasmTrackerPositionHandle {
        position: noon::TrackerPosition,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    }

    #[wasm_bindgen]
    impl WasmLiveMobjectState {
        #[wasm_bindgen(getter, js_name = translationX)]
        pub fn translation_x(&self) -> f64 {
            self.state.transform.translation.x as f64
        }
        #[wasm_bindgen(getter, js_name = translationY)]
        pub fn translation_y(&self) -> f64 {
            self.state.transform.translation.y as f64
        }
    }

    #[wasm_bindgen]
    impl WasmMobjectLayoutObservation {
        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            self.center_x
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            self.center_y
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            self.width
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.height
        }
    }

    #[wasm_bindgen]
    impl WasmCallbackTransform {
        #[wasm_bindgen(getter, js_name = translationX)]
        pub fn translation_x(&self) -> f64 {
            f64::from(self.transform.translation.x)
        }

        #[wasm_bindgen(getter, js_name = translationY)]
        pub fn translation_y(&self) -> f64 {
            f64::from(self.transform.translation.y)
        }

        #[wasm_bindgen(getter)]
        pub fn rotation(&self) -> f64 {
            f64::from(self.transform.rotation)
        }

        #[wasm_bindgen(getter, js_name = scaleX)]
        pub fn scale_x(&self) -> f64 {
            f64::from(self.transform.scale.x)
        }

        #[wasm_bindgen(getter, js_name = scaleY)]
        pub fn scale_y(&self) -> f64 {
            f64::from(self.transform.scale.y)
        }
    }

    #[wasm_bindgen]
    impl WasmCallbackPaint {
        #[wasm_bindgen(getter, js_name = hasFill)]
        pub fn has_fill(&self) -> bool {
            self.fill.is_some()
        }

        #[wasm_bindgen(getter, js_name = fillRed)]
        pub fn fill_red(&self) -> Option<f64> {
            self.fill.map(|color| f64::from(color.red))
        }

        #[wasm_bindgen(getter, js_name = fillGreen)]
        pub fn fill_green(&self) -> Option<f64> {
            self.fill.map(|color| f64::from(color.green))
        }

        #[wasm_bindgen(getter, js_name = fillBlue)]
        pub fn fill_blue(&self) -> Option<f64> {
            self.fill.map(|color| f64::from(color.blue))
        }

        #[wasm_bindgen(getter, js_name = fillAlpha)]
        pub fn fill_alpha(&self) -> Option<f64> {
            self.fill.map(|color| f64::from(color.alpha))
        }

        #[wasm_bindgen(getter, js_name = hasStroke)]
        pub fn has_stroke(&self) -> bool {
            self.stroke.is_some()
        }

        #[wasm_bindgen(getter, js_name = strokeRed)]
        pub fn stroke_red(&self) -> Option<f64> {
            self.stroke.map(|color| f64::from(color.red))
        }

        #[wasm_bindgen(getter, js_name = strokeGreen)]
        pub fn stroke_green(&self) -> Option<f64> {
            self.stroke.map(|color| f64::from(color.green))
        }

        #[wasm_bindgen(getter, js_name = strokeBlue)]
        pub fn stroke_blue(&self) -> Option<f64> {
            self.stroke.map(|color| f64::from(color.blue))
        }

        #[wasm_bindgen(getter, js_name = strokeAlpha)]
        pub fn stroke_alpha(&self) -> Option<f64> {
            self.stroke.map(|color| f64::from(color.alpha))
        }
    }

    impl WasmOrdinaryTransformCompositionBuilder {
        fn push_transform(
            &mut self,
            entering_id: Option<ObjectId>,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            interpolation: noon_core::SemanticTransformInterpolation,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.children.push(OrdinaryCompositionChild::TransformTo {
                entering_id,
                source: source.semantic_mobject().clone(),
                target: target.semantic_mobject().clone(),
                interpolation,
                options: noon_core::AnimationOptions::new()
                    .run_time(child_run_time)
                    .rate_func(rate_function),
            });
            Ok(())
        }

        fn push_rotate(
            &mut self,
            entering_id: Option<ObjectId>,
            target: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            if !angle.is_finite() {
                return Err(js_error("rotation angle must be finite"));
            }
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.children.push(OrdinaryCompositionChild::Rotate {
                entering_id,
                target: target.semantic_mobject().clone(),
                angle,
                options: noon_core::AnimationOptions::new()
                    .run_time(child_run_time)
                    .rate_func(rate_function),
            });
            Ok(())
        }
    }

    #[wasm_bindgen]
    impl WasmOrdinaryTransformCompositionBuilder {
        #[wasm_bindgen(js_name = appendTransformTo)]
        pub fn append_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_transform(
                None,
                source,
                target,
                noon_core::SemanticTransformInterpolation::Affine,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendPointTransformTo)]
        pub fn append_point_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_transform(
                None,
                source,
                target,
                noon_core::SemanticTransformInterpolation::PointCorrespondence,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendEnteringTransformTo)]
        pub fn append_entering_transform_to(
            &mut self,
            object_id: &str,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_transform(
                Some(parse_object_id("object ID", object_id)?),
                source,
                target,
                noon_core::SemanticTransformInterpolation::Affine,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendEnteringPointTransformTo)]
        pub fn append_entering_point_transform_to(
            &mut self,
            object_id: &str,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_transform(
                Some(parse_object_id("object ID", object_id)?),
                source,
                target,
                noon_core::SemanticTransformInterpolation::PointCorrespondence,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendRotate)]
        pub fn append_rotate(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_rotate(
                Some(parse_object_id("object ID", object_id)?),
                target,
                angle,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendBoundRotate)]
        pub fn append_bound_rotate(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_rotate(None, target, angle, child_run_time, rate_function)
        }
    }

    #[wasm_bindgen]
    impl WasmOrdinaryCreateParallelBuilder {
        #[wasm_bindgen(js_name = appendCreate)]
        pub fn append_create(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.children.push((
                id,
                target.semantic_mobject().clone(),
                noon_core::AnimationOptions::new()
                    .run_time(child_run_time)
                    .rate_func(rate_function),
            ));
            Ok(())
        }
    }

    #[wasm_bindgen]
    impl WasmValueTrackerHandle {
        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> u32 {
            self.tracker.node_id().slot()
        }

        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.tracker.node_id().generation()
        }

        #[wasm_bindgen(js_name = detachedValue)]
        pub fn detached_value(&self) -> Result<f64, JsValue> {
            self.tracker.detached_value().map_err(js_error)
        }

        #[wasm_bindgen(js_name = setDetachedValue)]
        pub fn set_detached_value(&self, value: f64) -> Result<(), JsValue> {
            self.tracker.set_detached_value(value).map_err(js_error)
        }
    }

    impl WasmDeclaredAnimationHandle {
        fn declaration_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::DeclaredAnimation, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) {
                return Err(js_error(
                    "animation and live execution context belong to different authoring stores",
                ));
            }
            Ok(&self.declaration)
        }
    }

    impl WasmValueTrackerHandle {
        pub(crate) fn from_tracker(
            tracker: noon::ValueTracker,
            store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Self {
            Self { tracker, store }
        }

        fn tracker_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::ValueTracker, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) || !self.tracker.is_in_store(store) {
                return Err(js_error(
                    "ValueTracker and canonical authoring context belong to different stores",
                ));
            }
            store
                .borrow()
                .semantic_signal_state(self.tracker.node_id())
                .map_err(|error| js_error(error.to_string()))?;
            Ok(&self.tracker)
        }
    }

    impl WasmNativeVectorSignalHandle {
        fn signal_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::NativeVectorSignal, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) || !self.signal.is_in_store(store) {
                return Err(js_error(
                    "native vector signal and canonical authoring context belong to different stores",
                ));
            }
            store
                .borrow()
                .semantic_signal_state(self.signal.node_id())
                .map_err(|error| js_error(error.to_string()))?;
            Ok(&self.signal)
        }
    }

    impl WasmNativeBoolSignalHandle {
        fn signal_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::NativeBoolSignal, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) || !self.signal.is_in_store(store) {
                return Err(js_error(
                    "native bool signal and canonical authoring context belong to different stores",
                ));
            }
            store
                .borrow()
                .semantic_signal_state(self.signal.node_id())
                .map_err(|error| js_error(error.to_string()))?;
            Ok(&self.signal)
        }
    }

    impl WasmTrackerPositionHandle {
        fn position_in(
            &self,
            store: &std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Result<&noon::TrackerPosition, JsValue> {
            if !std::rc::Rc::ptr_eq(&self.store, store) || !self.position.is_in_store(store) {
                return Err(js_error(
                    "tracker position and canonical authoring context belong to different stores",
                ));
            }
            store
                .borrow()
                .semantic_signal_state(self.position.node_id())
                .map_err(|error| js_error(error.to_string()))?;
            Ok(&self.position)
        }
    }

    impl CanonicalAuthoringSceneContext {
        pub(crate) fn with_store(
            store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        ) -> Self {
            Self {
                inner: CanonicalAuthoringScene::with_store(store),
            }
        }
    }

    #[wasm_bindgen]
    impl CanonicalAuthoringSceneContext {
        /// Evaluate one callback-local rotation without mutating authored scene state.
        #[wasm_bindgen(js_name = callbackRotateTransformAboutPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_rotate_transform_about_point(
            &self,
            translation_x: f64,
            translation_y: f64,
            rotation: f64,
            scale_x: f64,
            scale_y: f64,
            angle: f64,
            pivot_x: f64,
            pivot_y: f64,
        ) -> Result<WasmCallbackTransform, JsValue> {
            let transform = noon::rotate_effective_transform_about_point(
                Transform2D {
                    translation: Vec2::new(translation_x as f32, translation_y as f32),
                    rotation: rotation as f32,
                    scale: Vec2::new(scale_x as f32, scale_y as f32),
                },
                angle,
                Vec2::new(pivot_x as f32, pivot_y as f32),
            )
            .map_err(js_error)?;
            Ok(WasmCallbackTransform { transform })
        }

        /// Apply shared Manim `set_color` semantics to callback-local paint.
        #[wasm_bindgen(js_name = callbackPaintSetColor)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_paint_set_color(
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
        ) -> Result<WasmCallbackPaint, JsValue> {
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
                noon::effective_style_with_color(style, red, green, blue, alpha)
                    .map_err(js_error)?,
            ))
        }

        /// Apply shared Manim `set_fill` semantics to callback-local paint.
        #[wasm_bindgen(js_name = callbackPaintSetFill)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_paint_set_fill(
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
        ) -> Result<WasmCallbackPaint, JsValue> {
            let fill =
                callback_color("callback fill", fill_red, fill_green, fill_blue, fill_alpha)?;
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
                (Some(color), Some(opacity)) => noon::effective_style_with_fill(
                    style,
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    opacity,
                ),
                (Some(color), None) => noon::effective_style_with_fill_color(
                    style,
                    f64::from(color.red),
                    f64::from(color.green),
                    f64::from(color.blue),
                    f64::from(color.alpha),
                ),
                (None, Some(opacity)) => noon::effective_style_with_fill_opacity(style, opacity),
                (None, None) => Ok(style),
            }
            .map_err(js_error)?;
            Ok(callback_paint_result(style))
        }

        /// Apply shared Manim `set_stroke(color=...)` semantics to callback-local paint.
        #[wasm_bindgen(js_name = callbackPaintSetStroke)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_paint_set_stroke(
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
        ) -> Result<WasmCallbackPaint, JsValue> {
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
                noon::effective_style_with_stroke_color(
                    style,
                    color_red,
                    color_green,
                    color_blue,
                    color_alpha,
                )
                .map_err(js_error)?,
            ))
        }

        #[wasm_bindgen(js_name = callbackLineTarget)]
        pub fn callback_line_target(
            &self,
            start_x: f64,
            start_y: f64,
            end_x: f64,
            end_y: f64,
        ) -> Result<WasmCallbackLineTarget, JsValue> {
            let point = |name: &str, x: f64, y: f64| {
                if !x.is_finite()
                    || !y.is_finite()
                    || x.abs() > f64::from(f32::MAX)
                    || y.abs() > f64::from(f32::MAX)
                {
                    return Err(js_error(format!(
                        "Line.match_points {name} must be finite f32-compatible coordinates"
                    )));
                }
                Ok(Vec2::new(x as f32, y as f32))
            };
            Ok(WasmCallbackLineTarget {
                start: point("start", start_x, start_y)?,
                end: point("end", end_x, end_y)?,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = callbackMatchLineTransform)]
        pub fn callback_match_line_transform(
            &self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &WasmCallbackLineTarget,
        ) -> Result<WasmCallbackTransform, JsValue> {
            source.id_in_store(self.inner.scene.store(), "Line.match_points source")?;
            if !std::rc::Rc::ptr_eq(self.inner.scene.store(), &target.store) {
                return Err(js_error(
                    "Line.match_points target belongs to another callback context",
                ));
            }
            let transform = source
                .semantic_mobject()
                .line_match_transform(target.start, target.end)
                .map_err(js_error)?;
            Ok(WasmCallbackTransform { transform })
        }

        #[wasm_bindgen(js_name = bindMobject)]
        pub fn bind_mobject(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            self.inner
                .bind_mobject(id, handle.semantic_mobject())
                .map_err(js_error)
        }

        /// Create the scene-owned invisible 2D camera frame and return its opaque semantic handle.
        #[wasm_bindgen(js_name = createCameraFrame)]
        pub fn create_camera_frame(
            &mut self,
            object_id: &str,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            let id = parse_object_id("camera frame object ID", object_id)?;
            self.inner
                .create_camera_frame(id)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createValueTracker)]
        pub fn create_value_tracker(
            &mut self,
            initial: f64,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self.inner.create_value_tracker(initial).map_err(js_error)?;
            Ok(WasmValueTrackerHandle::from_tracker(
                tracker,
                std::rc::Rc::clone(self.inner.scene.store()),
            ))
        }

        #[wasm_bindgen(js_name = associateValueTracker)]
        pub fn associate_value_tracker(
            &mut self,
            tracker: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            self.inner
                .associate_value_tracker(tracker)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = pointerPositionSignal)]
        pub fn pointer_position_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self.inner.pointer_position_signal().map_err(js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = viewportSizeSignal)]
        pub fn viewport_size_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self.inner.viewport_size_signal().map_err(js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = wheelDeltaSignal)]
        pub fn wheel_delta_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self.inner.wheel_delta_signal().map_err(js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = keyStateSignal)]
        pub fn key_state_signal(
            &mut self,
            code: String,
            initial: bool,
        ) -> Result<WasmNativeBoolSignalHandle, JsValue> {
            let signal = self
                .inner
                .key_state_signal(code, initial)
                .map_err(js_error)?;
            Ok(WasmNativeBoolSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = controlSignal)]
        pub fn control_signal(
            &mut self,
            name: String,
            initial: f64,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self.inner.control_signal(name, initial).map_err(js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = pointerDownEvents)]
        pub fn pointer_down_events(
            &mut self,
            button: u32,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self
                .inner
                .pointer_down_events(parse_button(button)?)
                .map_err(js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = wheelEvents)]
        pub fn wheel_events(&mut self) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self.inner.wheel_events().map_err(js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = controlCommitEvents)]
        pub fn control_commit_events(
            &mut self,
            name: String,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self.inner.control_commit_events(name).map_err(js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = bindNativeTranslation)]
        pub fn bind_native_translation(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmNativeVectorSignalHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.store(), "native translation binding")?;
            let signal = signal.signal_in(self.inner.scene.store())?;
            self.inner
                .bind_native_translation(object.semantic_mobject(), signal)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindRotation)]
        pub fn bind_rotation(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.store(), "rotation binding")?;
            let signal = signal.tracker_in(self.inner.scene.store())?;
            self.inner
                .bind_rotation(object.semantic_mobject(), signal)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindOpacity)]
        pub fn bind_opacity(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.store(), "opacity binding")?;
            let signal = signal.tracker_in(self.inner.scene.store())?;
            self.inner
                .bind_opacity(object.semantic_mobject(), signal)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindPresence)]
        pub fn bind_presence(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmNativeBoolSignalHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.store(), "presence binding")?;
            let signal = signal.signal_in(self.inner.scene.store())?;
            self.inner
                .bind_presence(object.semantic_mobject(), signal)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = trackerPosition)]
        pub fn tracker_position(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            direction_x: f64,
            direction_y: f64,
            offset_x: f64,
            offset_y: f64,
        ) -> Result<WasmTrackerPositionHandle, JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            let position = self
                .inner
                .tracker_position(
                    tracker,
                    SemanticVec3::new(direction_x, direction_y, 0.0),
                    SemanticVec3::new(offset_x, offset_y, 0.0),
                )
                .map_err(js_error)?;
            Ok(WasmTrackerPositionHandle {
                position,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = bindTrackerPosition)]
        pub fn bind_tracker_position(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            position: &WasmTrackerPositionHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.store(), "tracker position binding")?;
            let position = position.position_in(self.inner.scene.store())?;
            self.inner
                .bind_tracker_position(object.semantic_mobject(), position)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = declareValueTrackerPlay)]
        pub fn declare_value_tracker_play(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            target: f64,
            duration: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported ValueTracker rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.inner
                .declare_tracker_play(tracker, target, duration, rate_function)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = valueTrackerValue)]
        pub fn value_tracker_value(
            &mut self,
            tracker: &WasmValueTrackerHandle,
        ) -> Result<f64, JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            self.inner.tracker_value(tracker).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setValueTracker)]
        pub fn set_value_tracker(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            value: f64,
        ) -> Result<(), JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            self.inner
                .set_tracker_value(tracker, value)
                .map_err(js_error)
        }

        /// Begin one ordinary scalar tracker play for the shared continuation host.
        #[wasm_bindgen(js_name = beginOrdinaryValueTrackerPlay)]
        pub fn begin_ordinary_value_tracker_play(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            target: f64,
            duration: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.store())?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported ValueTracker rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.inner
                .begin_ordinary_value_tracker_play(tracker, target, duration, rate_function)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = authoredDuration)]
        pub fn authored_duration(&self) -> f64 {
            self.inner.authored_duration()
        }

        #[wasm_bindgen(js_name = authoredWait)]
        pub fn authored_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner.authored_wait(duration).map_err(js_error)
        }

        #[wasm_bindgen(js_name = ordinaryWait)]
        pub fn ordinary_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner.ordinary_wait(duration).map_err(js_error)
        }

        /// Begin one ordinary wait for an async continuation without fast-forwarding it.
        #[wasm_bindgen(js_name = beginOrdinaryWait)]
        pub fn begin_ordinary_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner.begin_ordinary_wait(duration).map_err(js_error)
        }

        #[wasm_bindgen(js_name = beginOrdinaryTransformComposition)]
        pub fn begin_ordinary_transform_composition(
            &self,
            kind: &str,
            composition_run_time: Option<f64>,
            composition_lag_ratio: f64,
            play_run_time: Option<f64>,
        ) -> Result<WasmOrdinaryTransformCompositionBuilder, JsValue> {
            let kind = match kind {
                "parallel" => noon_core::SemanticAnimationCompositionKind::Parallel,
                "sequence" => noon_core::SemanticAnimationCompositionKind::Sequence,
                _ => return Err(js_error("composition kind must be parallel or sequence")),
            };
            let mut composition_options = noon_core::AnimationOptions::new()
                .lag_ratio(composition_lag_ratio)
                .rate_func(noon_core::RateFunction::Linear);
            if let Some(run_time) = composition_run_time {
                composition_options = composition_options.run_time(run_time);
            }
            let mut play_options =
                noon_core::AnimationOptions::new().rate_func(noon_core::RateFunction::Linear);
            if let Some(run_time) = play_run_time {
                play_options = play_options.run_time(run_time);
            }
            Ok(WasmOrdinaryTransformCompositionBuilder {
                kind,
                children: Vec::new(),
                composition_options,
                play_options,
            })
        }

        #[wasm_bindgen(js_name = ordinaryCanPlayComposition)]
        pub fn ordinary_can_play_composition(
            &self,
            candidate: &WasmOrdinaryTransformCompositionBuilder,
        ) -> Result<bool, JsValue> {
            self.inner
                .validate_ordinary_mixed_composition(
                    &candidate.children,
                    candidate.composition_options,
                    candidate.play_options,
                )
                .map(|()| true)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = ordinaryPlayComposition)]
        pub fn ordinary_play_composition(
            &mut self,
            candidate: WasmOrdinaryTransformCompositionBuilder,
        ) -> Result<f64, JsValue> {
            self.inner
                .ordinary_play_mixed_composition(
                    candidate.kind,
                    &candidate.children,
                    candidate.composition_options,
                    candidate.play_options,
                )
                .map_err(js_error)
        }

        /// Consume and activate one inert composition candidate without
        /// advancing it. The returned endpoint belongs to the segment retained
        /// by this context's single execution player.
        #[wasm_bindgen(js_name = beginOrdinaryComposition)]
        pub fn begin_ordinary_composition(
            &mut self,
            candidate: WasmOrdinaryTransformCompositionBuilder,
        ) -> Result<f64, JsValue> {
            self.inner
                .begin_ordinary_mixed_composition(
                    candidate.kind,
                    &candidate.children,
                    candidate.composition_options,
                    candidate.play_options,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = addUpdater)]
        pub fn add_updater(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            callback_id: &str,
            active_from: f64,
            position: Option<u32>,
        ) -> Result<(), JsValue> {
            let callback = parse_callback_id(callback_id)?;
            self.inner
                .add_updater(
                    handle.semantic_mobject(),
                    callback,
                    active_from,
                    position.map(|index| index as usize),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = removeUpdater)]
        pub fn remove_updater(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            callback_id: &str,
            inactive_from: f64,
        ) -> Result<(), JsValue> {
            let callback = parse_callback_id(callback_id)?;
            self.inner
                .remove_updater(handle.semantic_mobject(), callback, inactive_from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = clearUpdaters)]
        pub fn clear_updaters(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            inactive_from: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .clear_updaters(handle.semantic_mobject(), inactive_from)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createExecutionPlayer)]
        pub fn create_execution_player(
            &mut self,
            duration: f64,
            session: u32,
        ) -> Result<crate::SemanticExecutionPlayer, JsValue> {
            self.inner
                .take_execution_player(duration, session)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = beginLiveExecution)]
        pub fn begin_live_execution(&mut self, duration: f64) -> Result<(), JsValue> {
            self.inner
                .live_player(duration)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveHandoffDuration)]
        pub fn live_handoff_duration(&self) -> Option<f64> {
            self.inner.live_handoff_duration()
        }

        #[wasm_bindgen(js_name = liveExecutionOwnership)]
        pub fn live_execution_ownership(&self) -> String {
            self.inner.live_execution_ownership().to_owned()
        }

        #[wasm_bindgen(js_name = queryMobjectLayout)]
        pub fn query_mobject_layout(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<WasmMobjectLayoutObservation, JsValue> {
            let (center_x, center_y, width, height) = self
                .inner
                .mobject_layout(handle.semantic_mobject())
                .map_err(js_error)?;
            Ok(WasmMobjectLayoutObservation {
                center_x,
                center_y,
                width,
                height,
            })
        }

        #[wasm_bindgen(js_name = declareLiveTransformTo)]
        pub fn declare_live_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<WasmDeclaredAnimationHandle, JsValue> {
            source.id_in_store(self.inner.scene.store(), "animation declaration")?;
            target.id_in_store(self.inner.scene.store(), "animation declaration")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            let declaration = self
                .inner
                .declare_live_transform_to(
                    source.semantic_mobject(),
                    target.semantic_mobject(),
                    options,
                )
                .map_err(js_error)?;
            Ok(WasmDeclaredAnimationHandle {
                declaration,
                store: std::rc::Rc::clone(self.inner.scene.store()),
            })
        }

        #[wasm_bindgen(js_name = ordinaryPlayTransformTo)]
        pub fn ordinary_play_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            source.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            target.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .ordinary_play_transform_to(
                    source.semantic_mobject(),
                    target.semantic_mobject(),
                    options,
                )
                .map_err(js_error)
        }

        /// Atomically declare and activate one ordinary transform for an async continuation.
        ///
        /// The retained player keeps the shared segment; this method intentionally does not
        /// advance or complete it.
        #[wasm_bindgen(js_name = beginOrdinaryTransformTo)]
        pub fn begin_ordinary_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            source.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            target.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .begin_ordinary_transform_to(
                    source.semantic_mobject(),
                    target.semantic_mobject(),
                    options,
                )
                .map_err(js_error)
        }

        /// Atomically declare, activate, run, and complete one basic lifecycle fade.
        #[wasm_bindgen(js_name = ordinaryPlayFade)]
        pub fn ordinary_play_fade(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary fade animation")?;
            let direction = parse_fade_direction(direction)?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .ordinary_play_fade(id, target.semantic_mobject(), direction, options)
                .map_err(js_error)
        }

        /// Begin one basic lifecycle fade for an async/synchronous continuation.
        ///
        /// The exact retained player owns the returned segment and must later be
        /// driven and completed through the existing continuation lease.
        #[wasm_bindgen(js_name = beginOrdinaryFade)]
        pub fn begin_ordinary_fade(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary fade animation")?;
            let direction = parse_fade_direction(direction)?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .begin_ordinary_fade(id, target.semantic_mobject(), direction, options)
                .map_err(js_error)
        }

        /// Start one inert flat-parallel Create candidate. It does not mutate the
        /// semantic store or allocate an execution player.
        #[wasm_bindgen(js_name = beginOrdinaryCreateParallel)]
        pub fn begin_ordinary_create_parallel(
            &self,
            play_run_time: Option<f64>,
            rate_function: &str,
        ) -> Result<WasmOrdinaryCreateParallelBuilder, JsValue> {
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let mut play_options = noon_core::AnimationOptions::new().rate_func(rate_function);
            if let Some(run_time) = play_run_time {
                play_options = play_options.run_time(run_time);
            }
            Ok(WasmOrdinaryCreateParallelBuilder {
                children: Vec::new(),
                play_options,
            })
        }

        /// Consume, activate, run, and complete one flat-parallel Create candidate.
        #[wasm_bindgen(js_name = ordinaryPlayCreateParallel)]
        pub fn ordinary_play_create_parallel(
            &mut self,
            candidate: WasmOrdinaryCreateParallelBuilder,
        ) -> Result<f64, JsValue> {
            self.inner
                .ordinary_play_create_parallel(&candidate.children, candidate.play_options)
                .map_err(js_error)
        }

        /// Consume and activate one flat-parallel Create candidate without advancing it.
        #[wasm_bindgen(js_name = beginOrdinaryCreateParallelSegment)]
        pub fn begin_ordinary_create_parallel_segment(
            &mut self,
            candidate: WasmOrdinaryCreateParallelBuilder,
        ) -> Result<f64, JsValue> {
            self.inner
                .begin_ordinary_create_parallel(&candidate.children, candidate.play_options)
                .map_err(js_error)
        }

        /// Atomically declare, activate, run, and complete one single-leaf Create.
        #[wasm_bindgen(js_name = ordinaryPlayCreate)]
        pub fn ordinary_play_create(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary Create animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .ordinary_play_create(id, target.semantic_mobject(), options)
                .map_err(js_error)
        }

        /// Begin one Create for the existing async/synchronous continuation player.
        #[wasm_bindgen(js_name = beginOrdinaryCreate)]
        pub fn begin_ordinary_create(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary Create animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .begin_ordinary_create(id, target.semantic_mobject(), options)
                .map_err(js_error)
        }

        /// Atomically declare, activate, run, and complete one single-leaf Uncreate.
        #[wasm_bindgen(js_name = ordinaryPlayUncreate)]
        pub fn ordinary_play_uncreate(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary Uncreate animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .ordinary_play_uncreate(id, target.semantic_mobject(), options)
                .map_err(js_error)
        }

        /// Begin one Uncreate for the existing async/synchronous continuation player.
        #[wasm_bindgen(js_name = beginOrdinaryUncreate)]
        pub fn begin_ordinary_uncreate(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(self.inner.scene.store(), "ordinary Uncreate animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .begin_ordinary_uncreate(id, target.semantic_mobject(), options)
                .map_err(js_error)
        }

        /// Query shared root membership after an exact fade completion. Python
        /// uses it only to attach/detach its derived wrapper identity.
        #[wasm_bindgen(js_name = liveContainsMobject)]
        pub fn live_contains_mobject(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<bool, JsValue> {
            target.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_contains_mobject(target.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = ordinaryCanPlayTransformTo)]
        pub fn ordinary_can_play_transform_to(
            &self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<bool, JsValue> {
            source.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            target.id_in_store(self.inner.scene.store(), "ordinary affine animation")?;
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            let options = noon_core::AnimationOptions::new()
                .run_time(run_time)
                .rate_func(rate_function);
            self.inner
                .can_ordinary_transform_to(
                    source.semantic_mobject(),
                    target.semantic_mobject(),
                    options,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveTargetEditor)]
        pub fn live_target_editor(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            source.id_in_store(self.inner.scene.store(), "live target editor")?;
            self.inner
                .live_target_editor(source.semantic_mobject())
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = livePlayAnimation)]
        pub fn live_play_animation(
            &mut self,
            animation: &WasmDeclaredAnimationHandle,
        ) -> Result<f64, JsValue> {
            let declaration = animation.declaration_in(self.inner.scene.store())?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_play_animation(declaration)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveWait)]
        pub fn live_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_wait(duration)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveAdvanceSegmentTo)]
        pub fn live_advance_segment_to(&mut self, time: f64) -> Result<bool, JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_advance_segment_to(time)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveCompleteSegment)]
        pub fn live_complete_segment(&mut self) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_complete_segment()
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveEvaluate)]
        pub fn live_evaluate(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_evaluate(time)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = prepareExecutionRun)]
        pub fn prepare_execution_run(&mut self) -> Result<(), JsValue> {
            self.inner.prepare_execution_run().map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetTranslation)]
        pub fn live_set_translation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_translation(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveMoveToPoint)]
        pub fn live_move_to_point(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_move_to_point(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetFill)]
        pub fn live_set_fill(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_fill(handle.semantic_mobject(), red, green, blue, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetFillColor)]
        pub fn live_set_fill_color(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_fill_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveDisableFill)]
        pub fn live_disable_fill(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_disable_fill(handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetFillOpacity)]
        pub fn live_set_fill_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_fill_opacity(handle.semantic_mobject(), opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetColor)]
        pub fn live_set_color(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetStroke)]
        pub fn live_set_stroke(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_stroke(handle.semantic_mobject(), red, green, blue, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetStrokeColor)]
        pub fn live_set_stroke_color(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_stroke_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveDisableStroke)]
        pub fn live_disable_stroke(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_disable_stroke(handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetStrokeOpacity)]
        pub fn live_set_stroke_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_stroke_opacity(handle.semantic_mobject(), opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetOpacity)]
        pub fn live_set_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_opacity(handle.semantic_mobject(), opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetObjectOpacity)]
        pub fn live_set_object_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_object_opacity(handle.semantic_mobject(), opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveAdd)]
        pub fn live_add(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_add_mobject(id, handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveRemove)]
        pub fn live_remove(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_remove_mobject(handle.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveReplaceContent)]
        pub fn live_replace_content(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            target.id_in_store(self.inner.scene.store(), "live execution context")?;
            source.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .live_replace_content(target.semantic_mobject(), source.semantic_mobject())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveShift)]
        pub fn live_shift(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_shift(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetScale)]
        pub fn live_set_scale(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_scale(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveScale)]
        pub fn live_scale(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_scale(handle.semantic_mobject(), x, y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetRotation)]
        pub fn live_set_rotation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_set_rotation(handle.semantic_mobject(), angle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveRotate)]
        pub fn live_rotate(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_rotate(handle.semantic_mobject(), angle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveEffectiveMobject)]
        pub fn live_effective_mobject(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<WasmLiveMobjectState, JsValue> {
            handle.id_in_store(self.inner.scene.store(), "live execution context")?;
            Ok(WasmLiveMobjectState {
                state: self
                    .inner
                    .active_live_player()
                    .map_err(js_error)?
                    .live_effective(handle.semantic_mobject())
                    .map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = returnExecutionPlayer)]
        pub fn return_execution_player(
            &mut self,
            player: crate::SemanticExecutionPlayer,
        ) -> Result<(), JsValue> {
            self.inner.return_execution_player(player).map_err(js_error)
        }

        #[wasm_bindgen(js_name = resumeExecutionPlayer)]
        pub fn resume_execution_player(
            &mut self,
        ) -> Result<crate::SemanticExecutionPlayer, JsValue> {
            self.inner.resume_execution_player().map_err(js_error)
        }

        /// Final publication at the genuine authoring/render worker boundary.
        #[wasm_bindgen(js_name = drainReturnedPublicationJson)]
        pub fn drain_returned_publication_json(&mut self) -> Result<Option<String>, JsValue> {
            self.inner
                .drain_returned_publication_json()
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindGeometry)]
        pub fn bind_geometry(
            &mut self,
            object_id: &str,
            snapshot_json: &str,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let snapshot = parse_json::<ObjectSnapshot>("geometry snapshot", snapshot_json)?;
            self.inner.bind_geometry(id, snapshot).map_err(js_error)
        }

        #[wasm_bindgen(js_name = updateGeometry)]
        pub fn update_geometry(
            &mut self,
            object_id: &str,
            snapshot_json: &str,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let snapshot = parse_json::<ObjectSnapshot>("geometry snapshot", snapshot_json)?;
            self.inner.update_geometry(id, snapshot).map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindText)]
        pub fn bind_text(&mut self, object_id: &str, text_json: &str) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let text = parse_json::<RetainedTextAuthoringSpec>("retained text spec", text_json)?;
            self.inner.bind_text(id, text).map_err(js_error)
        }

        #[wasm_bindgen(js_name = updateText)]
        pub fn update_text(&mut self, object_id: &str, text_json: &str) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            let text = parse_json::<RetainedTextAuthoringSpec>("retained text spec", text_json)?;
            self.inner.update_text(id, text).map_err(js_error)
        }

        pub fn checkpoint(&self) -> u32 {
            u32::try_from(self.inner.checkpoint()).expect("canonical object count fits u32")
        }

        pub fn restore(&mut self, checkpoint: u32) -> Result<(), JsValue> {
            self.inner.restore(checkpoint as usize).map_err(js_error)
        }

        #[wasm_bindgen(js_name = sceneSpecJson)]
        pub fn scene_spec_json(
            &self,
            geometry_tracks_json: &str,
            retained_tracks_json: &str,
            family_animations_json: &str,
            camera_object_id: &str,
        ) -> Result<String, JsValue> {
            let geometry_tracks =
                parse_json::<Vec<TrackDefinition>>("geometry tracks", geometry_tracks_json)?;
            let retained_tracks = parse_json::<Vec<RetainedTrackAuthoringSpec>>(
                "retained tracks",
                retained_tracks_json,
            )?;
            let family_animations = parse_json::<Vec<FamilyAnimationRequest>>(
                "family animations",
                family_animations_json,
            )?;
            let camera_object = if camera_object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("camera object ID", camera_object_id)?)
            };
            let spec = self
                .inner
                .finalize(
                    geometry_tracks,
                    retained_tracks,
                    family_animations,
                    camera_object,
                )
                .map_err(js_error)?;
            serde_json::to_string(&spec).map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, GeometryRef, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticVec3, Transform2D, Vec2,
    };
    use noon_ir::{ObjectSpecContent, TextSpecKind};

    use super::*;

    fn bound_transform_child(
        source: &noon::Mobject,
        target: noon::Mobject,
        options: AnimationOptions,
    ) -> OrdinaryCompositionChild {
        OrdinaryCompositionChild::TransformTo {
            entering_id: None,
            source: source.clone(),
            target,
            interpolation: noon_core::SemanticTransformInterpolation::Affine,
            options,
        }
    }

    #[test]
    fn typed_binding_shares_state_and_root_without_snapshot_synchronization() {
        use std::{cell::RefCell, rc::Rc};
        let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
        let mut context = CanonicalAuthoringScene::with_store(Rc::clone(&store));
        let mut object = noon::Mobject::manim_circle(Rc::clone(&store), 1.0).unwrap();
        let id = object.node_id();
        context.bind_mobject(ObjectId::new(42), &object).unwrap();
        object.shift(2.0, -1.0).unwrap();
        let execution = context.lower_execution().unwrap();
        assert_eq!(
            execution.execution_object_id(id),
            Some(execution.frame().objects[0].id)
        );
        assert_eq!(
            execution.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        let mut other = CanonicalAuthoringScene::with_store(Rc::clone(&store));
        assert!(other.lower_execution().unwrap().frame().objects.is_empty());
        other.bind_mobject(ObjectId::new(0), &object).unwrap();
        context.restore(0).unwrap();
        assert!(context
            .lower_execution()
            .unwrap()
            .frame()
            .objects
            .is_empty());
        assert_eq!(other.lower_execution().unwrap().frame().objects.len(), 1);
        context.bind_mobject(ObjectId::new(42), &object).unwrap();
        assert_eq!(
            context.lower_execution().unwrap().execution_object_id(id),
            execution.execution_object_id(id)
        );
    }

    #[test]
    fn camera_factory_binds_one_scene_local_role_before_execution() {
        use std::{cell::RefCell, rc::Rc};

        let store = Rc::new(RefCell::new(noon_core::SemanticStore::new()));
        let mut first = CanonicalAuthoringScene::with_store(Rc::clone(&store));
        let frame = first.create_camera_frame(ObjectId::new(4)).unwrap();
        assert_eq!(
            first.lower_execution().unwrap().camera().unwrap(),
            noon_core::Camera2DState::default()
        );
        let checkpoint = first.checkpoint();
        let revision = store.borrow().scene_revision();
        assert!(first.create_camera_frame(ObjectId::new(5)).is_err());
        assert_eq!(first.checkpoint(), checkpoint);
        assert_eq!(store.borrow().scene_revision(), revision);
        assert_eq!(
            first.bindings.get(&ObjectId::new(4)),
            Some(&frame.node_id())
        );

        // Store identity is shared, while camera uniqueness is scoped to each scene root.
        let mut second = CanonicalAuthoringScene::with_store(store);
        second.create_camera_frame(ObjectId::new(4)).unwrap();
        assert_eq!(
            second.lower_execution().unwrap().camera().unwrap(),
            noon_core::Camera2DState::default()
        );
    }

    #[test]
    fn typed_binding_rejects_cross_store_collisions_atomically() {
        let mut first = CanonicalAuthoringScene::default();
        let second = CanonicalAuthoringScene::default();
        let local = first.scene.circle(1.0).unwrap();
        let foreign = second.scene.circle(2.0).unwrap();
        assert_eq!(local.node_id(), foreign.node_id());
        let revision = first.scene.store().borrow().scene_revision();
        assert!(first.bind_mobject(ObjectId::new(0), &foreign).is_err());
        assert_eq!(first.checkpoint(), 0);
        assert_eq!(first.scene.store().borrow().scene_revision(), revision);
        first.bind_mobject(ObjectId::new(0), &local).unwrap();
    }

    #[test]
    fn live_membership_uses_the_existing_session_and_registers_detached_handles() {
        let mut context = CanonicalAuthoringScene::default();
        let anchor = context.scene.circle(0.5).unwrap();
        let toggled = context.scene.circle(1.0).unwrap();
        let appended = context.scene.square(1.5).unwrap();
        context.bind_mobject(ObjectId::new(0), &anchor).unwrap();
        context.bind_mobject(ObjectId::new(1), &toggled).unwrap();
        let anchor_slot = context
            .live_player(1.0)
            .unwrap()
            .session_mut_for_test()
            .execution_slot_for_frame_index(0)
            .unwrap();
        assert_eq!(context.ordinary_wait(0.3).unwrap(), 0.3);
        // New detached state must publish through the active session as well.
        let collision = context.live_target_editor(&anchor).unwrap();
        let revision = context.scene.store().borrow().scene_revision();
        assert!(context
            .live_add_mobject(ObjectId::new(0), &collision)
            .is_err());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        assert!(context
            .active_live_player()
            .unwrap()
            .live_effective(&collision)
            .is_err());

        context.live_remove_mobject(&toggled).unwrap();
        assert!(context
            .active_live_player()
            .unwrap()
            .live_effective(&toggled)
            .is_err());
        context
            .live_add_mobject(ObjectId::new(1), &toggled)
            .unwrap();
        context
            .live_add_mobject(ObjectId::new(2), &appended)
            .unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&appended, 2.0, -1.0)
            .unwrap();

        assert_eq!(context.node(ObjectId::new(2)).unwrap(), appended.node_id());
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&anchor)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO
        );
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&appended)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(context.active_live_player().unwrap().time(), 0.3);
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .session_mut_for_test()
                .execution_slot_for_frame_index(0),
            Some(anchor_slot)
        );
    }

    #[test]
    fn live_content_switch_refreshes_handoff_resources_and_preserves_execution_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let target = context.scene.circle(0.5).unwrap();
        let replacement = context.scene.text(noon::Text::new("replacement")).unwrap();
        context.bind_mobject(ObjectId::new(0), &target).unwrap();

        let (execution_id, slot) = {
            let player = context.live_player(1.0).unwrap();
            let bundle =
                crate::RetainedResourceBundle::decode_binary(&player.resource_bundle_bytes())
                    .unwrap();
            assert_eq!(bundle.text_count(), 0);
            let session = player.session_mut_for_test();
            (
                session.execution_object_id(target.node_id()).unwrap(),
                session.execution_slot_for_frame_index(0).unwrap(),
            )
        };

        context.live_replace_content(&target, &replacement).unwrap();
        {
            let player = context.active_live_player().unwrap();
            let session = player.session_mut_for_test();
            assert_eq!(
                session.execution_object_id(target.node_id()),
                Some(execution_id)
            );
            assert_eq!(session.execution_slot_for_frame_index(0), Some(slot));
            assert!(session.frame().objects[0].text().is_some());
        }

        let mut handed_off = context.take_execution_player(1.0, 29).unwrap();
        let bundle =
            crate::RetainedResourceBundle::decode_binary(&handed_off.resource_bundle_bytes())
                .unwrap();
        assert_eq!(bundle.text_count(), 1);
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&handed_off.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.objects[0].object, execution_id);
        assert!(matches!(
            snapshot.objects[0].content,
            crate::TransportObjectContent::Text { .. }
        ));
    }

    fn native_text(source: &str) -> RetainedTextAuthoringSpec {
        RetainedTextAuthoringSpec::native(source, "DejaVu Sans Mono", 48.0, 0.5).unwrap()
    }

    #[test]
    fn mixed_bind_events_define_the_canonical_object_stream_directly() {
        let mut context = CanonicalAuthoringScene::default();
        context
            .bind_geometry(
                ObjectId::new(0),
                ObjectSnapshot::new(GeometryRef::circle(0.5)),
            )
            .unwrap();
        context
            .bind_text(ObjectId::new(1), native_text("A"))
            .unwrap();
        context
            .bind_geometry(
                ObjectId::new(2),
                ObjectSnapshot::new(GeometryRef::rectangle(1.0, 1.0)),
            )
            .unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        assert_eq!(
            spec.objects
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![ObjectId::new(0), ObjectId::new(1), ObjectId::new(2)]
        );
        let ObjectSpecContent::Text(text) = &spec.objects[1].content else {
            panic!("middle object must be source-level text");
        };
        assert_eq!(text.kind, TextSpecKind::Plain);
        assert_eq!(text.source, "A");
    }

    #[test]
    fn native_semantic_text_exports_only_at_the_legacy_callback_boundary() {
        let mut context = CanonicalAuthoringScene::default();
        let mut label = context
            .scene
            .text(
                noon::Text::new("A\nB")
                    .with_font_size(36.0)
                    .with_line_spacing(0.5),
            )
            .unwrap();
        label.shift(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(4), &label).unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        let ObjectSpecContent::Text(text) = &spec.objects[0].content else {
            panic!("shared native Text must derive a legacy-boundary text spec");
        };
        assert_eq!(text.source, "A\nB");
        assert_eq!(text.font_size, 36.0);
        let noon_ir::TextSpecOptions::NativePlain { line_spacing, .. } = &text.options else {
            panic!("native Text export requires native options");
        };
        assert!((*line_spacing - 0.5).abs() < 1.0e-6);
        assert_eq!(spec.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(spec.objects[0].transform.scale, Vec2::ONE);
    }

    #[test]
    fn typst_and_math_typst_export_source_kind_and_effective_presentation() {
        let mut context = CanonicalAuthoringScene::default();
        let label = context
            .scene
            .typst(
                noon::Typst::new("*Noon*")
                    .with_font_size(72.0)
                    .color(noon_core::YELLOW)
                    .shift(Vec2::new(2.0, -1.0)),
            )
            .unwrap();
        let equation = context
            .scene
            .math_typst(
                noon::MathTypst::new("frac(x, 2)")
                    .set_opacity(0.5)
                    .shift(Vec2::new(-1.0, 0.5)),
            )
            .unwrap();
        context.bind_mobject(ObjectId::new(4), &label).unwrap();
        context.bind_mobject(ObjectId::new(5), &equation).unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        let ObjectSpecContent::Text(label) = &spec.objects[0].content else {
            panic!("Typst export must remain source-level text");
        };
        assert_eq!(label.kind, TextSpecKind::Typst);
        assert_eq!(label.source, "*Noon*");
        assert_eq!(label.font_size, noon::DEFAULT_TYPST_FONT_SIZE);
        assert_eq!(spec.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        let scale = spec.objects[0].transform.scale;
        assert!((scale.x - 1.5).abs() < 1.0e-6 && (scale.y - 1.5).abs() < 1.0e-6);
        assert_eq!(spec.objects[0].style.fill, Some(noon_core::YELLOW));

        let ObjectSpecContent::Text(equation) = &spec.objects[1].content else {
            panic!("MathTypst export must remain source-level text");
        };
        assert_eq!(equation.kind, TextSpecKind::MathTypst);
        assert_eq!(equation.source, "frac(x, 2)");
        assert_eq!(equation.font_size, noon::DEFAULT_TYPST_FONT_SIZE);
        assert_eq!(spec.objects[1].transform.translation, Vec2::new(-1.0, 0.5));
        assert_eq!(spec.objects[1].transform.scale, Vec2::ONE);
        assert_eq!(spec.objects[1].style.opacity, 0.5);
    }

    #[test]
    fn updates_preserve_slots_and_append_checkpoint_restore_reclaims_failed_binds() {
        let mut context = CanonicalAuthoringScene::default();
        let first = ObjectId::new(0);
        context
            .bind_geometry(first, ObjectSnapshot::new(GeometryRef::circle(0.5)))
            .unwrap();
        let checkpoint = context.checkpoint();
        context
            .bind_text(ObjectId::new(1), native_text("temporary"))
            .unwrap();
        // Checkpoint rollback is intentionally append-only: an update to an
        // existing slot remains visible after the failed bind is reclaimed.
        context
            .update_geometry(first, ObjectSnapshot::new(GeometryRef::circle(0.75)))
            .unwrap();
        context.restore(checkpoint).unwrap();
        let exported = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), None)
            .unwrap();
        let ObjectSpecContent::Geometry(geometry) = &exported.objects[0].content else {
            panic!("first object must remain geometry-backed");
        };
        assert_eq!(geometry, &GeometryRef::circle(0.75));

        let mut replacement = ObjectSnapshot::new(GeometryRef::circle(1.0));
        replacement.transform = Transform2D {
            translation: Vec2::new(2.0, -1.0),
            ..Transform2D::default()
        };
        context.update_geometry(first, replacement).unwrap();
        context
            .bind_geometry(
                ObjectId::new(1),
                ObjectSnapshot::new(GeometryRef::rectangle(2.0, 1.0)),
            )
            .unwrap();

        let spec = context
            .finalize(Vec::new(), Vec::new(), Vec::new(), Some(first))
            .unwrap();
        assert_eq!(spec.objects.len(), 2);
        assert_eq!(spec.objects[0].id, first);
        assert_eq!(spec.objects[0].transform.translation, Vec2::new(2.0, -1.0));
        assert_eq!(spec.camera_object, Some(first));
    }

    #[test]
    fn content_domain_cannot_change_after_binding() {
        let mut context = CanonicalAuthoringScene::default();
        let id = ObjectId::new(7);
        context.bind_text(id, native_text("stable")).unwrap();
        let error = context
            .update_geometry(id, ObjectSnapshot::new(GeometryRef::circle(1.0)))
            .unwrap_err();
        assert!(error.contains("not geometry-backed"));
    }

    #[test]
    fn live_runtime_survives_normal_execution_handoff_and_renderer_recovery() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let animation = context
            .declare_live_transform_to(&circle, &target, options)
            .unwrap();
        let store = std::rc::Rc::clone(context.scene.store());

        {
            let player = context.live_player(2.0).unwrap();
            let end = player.live_play_animation(&animation).unwrap();
            assert_eq!(end, 2.0);
            assert!(player.live_wait(0.5).is_err());
            assert!(!player.live_advance_segment_to(1.0).unwrap());

            assert!(player.live_set_translation(&circle, 100.0, 0.0).is_err());
            assert_eq!(
                store
                    .borrow()
                    .semantic_object_state_checked(circle.node_id())
                    .unwrap()
                    .transform
                    .translation,
                SemanticVec3::new(0.0, 0.0, 0.0)
            );
            assert_eq!(
                player
                    .live_effective(&circle)
                    .unwrap()
                    .transform
                    .translation
                    .x,
                2.0
            );
        }

        context.prepare_execution_run().unwrap();
        let mut handed_off = context.take_execution_player(2.0, 17).unwrap();
        assert_eq!(
            handed_off
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            2.0
        );
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&handed_off.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.session, 17);
        assert_eq!(snapshot.objects[0].transform.translation.x, 2.0);

        context.return_execution_player(handed_off).unwrap();
        let mut recovered = context.take_execution_player(2.0, 18).unwrap();
        assert_eq!(
            recovered
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            2.0
        );
        let recovery_snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&recovered.initial_delta_json().unwrap()).unwrap();
        assert_eq!(recovery_snapshot.session, 18);
        assert_eq!(recovery_snapshot.objects[0].transform.translation.x, 2.0);
        assert!(context.live_player(2.0).is_err());

        assert!(!recovered.live_advance_segment_to(2.0).unwrap());
        recovered.live_complete_segment().unwrap();
        recovered.live_set_translation(&circle, 100.0, 0.0).unwrap();
        assert_eq!(
            recovered
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            100.0,
        );
    }

    #[test]
    fn callback_registration_keeps_target_editor_authored_before_player_bootstrap() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context
            .add_updater(&circle, HostCallbackId::new(9), 0.0, None)
            .unwrap();
        let revision = context.scene.store().borrow().scene_revision();

        let mut target = context.live_target_editor(&circle).unwrap();
        target.set_translation(2.0, -1.0).unwrap();

        assert!(context.live_player.is_none());
        assert!(
            context.scene.store().borrow().scene_revision().get() > revision.get(),
            "the detached authored target must be published without bootstrapping a player"
        );
        assert_eq!(
            target.state().unwrap().transform.translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
    }

    #[test]
    fn target_editor_rejects_a_transferred_player_without_authored_fallback() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context.live_player(1.0).unwrap();
        let player = context.take_execution_player(1.0, 17).unwrap();
        let revision = context.scene.store().borrow().scene_revision();

        assert!(context.live_target_editor(&circle).is_err());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        assert_eq!(context.live_execution_ownership(), "transferred");

        context.return_execution_player(player).unwrap();
        let target = context.live_target_editor(&circle).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
        assert!(target.validate().is_ok());
    }

    #[test]
    fn ordinary_affine_barriers_reuse_the_runtime_and_accept_a_late_detached_target() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let mut first_target = circle.target_editor().unwrap();
        first_target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);

        assert_eq!(
            context
                .ordinary_play_transform_to(&circle, &first_target, options)
                .unwrap(),
            2.0
        );
        assert_eq!(context.authored_duration(), 2.0);
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0)
        );

        assert_eq!(context.ordinary_wait(1.0).unwrap(), 3.0);
        context
            .active_live_player()
            .unwrap()
            .live_shift(&circle, 1.0, 0.0)
            .unwrap();
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(3.0, -1.0)
        );

        // Python's ordinary `Transform` creates this target after the runtime
        // exists. The target and its edit publish through that same runtime, so
        // the second activation neither rebuilds nor resets the live session.
        let second_target = context.live_target_editor(&circle).unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&second_target, 5.0, -1.0)
            .unwrap();
        let second_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        assert_eq!(
            context
                .ordinary_play_transform_to(&circle, &second_target, second_options)
                .unwrap(),
            4.0
        );
        assert_eq!(context.authored_duration(), 4.0);
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(5.0, -1.0)
        );
    }

    #[test]
    fn ordinary_composition_candidate_preflight_is_read_only_before_atomic_play() {
        let mut context = CanonicalAuthoringScene::default();
        let mut left = context.scene.circle(0.4).unwrap();
        left.set_translation(-2.0, 0.0).unwrap();
        let mut right = context.scene.circle(0.4).unwrap();
        right.set_translation(2.0, 0.0).unwrap();
        let mut left_target = left.target_editor().unwrap();
        left_target.set_translation(-2.0, 1.0).unwrap();
        let mut right_target = right.target_editor().unwrap();
        right_target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &left).unwrap();
        context.bind_mobject(ObjectId::new(1), &right).unwrap();
        let child = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let children = [
            bound_transform_child(&left, left_target, child),
            bound_transform_child(&right, right_target, child),
        ];
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);
        let revision = context.scene.store().borrow().scene_revision();

        context
            .validate_ordinary_mixed_composition(&children, composition, play)
            .unwrap();
        assert!(context.live_player.is_none());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        let mut unsupported_target = right.target_editor().unwrap();
        unsupported_target.set_stroke_width(3.0).unwrap();
        let unsupported = [bound_transform_child(&right, unsupported_target, child)];
        let revision = context.scene.store().borrow().scene_revision();
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &unsupported,
                composition,
                play,
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &children,
                    composition,
                    play,
                )
                .unwrap(),
            2.0
        );
        let player = context.active_live_player().unwrap();
        assert_eq!(
            player.live_effective(&left).unwrap().transform.translation,
            Vec2::new(-2.0, 1.0)
        );
        assert_eq!(
            player.live_effective(&right).unwrap().transform.translation,
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn mixed_sequence_keeps_rotate_before_transform_in_one_shared_segment() {
        let mut context = CanonicalAuthoringScene::default();
        let rotating = context.scene.square(0.8).unwrap();
        let moving = context.scene.circle(0.4).unwrap();
        let mut moving_target = moving.target_editor().unwrap();
        moving_target.set_translation(2.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &rotating).unwrap();
        context.bind_mobject(ObjectId::new(1), &moving).unwrap();
        let child = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [
            OrdinaryCompositionChild::Rotate {
                entering_id: None,
                target: rotating.clone(),
                angle: std::f64::consts::PI,
                options: child,
            },
            bound_transform_child(&moving, moving_target, child),
        ];
        let composition = AnimationOptions::new()
            .lag_ratio(1.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);

        let end = context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Sequence,
                &children,
                composition,
                play,
            )
            .unwrap();
        assert_eq!(end, 2.0);
        let player = context.active_live_player().unwrap();
        player.live_advance_segment_to(0.5).unwrap();
        assert_eq!(
            player
                .live_effective(&moving)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO
        );
        player.live_advance_segment_to(1.5).unwrap();
        assert_eq!(
            player
                .live_effective(&moving)
                .unwrap()
                .transform
                .translation,
            Vec2::new(1.0, 0.0)
        );
        assert!(
            (player.live_effective(&rotating).unwrap().transform.rotation - std::f32::consts::PI)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn begun_composition_stays_unadvanced_and_uses_required_callback_barriers() {
        let mut context = CanonicalAuthoringScene::default();
        let mut left = context.scene.circle(0.4).unwrap();
        left.set_translation(-2.0, 0.0).unwrap();
        let mut right = context.scene.circle(0.4).unwrap();
        right.set_translation(2.0, 0.0).unwrap();
        let mut left_target = left.target_editor().unwrap();
        left_target.set_translation(-2.0, 1.0).unwrap();
        let mut right_target = right.target_editor().unwrap();
        right_target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &left).unwrap();
        context.bind_mobject(ObjectId::new(1), &right).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(left.node_id(), HostCallbackId::new(7), 0.0, None);
        callbacks.add_updater(left.node_id(), HostCallbackId::new(8), 0.0, None);
        callbacks
            .apply(&mut context.scene.store().borrow_mut())
            .unwrap();
        let child = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let children = [
            bound_transform_child(&left, left_target, child),
            bound_transform_child(&right, right_target, child),
        ];
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);

        let revision = context.scene.store().borrow().scene_revision();
        let endpoint_only_error = context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .unwrap_err();
        assert!(endpoint_only_error.contains("needs an asynchronous continuation"));
        assert!(context.live_player.is_none());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        let end_time = context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .unwrap();
        assert_eq!(end_time, 2.0);
        let player = context.active_live_player().unwrap();
        assert_eq!(
            player.time(),
            0.0,
            "activation must not advance the segment"
        );
        assert!(player.has_pending_live_segment());
        assert_eq!(
            player.live_effective(&left).unwrap().transform.translation,
            Vec2::new(-2.0, 0.0)
        );

        player.live_segment_wake(1_000.0).unwrap();
        let initial = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
        let initial_phase: serde_json::Value =
            serde_json::from_str(&initial.callback_phase_json().unwrap()).unwrap();
        assert_eq!(initial_phase["time"], serde_json::json!(0.0));
        assert_eq!(
            initial_phase["invocations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|invocation| invocation["callback_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["7", "8"]
        );
        player
            .commit_callback_phase_json(
                &serde_json::json!({
                    "token": initial_phase["token"].clone(),
                    "writes": [],
                })
                .to_string(),
            )
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(1_000.0).unwrap();
        assert!(ready.callback_phase_json().is_none());
        assert!(!ready.reached_endpoint());
        assert_eq!(player.time(), 0.0);

        let endpoint = player.live_drive_segment_from_wall_time(3_000.0).unwrap();
        let endpoint_phase: serde_json::Value =
            serde_json::from_str(&endpoint.callback_phase_json().unwrap()).unwrap();
        assert_eq!(endpoint_phase["time"], serde_json::json!(2.0));
        player
            .commit_callback_phase_json(
                &serde_json::json!({
                    "token": endpoint_phase["token"].clone(),
                    "writes": [],
                })
                .to_string(),
            )
            .unwrap();
        let ready = player.live_drive_segment_from_wall_time(3_000.0).unwrap();
        assert!(ready.reached_endpoint());
        player.live_complete_segment().unwrap();
        assert_eq!(
            player.live_effective(&left).unwrap().transform.translation,
            Vec2::new(-2.0, 1.0)
        );
        assert_eq!(
            player.live_effective(&right).unwrap().transform.translation,
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn begun_composition_duplicate_driver_leaves_no_first_player_and_valid_retry_works() {
        let mut context = CanonicalAuthoringScene::default();
        let source = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &source).unwrap();
        let mut first_target = source.target_editor().unwrap();
        first_target.set_translation(1.0, 0.0).unwrap();
        let mut second_target = source.target_editor().unwrap();
        second_target.set_translation(2.0, 0.0).unwrap();
        let child = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [
            bound_transform_child(&source, first_target, child),
            bound_transform_child(&source, second_target, child),
        ];
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);
        let revision = context.scene.store().borrow().scene_revision();

        assert!(context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .is_err());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        assert!(context.live_player.is_none());
        assert_eq!(context.live_execution_ownership(), "none");

        let mut valid_target = source.target_editor().unwrap();
        valid_target.set_translation(3.0, 0.0).unwrap();
        assert_eq!(
            context
                .begin_ordinary_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &[bound_transform_child(&source, valid_target, child)],
                    composition,
                    play,
                )
                .unwrap(),
            1.0
        );
        let player = context.active_live_player().unwrap();
        assert!(player.has_pending_live_segment());
        assert_eq!(player.time(), 0.0);
        assert_eq!(
            player
                .live_effective(&source)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO
        );
    }

    #[test]
    fn rejected_composition_preserves_an_exact_returned_player() {
        let mut context = CanonicalAuthoringScene::default();
        let source = context.scene.circle(0.4).unwrap();
        let mut setup_target = source.target_editor().unwrap();
        setup_target.set_translation(1.0, 0.0).unwrap();
        let mut first_target = source.target_editor().unwrap();
        first_target.set_translation(2.0, 0.0).unwrap();
        let mut second_target = source.target_editor().unwrap();
        second_target.set_translation(3.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &source).unwrap();
        let child = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        context
            .ordinary_play_transform_to(&source, &setup_target, child)
            .unwrap();
        let player = context.take_execution_player(1.0, 73).unwrap();
        context.return_execution_player(player).unwrap();
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);
        let revision = context.scene.store().borrow().scene_revision();
        let (publication, frame, handoff_duration) = {
            let player = context.live_player.as_mut().unwrap();
            let handoff_duration = player.live_handoff_duration();
            let session = player.session_mut_for_test();
            (
                session.publication_context(),
                session.frame().clone(),
                handoff_duration,
            )
        };

        assert!(context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &[
                    bound_transform_child(&source, first_target, child),
                    bound_transform_child(&source, second_target, child),
                ],
                composition,
                play,
            )
            .is_err());
        assert_eq!(context.live_execution_ownership(), "returned");
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        let player = context.live_player.as_mut().unwrap();
        assert_eq!(player.live_handoff_duration(), handoff_duration);
        assert!(!player.has_pending_live_segment());
        let session = player.session_mut_for_test();
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }

    #[test]
    fn ordinary_composition_candidate_surfaces_foreign_and_stale_handles_before_bootstrap() {
        let mut context = CanonicalAuthoringScene::default();
        let source = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &source).unwrap();
        let mut unsupported = source.target_editor().unwrap();
        unsupported.set_stroke_width(3.0).unwrap();
        let foreign = noon::Scene::new().circle(0.4).unwrap();
        let child = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);

        assert!(context
            .validate_ordinary_mixed_composition(
                &[
                    bound_transform_child(&source, unsupported.clone(), child),
                    bound_transform_child(&source, foreign, child),
                ],
                composition,
                play,
            )
            .unwrap_err()
            .contains("another authoring store"));
        assert!(context.live_player.is_none());

        let stale = source.target_editor().unwrap();
        let mut removal = SemanticMutationTransaction::new();
        removal.remove_node(stale.node_id());
        removal
            .apply(&mut context.scene.store().borrow_mut())
            .unwrap();
        assert!(context
            .validate_ordinary_mixed_composition(
                &[
                    bound_transform_child(&source, unsupported, child),
                    bound_transform_child(&source, stale, child),
                ],
                composition,
                play,
            )
            .is_err());
        assert!(context.live_player.is_none());
    }

    #[test]
    fn ordinary_layout_query_uses_effective_runtime_and_rejects_transferred_reads() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, -2.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let animation = context
            .declare_live_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (0.0, 0.0, 2.0, 2.0)
        );
        {
            let player = context.live_player(2.0).unwrap();
            player.live_play_animation(&animation).unwrap();
            player.live_advance_segment_to(1.0).unwrap();
        }
        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (2.0, -1.0, 2.0, 2.0)
        );

        let player = context.take_execution_player(2.0, 17).unwrap();
        assert!(context
            .mobject_layout(&circle)
            .unwrap_err()
            .contains("running in the semantic engine"));
        context.return_execution_player(player).unwrap();
        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (2.0, -1.0, 2.0, 2.0)
        );
    }

    #[test]
    fn live_advancement_anchors_presentation_and_handoff_cannot_rewind_it() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let animation = context
            .declare_live_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        {
            let player = context.live_player(1.0).unwrap();
            assert_eq!(player.live_play_animation(&animation).unwrap(), 2.0);
            let (publication, frame) = {
                let session = player.session_mut_for_test();
                (session.publication_context(), session.frame().clone())
            };
            assert!(player.live_advance_segment_to(f64::NAN).is_err());
            let session = player.session_mut_for_test();
            assert_eq!(session.publication_context(), publication);
            assert_eq!(session.frame(), &frame);
            assert!(player.is_playing());
            assert!(!player.live_advance_segment_to(2.0).unwrap());
            assert_eq!(player.time(), 2.0);
            assert!(!player.is_playing());
            player.live_complete_segment().unwrap();
            assert_eq!(player.live_wait(0.25).unwrap(), 2.25);
            assert!(player.is_playing());
            assert!(player.live_advance_segment_to(2.25).unwrap());
            assert_eq!(player.time(), 2.25);
            assert!(!player.is_playing());
            player.live_complete_segment().unwrap();
        }
        assert_eq!(context.live_handoff_duration(), Some(2.25));

        let error = context.take_execution_player(2.0, 16).err().unwrap();
        assert!(error.contains("shorter than live handoff duration 2.25"));
        assert_eq!(context.live_handoff_duration(), Some(2.25));

        let duration = context.live_handoff_duration().unwrap();
        let mut handed_off = context.take_execution_player(duration, 17).unwrap();
        handed_off.tick_delta_json(4_000.0).unwrap();
        assert_eq!(handed_off.time(), 2.25);
        assert_eq!(
            handed_off
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            4.0
        );
        handed_off.seek_delta_json(0.5).unwrap();
        assert_eq!(handed_off.time(), 0.5);
        context.return_execution_player(handed_off).unwrap();

        // Presentation may scrub back, but the completed logical continuation
        // remains the authoritative handoff boundary for the next attachment.
        assert_eq!(context.live_handoff_duration(), Some(2.25));
        let duration = context.live_handoff_duration().unwrap();
        let mut recovered = context.take_execution_player(duration, 18).unwrap();
        recovered.seek_delta_json(2.0).unwrap();
        assert_eq!(recovered.time(), 2.0);
        assert_eq!(
            recovered
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            4.0
        );
    }

    #[test]
    fn new_authoring_run_refreshes_a_returned_runtime_after_direct_scene_edits() {
        let mut context = CanonicalAuthoringScene::default();
        let mut circle = context.scene.circle(1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();

        let initial = context.take_execution_player(1.0, 17).unwrap();
        context.return_execution_player(initial).unwrap();

        // A direct authoring operation happens outside the returned runtime.
        circle.shift(3.0, -1.0).unwrap();
        circle.scale(2.0, 0.5).unwrap();
        circle.set_fill(0.25, 0.5, 0.75, 0.8).unwrap();

        // Ordinary authoring reads observe the shared store without relowering
        // or treating the dormant returned runtime as active live authority.
        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (3.0, -1.0, 4.0, 1.0)
        );
        assert!(context.live_player.is_some());

        // The next registration boundary lowers precisely one fresh runtime.
        context.prepare_execution_run().unwrap();
        assert!(context.live_player.is_none());

        let mut rerun = context.take_execution_player(1.0, 18).unwrap();
        let effective = rerun.live_effective(&circle).unwrap();
        assert_eq!(effective.transform.translation, Vec2::new(3.0, -1.0));
        assert_eq!(effective.transform.scale, Vec2::new(2.0, 0.5));
        assert_eq!(effective.style.fill.unwrap().alpha, 0.8);
        let snapshot: crate::RetainedExecutionDeltaEnvelope =
            serde_json::from_str(&rerun.initial_delta_json().unwrap()).unwrap();
        assert_eq!(snapshot.session, 18);
        assert_eq!(snapshot.objects[0].transform.translation.x, 3.0);
    }

    #[test]
    fn returned_final_publication_preserves_runtime_time_and_encoder() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        assert!(context.drain_returned_publication_json().is_err());
        context.begin_ordinary_wait(0.25).unwrap();
        let mut player = context.take_execution_player(1.0, 17).unwrap();
        player.initial_delta_json().unwrap();
        assert!(context.drain_returned_publication_json().is_err());
        player.live_advance_segment_to(0.25).unwrap();
        player.live_complete_segment().unwrap();
        player.drain_delta_json().unwrap();
        context.return_execution_player(player).unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&circle, 1.0, 0.0)
            .unwrap();
        let json = context.drain_returned_publication_json().unwrap().unwrap();
        let delta: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(delta["session"], 17);
        assert!(delta["sequence"].as_u64().unwrap() > 0);
        assert_ne!(delta["snapshot"], true);
        assert_eq!(context.live_execution_ownership(), "returned");
        assert_eq!(context.live_handoff_duration(), Some(0.25));
        assert!(context.drain_returned_publication_json().unwrap().is_none());
        context.begin_ordinary_wait(0.25).unwrap();
        assert!(context.drain_returned_publication_json().is_err());
    }

    #[test]
    fn live_execution_ownership_is_derived_from_the_retained_player_lifecycle() {
        let mut context = CanonicalAuthoringScene::default();
        assert_eq!(context.live_execution_ownership(), "none");

        context.live_player(1.0).unwrap();
        assert_eq!(context.live_execution_ownership(), "active");

        let player = context.take_execution_player(1.0, 17).unwrap();
        assert_eq!(context.live_execution_ownership(), "transferred");

        context.return_execution_player(player).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
        assert!(context.resume_execution_player().is_err());

        context.begin_ordinary_wait(0.25).unwrap();
        let mut resumed = context.resume_execution_player().unwrap();
        assert_eq!(context.live_execution_ownership(), "transferred");
        resumed.live_advance_segment_to(0.25).unwrap();
        resumed.live_complete_segment().unwrap();
        context.return_execution_player(resumed).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");

        context.begin_ordinary_wait(0.0).unwrap();
        let mut zero_wait = context.resume_execution_player().unwrap();
        assert!(zero_wait.live_wait(1.0).is_err());
        let wake = zero_wait.live_segment_wake(1_000.0).unwrap();
        assert_eq!(wake.cadence(), "timer");
        assert_eq!(wake.timer_after_milliseconds(), Some(0.0));
        assert!(zero_wait
            .live_drive_segment_from_wall_time(1_000.0)
            .unwrap()
            .reached_endpoint());
        zero_wait.live_complete_segment().unwrap();
        assert!(!zero_wait.has_pending_live_segment());
        assert!(zero_wait.live_segment_wake(1_000.0).is_err());
        context.return_execution_player(zero_wait).unwrap();
        assert!(context.resume_execution_player().is_err());
    }

    #[test]
    fn callback_continuation_can_resume_but_terminal_failure_cannot_reenter_source() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(circle.node_id(), HostCallbackId::new(7), 0.0, None);
        callbacks
            .apply(&mut context.scene.store().borrow_mut())
            .unwrap();

        context.begin_ordinary_wait(0.25).unwrap();
        let leased = context.take_execution_player(0.25, 91).unwrap();
        context.return_execution_player(leased).unwrap();
        let mut resumed = context.resume_execution_player().unwrap();
        resumed.live_segment_wake(1_000.0).unwrap();
        let drive = resumed.live_drive_segment_from_wall_time(1_000.0).unwrap();
        let phase = drive.callback_phase_json().unwrap();
        resumed.fail_callback_phase_json(&phase).unwrap();
        assert!(resumed.live_complete_segment().is_err());

        context.return_execution_player(resumed).unwrap();
        let error = context.resume_execution_player().err().unwrap();
        assert!(error.contains("callback progression terminated"));
        assert_eq!(context.live_execution_ownership(), "returned");
    }

    #[test]
    fn begun_ordinary_transform_leases_and_returns_the_same_unadvanced_player() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();

        let end_time = context
            .begin_ordinary_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(end_time, 2.0);
        assert_eq!(context.active_live_player().unwrap().time(), 0.0);

        let mut player = context.take_execution_player(end_time, 71).unwrap();
        assert_eq!(context.live_execution_ownership(), "transferred");
        assert_eq!(
            player.live_segment_wake(1_000.0).unwrap().cadence(),
            "animation_frame"
        );
        assert!(!player
            .live_drive_segment_from_wall_time(2_000.0)
            .unwrap()
            .reached_endpoint());
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(1.0, -0.5)
        );
        assert!(player
            .live_drive_segment_from_wall_time(3_000.0)
            .unwrap()
            .reached_endpoint());
        player.live_complete_segment().unwrap();

        context.return_execution_player(player).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (2.0, -1.0, f64::from(0.8_f32), f64::from(0.8_f32))
        );

        let next_target = context.live_target_editor(&circle).unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_shift(&next_target, 2.0, 0.0)
            .unwrap();
        let next_endpoint = context
            .begin_ordinary_transform_to(
                &circle,
                &next_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
        let mut resumed = context.resume_execution_player().unwrap();
        resumed.live_advance_segment_to(next_endpoint).unwrap();
        resumed.live_complete_segment().unwrap();
        assert_eq!(
            resumed
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(4.0, -1.0)
        );
        context.return_execution_player(resumed).unwrap();
    }

    #[test]
    fn direct_authoring_cannot_hide_a_stale_active_live_runtime() {
        let mut context = CanonicalAuthoringScene::default();
        let mut circle = context.scene.circle(1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context.live_player(1.0).unwrap();

        circle.shift(3.0, -1.0).unwrap();

        let query_error = context.mobject_layout(&circle).unwrap_err();
        assert!(
            query_error.contains("has not been published"),
            "{query_error}"
        );
        let run_error = context.prepare_execution_run().unwrap_err();
        assert!(run_error.contains("authored scene changed while live execution is active"));
        assert!(context.live_player.is_some());
        assert!(!context.live_player_returned);
    }

    #[test]
    fn callback_occurrences_publish_before_session_lowering_and_reject_late_edits() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();

        context
            .add_updater(&circle, HostCallbackId::new(12), 0.0, None)
            .unwrap();
        let registrations = context
            .scene
            .store()
            .borrow()
            .semantic_updater_registrations(circle.node_id())
            .unwrap()
            .to_vec();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].callback(), HostCallbackId::new(12));
        assert_eq!(registrations[0].active_from(), 0.0);

        context.live_player(2.0).unwrap();
        let error = context
            .add_updater(&circle, HostCallbackId::new(13), 1.0, None)
            .unwrap_err();
        assert!(error.contains("before canonical execution begins"));
    }

    #[test]
    fn scalar_tracker_uses_the_authored_cursor_then_one_live_session() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let position = context
            .tracker_position(
                &tracker,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(-2.0, 0.0, 0.0),
            )
            .unwrap();
        context.bind_tracker_position(&circle, &position).unwrap();
        assert_eq!(
            context
                .declare_tracker_play(&tracker, 4.0, 2.0, RateFunction::Linear)
                .unwrap(),
            2.0
        );

        // Before bootstrap, the Rust-authored cursor selects the shared track
        // endpoint; the language wrapper owns no scalar value or cursor.
        assert_eq!(context.tracker_value(&tracker).unwrap(), 4.0);
        assert!(context.set_tracker_value(&tracker, 3.0).is_err());

        let player = context.live_player(2.0).unwrap();
        assert!(player.live_evaluate(2.25).is_err());
        assert_eq!(player.time(), 0.0);
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 0.0);

        player.live_evaluate(1.0).unwrap();
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 2.0);
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO
        );

        player.live_evaluate(2.0).unwrap();
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 4.0);
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, 0.0)
        );
        assert!(player.live_set_signal(&tracker, 3.0).is_err());
    }

    #[test]
    fn scalar_tracker_creation_uses_the_owned_live_session() {
        let mut context = CanonicalAuthoringScene::default();
        context.live_player(1.0).unwrap();

        let tracker = context.create_value_tracker(1.25).unwrap();
        assert_eq!(context.tracker_value(&tracker).unwrap(), 1.25);
        assert!(context
            .scene
            .store()
            .borrow()
            .is_semantic_signal_scoped(context.scene.root(), tracker.node_id()));

        let revision = context.scene.store().borrow().scene_revision();
        assert!(context.create_value_tracker(f64::MAX).is_err());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        assert_eq!(context.tracker_value(&tracker).unwrap(), 1.25);

        let leased = context.take_execution_player(1.0, 91).unwrap();
        assert!(context.create_value_tracker(2.0).is_err());
        context.return_execution_player(leased).unwrap();
        let returned_tracker = context.create_value_tracker(2.0).unwrap();
        assert_eq!(context.tracker_value(&returned_tracker).unwrap(), 2.0);
    }

    #[test]
    fn detached_tracker_association_uses_authored_and_live_publication_paths() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker =
            noon::ValueTracker::detached(std::rc::Rc::clone(context.scene.store()), 1.25).unwrap();
        context.associate_value_tracker(&tracker).unwrap();
        assert_eq!(context.tracker_value(&tracker).unwrap(), 1.25);

        context.live_player(1.0).unwrap();
        let live_tracker =
            noon::ValueTracker::detached(std::rc::Rc::clone(context.scene.store()), 2.5).unwrap();
        context.associate_value_tracker(&live_tracker).unwrap();
        assert_eq!(context.tracker_value(&live_tracker).unwrap(), 2.5);

        let invalid =
            noon::ValueTracker::detached(std::rc::Rc::clone(context.scene.store()), f64::MAX)
                .unwrap();
        let revision = context.scene.store().borrow().scene_revision();
        assert!(context.associate_value_tracker(&invalid).is_err());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
        assert_eq!(invalid.detached_value().unwrap(), f64::MAX);

        let foreign = CanonicalAuthoringScene::default();
        let foreign_tracker =
            noon::ValueTracker::detached(std::rc::Rc::clone(foreign.scene.store()), 3.0).unwrap();
        assert!(context.associate_value_tracker(&foreign_tracker).is_err());
        assert_eq!(foreign_tracker.detached_value().unwrap(), 3.0);
    }

    #[test]
    fn scalar_tracker_wait_keeps_the_canonical_authoring_cursor() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker = context.create_value_tracker(0.0).unwrap();
        context
            .declare_tracker_play(&tracker, 4.0, 2.0, RateFunction::Linear)
            .unwrap();
        assert_eq!(context.authored_wait(1.0).unwrap(), 3.0);
        assert_eq!(
            context
                .declare_tracker_play(&tracker, 6.0, 1.0, RateFunction::Linear)
                .unwrap(),
            4.0
        );
        let timeline = context
            .scene
            .store()
            .borrow()
            .semantic_signal_state(tracker.node_id())
            .unwrap()
            .scalar_timeline()
            .to_vec();
        let noon_core::SemanticScalarSignalTimelineEntry::Track(second) = timeline[1] else {
            panic!("expected a second scalar track")
        };
        assert_eq!(second.timing().start_time, 3.0);
        assert_eq!(context.authored_duration(), 4.0);
    }

    #[test]
    fn ordinary_scalar_begin_is_provisional_and_postcompletion_set_persists() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let position = context
            .tracker_position(
                &tracker,
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(-2.0, 0.0, 0.0),
            )
            .unwrap();
        context.bind_tracker_position(&circle, &position).unwrap();
        let revision = context.scene.store().borrow().scene_revision();

        assert!(context
            .begin_ordinary_value_tracker_play(&tracker, f64::MAX, 2.0, RateFunction::Linear,)
            .is_err());
        assert!(context.live_player.is_none());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        let end = context
            .begin_ordinary_value_tracker_play(&tracker, 2.0, 2.0, RateFunction::Linear)
            .unwrap();
        assert_eq!(end, 2.0);
        let player = context.active_live_player().unwrap();
        player.live_advance_segment_to(1.0).unwrap();
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 1.0);
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            -1.0
        );
        player.live_advance_segment_to(2.0).unwrap();
        player.live_complete_segment().unwrap();
        player.live_set_signal(&tracker, 3.0).unwrap();
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 3.0);
        assert_eq!(
            player
                .live_effective(&circle)
                .unwrap()
                .transform
                .translation
                .x,
            1.0
        );
        assert_eq!(
            context
                .scene
                .store()
                .borrow()
                .semantic_input_scalar_value_at(tracker.node_id(), 1.0)
                .unwrap(),
            1.0
        );
    }

    #[test]
    fn ordinary_affine_play_rejects_a_pre_execution_scalar_cursor_without_bootstrapping() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker = context.create_value_tracker(0.0).unwrap();
        context
            .declare_tracker_play(&tracker, 4.0, 2.0, RateFunction::Linear)
            .unwrap();
        let circle = context.scene.circle(0.4).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let revision = context.scene.store().borrow().scene_revision();

        let error = context
            .ordinary_play_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap_err();
        assert!(error.contains("cannot follow pre-execution canonical timing"));
        assert!(context.live_player.is_none());
        assert_eq!(context.authored_duration(), 2.0);
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
    }

    #[test]
    fn ordinary_fade_reuses_one_live_session_and_preserves_readd_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);

        let fade_in_end = context
            .begin_ordinary_fade(
                ObjectId::new(0),
                &circle,
                SemanticFadeDirection::In,
                options,
            )
            .unwrap();
        assert!(context.live_contains_mobject(&circle).unwrap());
        {
            let player = context.active_live_player().unwrap();
            assert_eq!(player.time(), 0.0);
            player.live_advance_segment_to(fade_in_end).unwrap();
            player.live_complete_segment().unwrap();
        }
        assert!(context.live_contains_mobject(&circle).unwrap());

        let fade_out_end = context
            .begin_ordinary_fade(
                ObjectId::new(0),
                &circle,
                SemanticFadeDirection::Out,
                options,
            )
            .unwrap();
        {
            let player = context.active_live_player().unwrap();
            player.live_advance_segment_to(fade_out_end).unwrap();
            player.live_complete_segment().unwrap();
        }
        assert!(!context.live_contains_mobject(&circle).unwrap());

        // The original derived ObjectId re-enters through the shared session;
        // no replacement semantic handle or second runtime is allocated.
        context.live_add_mobject(ObjectId::new(0), &circle).unwrap();
        assert!(context.live_contains_mobject(&circle).unwrap());
    }

    #[test]
    fn ordinary_create_is_atomic_and_rejects_foreign_or_second_membership() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let revision = context.scene.store().borrow().scene_revision();
        assert!(context
            .begin_ordinary_create(
                ObjectId::new(0),
                &circle,
                AnimationOptions::new().run_time(f64::NAN),
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        let foreign = CanonicalAuthoringScene::default()
            .scene
            .circle(0.4)
            .unwrap();
        assert!(context
            .begin_ordinary_create(
                ObjectId::new(0),
                &foreign,
                AnimationOptions::new().run_time(1.0),
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);

        let end = context
            .begin_ordinary_create(
                ObjectId::new(0),
                &circle,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(end, 1.0);
        assert!(context.live_contains_mobject(&circle).unwrap());
        assert!(context
            .begin_ordinary_create(
                ObjectId::new(1),
                &circle,
                AnimationOptions::new().run_time(1.0),
            )
            .is_err());
        assert_eq!(context.bindings.len(), 1);
        assert_eq!(context.identities.len(), 1);
    }

    #[test]
    fn ordinary_uncreate_releases_membership_and_preserves_same_handle_reentry() {
        let mut context = CanonicalAuthoringScene::default();
        let square = context.scene.square(2.0).unwrap();
        let id = ObjectId::new(0);
        let end = context
            .begin_ordinary_uncreate(id, &square, AnimationOptions::new())
            .unwrap();
        assert_eq!(end, 1.0);
        assert!(context.live_contains_mobject(&square).unwrap());
        let player = context.active_live_player().unwrap();
        player.live_advance_segment_to(end).unwrap();
        player.live_complete_segment().unwrap();
        assert!(!context.live_contains_mobject(&square).unwrap());
        context.live_add_mobject(id, &square).unwrap();
        assert!(context.live_contains_mobject(&square).unwrap());
        assert_eq!(context.identities.get(&square.node_id()), Some(&id));
    }

    #[test]
    fn ordinary_parallel_create_commits_bindings_only_after_shared_admission() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let square = context.scene.square(0.8).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth);
        let end = context
            .begin_ordinary_create_parallel(
                &[
                    (ObjectId::new(0), circle.clone(), options),
                    (ObjectId::new(1), square.clone(), options),
                ],
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(end, 1.0);
        assert_eq!(
            context.bindings.get(&ObjectId::new(0)),
            Some(&circle.node_id())
        );
        assert_eq!(
            context.bindings.get(&ObjectId::new(1)),
            Some(&square.node_id())
        );
        assert_eq!(
            context.identities.get(&circle.node_id()),
            Some(&ObjectId::new(0))
        );
        assert_eq!(
            context.identities.get(&square.node_id()),
            Some(&ObjectId::new(1))
        );
        let player = context.active_live_player().unwrap();
        assert_eq!(player.time(), 0.0);
        player.live_advance_segment_to(end).unwrap();
        player.live_complete_segment().unwrap();
        assert!(context.live_contains_mobject(&circle).unwrap());
        assert!(context.live_contains_mobject(&square).unwrap());
    }

    #[test]
    fn failed_parallel_create_keeps_all_derived_bindings_and_first_player_absent() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let square = context.scene.square(0.8).unwrap();
        let revision = context.scene.store().borrow().scene_revision();

        assert!(context
            .begin_ordinary_create_parallel(
                &[
                    (
                        ObjectId::new(0),
                        circle.clone(),
                        AnimationOptions::new().run_time(1.0)
                    ),
                    (
                        ObjectId::new(1),
                        square.clone(),
                        AnimationOptions::new().run_time(f64::NAN)
                    ),
                ],
                AnimationOptions::new().run_time(1.0),
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert_eq!(context.scene.store().borrow().scene_revision(), revision);
    }

    #[test]
    fn failed_first_fade_does_not_install_a_player_or_derived_binding() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let before = context.scene.store().borrow().scene_revision();
        let id = ObjectId::new(0);

        assert!(context
            .begin_ordinary_fade(
                id,
                &circle,
                SemanticFadeDirection::In,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.5),
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert!(!context.live_player_returned);
        assert!(!context.live_player_transferred);
        assert!(!context.bindings.contains_key(&id));
        assert!(!context.identities.contains_key(&circle.node_id()));
        assert_eq!(context.scene.store().borrow().scene_revision(), before);

        // The failed provisional player did not poison the ordinary path.
        context
            .begin_ordinary_fade(
                id,
                &circle,
                SemanticFadeDirection::In,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert!(context.live_player.is_some());
        assert_eq!(context.bindings.get(&id), Some(&circle.node_id()));
    }

    #[test]
    fn unsupported_first_fade_entry_keeps_context_unbootstrapped() {
        let mut context = CanonicalAuthoringScene::default();
        let text = context
            .scene
            .text(noon::Text::new("unsupported entry"))
            .unwrap();
        let before = context.scene.store().borrow().scene_revision();
        let id = ObjectId::new(0);

        assert!(context
            .begin_ordinary_fade(
                id,
                &text,
                SemanticFadeDirection::In,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .is_err());
        assert!(context.live_player.is_none());
        assert!(!context.bindings.contains_key(&id));
        assert!(!context.identities.contains_key(&text.node_id()));
        assert_eq!(context.scene.store().borrow().scene_revision(), before);
    }

    #[test]
    fn native_signal_declarations_bind_through_the_canonical_scene() {
        let mut context = CanonicalAuthoringScene::default();
        let square = context.scene.square(0.9).unwrap();
        context.bind_mobject(ObjectId::new(0), &square).unwrap();

        let pointer = context.pointer_position_signal().unwrap();
        context.bind_native_translation(&square, &pointer).unwrap();
        let opacity = context.control_signal("opacity".into(), 1.0).unwrap();
        context.bind_opacity(&square, &opacity).unwrap();
        let clicks = context.pointer_down_events(0).unwrap();
        context.bind_rotation(&square, &clicks).unwrap();
        let visible = context.key_state_signal("Space".into(), false).unwrap();
        context.bind_presence(&square, &visible).unwrap();
        context.viewport_size_signal().unwrap();
        context.wheel_delta_signal().unwrap();
        context.wheel_events().unwrap();
        context.control_commit_events("opacity".into()).unwrap();

        let foreign = CanonicalAuthoringScene::default();
        assert!(foreign.bind_native_translation(&square, &pointer).is_err());

        context.live_player(1.0).unwrap();
        assert!(context.pointer_position_signal().is_err());
        assert!(context.bind_opacity(&square, &opacity).is_err());
    }
}

use crate::authoring_error::AuthoringFailure;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(any(target_arch = "wasm32", test))]
mod player_ownership;
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) use player_ownership::PlayerReturnError;
#[cfg(any(target_arch = "wasm32", test))]
use player_ownership::{PlayerOwnership, RejectedPlayerReturn};
#[cfg(test)]
mod ownership_tests;
#[cfg(test)]
mod wait_bootstrap_tests;

use noon_core::ObjectId;
#[cfg(any(target_arch = "wasm32", test))]
use noon_core::{HostCallbackId, SemanticFadeDirection, SemanticMutationTransaction, SemanticVec3};

#[derive(Clone)]
enum OwnedSceneMembershipMember {
    Mobject {
        wrapper_id: Option<ObjectId>,
        handle: noon::Mobject,
    },
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Family(noon::MobjectFamily),
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneMembershipBatchKind {
    Add,
    Remove,
    Clear,
    Replace,
}

struct SceneMembershipBatch {
    kind: SceneMembershipBatchKind,
    members: Vec<OwnedSceneMembershipMember>,
    bindings: Vec<(ObjectId, noon::Mobject)>,
}

#[cfg(any(target_arch = "wasm32", test))]
impl SceneMembershipBatch {
    fn create_family(
        &self,
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        z_index: f64,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        if self.kind != SceneMembershipBatchKind::Add {
            return Err("family creation requires an add batch".into());
        }
        noon::MobjectFamily::create_with_z_index(store, &self.family_members()?, z_index)
            .map_err(AuthoringFailure::from)
    }

    fn edit_family(&self, family: &noon::MobjectFamily) -> Result<Vec<bool>, AuthoringFailure> {
        let members = self.family_members()?;
        match self.kind {
            SceneMembershipBatchKind::Add => {
                family.add_many(&members).map_err(AuthoringFailure::from)
            }
            SceneMembershipBatchKind::Remove => {
                family.remove_many(&members).map_err(AuthoringFailure::from)
            }
            _ => Err("family membership requires add or remove".into()),
        }
    }

    fn family_members(&self) -> Result<Vec<noon::MobjectFamilyMember<'_>>, String> {
        if !self.bindings.is_empty() {
            return Err("family membership does not accept scene binding reservations".into());
        }
        self.members
            .iter()
            .map(|member| match member {
                OwnedSceneMembershipMember::Mobject {
                    wrapper_id: None,
                    handle,
                } => Ok(handle.into()),
                OwnedSceneMembershipMember::Mobject {
                    wrapper_id: Some(_),
                    ..
                } => Err("family membership does not accept wrapper binding IDs".into()),
                OwnedSceneMembershipMember::Family(family) => Ok(family.into()),
            })
            .collect()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone)]
enum OrdinaryCompositionChild {
    FocusOn {
        focus: noon::FocusOnOptions,
        options: noon_core::AnimationOptions,
    },
    TransformTo {
        entering_id: Option<ObjectId>,
        source: noon::Mobject,
        target: noon::Mobject,
        interpolation: noon_core::SemanticTransformInterpolation,
        options: noon_core::AnimationOptions,
    },
    FamilyTransformTo {
        source: noon::MobjectFamily,
        target_state: noon::MobjectFamily,
        options: noon_core::AnimationOptions,
    },
    Indicate {
        target: noon::Mobject,
        indication: noon::IndicateOptions,
        options: noon_core::AnimationOptions,
    },
    FamilyIndicate {
        target: noon::MobjectFamily,
        indication: noon::IndicateOptions,
        options: noon_core::AnimationOptions,
    },
    DrawBorderThenFill {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        outline: noon::DrawBorderThenFillOptions,
        options: noon_core::AnimationOptions,
    },
    PassingFlash {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        time_width: f64,
        options: noon_core::AnimationOptions,
    },
    FamilyDrawBorderThenFill {
        target: noon::MobjectFamily,
        entering: Vec<(ObjectId, noon::Mobject)>,
        outline: noon::DrawBorderThenFillOptions,
        options: noon_core::AnimationOptions,
    },
    FamilySubsetDisplay {
        target: noon::MobjectFamily,
        entering: Vec<(ObjectId, noon::Mobject)>,
        mode: noon::SubsetDisplayMode,
        options: noon_core::AnimationOptions,
    },
    TextWrite {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        reverse_member_order: bool,
        options: noon_core::AnimationOptions,
    },
    FamilyTextWrite {
        target: noon::MobjectFamily,
        entering: Vec<(ObjectId, noon::Mobject)>,
        reverse_member_order: bool,
        options: noon_core::AnimationOptions,
    },
    TextReveal {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        reverse: bool,
        options: noon_core::AnimationOptions,
    },
    FamilyReveal {
        target: noon::MobjectFamily,
        entering: Vec<(ObjectId, noon::Mobject)>,
        reverse: bool,
        options: noon_core::AnimationOptions,
    },
    Rotate {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        angle: f64,
        pivot: Option<noon::ManimRotationPivot>,
        options: noon_core::AnimationOptions,
    },
    ValueTracker {
        tracker: noon::ValueTracker,
        target: f64,
        options: noon_core::AnimationOptions,
    },
    Wait {
        duration: f64,
    },
    Add {
        entering_id: ObjectId,
        target: noon::Mobject,
        options: noon_core::AnimationOptions,
    },
    Fade {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        direction: SemanticFadeDirection,
        endpoint: noon::FadeEndpoint,
        options: noon_core::AnimationOptions,
    },
    FamilyFade {
        target: noon::MobjectFamily,
        entering: Vec<(ObjectId, noon::Mobject)>,
        direction: SemanticFadeDirection,
        options: noon_core::AnimationOptions,
    },
    Create {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        options: noon_core::AnimationOptions,
    },
    Uncreate {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        options: noon_core::AnimationOptions,
    },
    AffineLifecycle {
        entering_id: Option<ObjectId>,
        target: noon::Mobject,
        direction: noon::AffineLifecycleDirection,
        endpoint: noon::AffineLifecycleEndpoint,
        options: noon_core::AnimationOptions,
    },
    Composition {
        kind: noon_core::SemanticAnimationCompositionKind,
        children: Vec<OrdinaryCompositionChild>,
        options: noon_core::AnimationOptions,
    },
}

/// One scene family in the worker's shared semantic store.
/// Frontend bindings retain identity only; all object content remains Rust-owned.
pub struct CanonicalAuthoringScene {
    scene: noon::Scene,
    bindings: BTreeMap<ObjectId, noon_core::SemanticNodeId>,
    identities: BTreeMap<noon_core::SemanticNodeId, ObjectId>,
    #[cfg(any(target_arch = "wasm32", test))]
    player_ownership: PlayerOwnership,
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
        let scene = noon::Scene::with_integration_store(semantics);
        Self {
            scene,
            bindings: BTreeMap::new(),
            identities: BTreeMap::new(),
            #[cfg(any(target_arch = "wasm32", test))]
            player_ownership: PlayerOwnership::Unstarted,
        }
    }

    pub fn bind_mobject(
        &mut self,
        id: ObjectId,
        handle: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Mobject {
                wrapper_id: Some(id),
                handle: handle.clone(),
            }],
            bindings: vec![(id, handle.clone())],
        })
    }

    /// Create and bind this scene's camera frame through the shared semantic transaction.
    ///
    /// The returned handle is only an alias of the scene-owned semantic identity. It carries no
    /// camera state or frontend allocation authority.
    pub fn create_camera_frame(&mut self, id: ObjectId) -> Result<noon::Mobject, AuthoringFailure> {
        if self.bindings.contains_key(&id) {
            return Err(format!("canonical object {} is already bound", id.get()).into());
        }
        let frame = self.scene.camera_frame().map_err(AuthoringFailure::from)?;
        let node = frame.node_id();
        debug_assert!(!self.identities.contains_key(&node));
        self.bindings.insert(id, node);
        self.identities.insert(node, id);
        Ok(frame)
    }

    #[cfg(test)]
    fn members(&self) -> Result<Vec<noon_core::SemanticNodeId>, String> {
        self.scene
            .integration_store()
            .borrow()
            .node(self.scene.root())
            .map(|node| node.members().to_vec())
            .ok_or_else(|| "semantic scene root is no longer live".into())
    }

    pub fn lower_execution(&self) -> Result<noon::ExecutionSession, String> {
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
        self.require_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_updater(handle.node_id(), callback, active_from, position);
        self.publish_updater_edit(transaction)
    }

    /// Close the first open occurrence for this host callback at an exclusive
    /// authored time. The store validates the complete mutation before commit.
    #[cfg(any(target_arch = "wasm32", test))]
    fn remove_updater(
        &mut self,
        handle: &noon::Mobject,
        callback: HostCallbackId,
        inactive_from: f64,
    ) -> Result<(), String> {
        self.require_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_updater(handle.node_id(), callback, inactive_from);
        self.publish_updater_edit(transaction)
    }

    /// Close every open callback occurrence on this target at an exclusive
    /// authored time through the owning live session when execution has begun.
    #[cfg(any(target_arch = "wasm32", test))]
    fn clear_updaters(&mut self, handle: &noon::Mobject, inactive_from: f64) -> Result<(), String> {
        self.require_updater_target(handle)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.clear_updaters(handle.node_id(), inactive_from);
        self.publish_updater_edit(transaction)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn publish_updater_edit(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<(), String> {
        if self.player_ownership.is_transferred() {
            return Err("return the active execution player before editing updaters".into());
        }
        if let Some(player) = self.player_ownership.local_mut() {
            return player.live_edit_updaters(transaction);
        }
        transaction
            .apply(&mut self.scene.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_updater_target(&self, handle: &noon::Mobject) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
            return Err("mobject belongs to another authoring store".into());
        }
        handle.validate().map_err(|error| error.to_string())?;
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
        if let Some(player) = self.player_ownership.local_mut() {
            player.set_loop_duration(duration)?;
        } else {
            self.player_ownership = PlayerOwnership::Active(self.build_live_player(duration, 0)?);
        }
        Ok(self
            .player_ownership
            .local_mut()
            .expect("live player initialized"))
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn build_live_player(
        &self,
        duration: f64,
        transport_session: u32,
    ) -> Result<crate::SemanticExecutionPlayer, String> {
        crate::SemanticExecutionPlayer::from_live_session(
            self.lower_execution()?,
            std::rc::Rc::clone(self.scene.integration_store()),
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
        self.player_ownership
            .prepare_for_run(self.scene.integration_store().borrow().scene_revision())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn returned_player_is_stale(&self) -> bool {
        self.player_ownership
            .returned_is_stale(self.scene.integration_store().borrow().scene_revision())
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
        if self.player_ownership.is_transferred() {
            return Err("live execution session is running in the semantic engine".into());
        }
        self.player_ownership
            .local_mut()
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
        if self.player_ownership.is_unstarted() {
            self.live_player(duration)?;
        }
        self.active_live_player()
    }

    /// Report only the Rust-owned lifecycle of this context's retained player.
    /// Python uses this derived observation to choose its wrapper dispatch; it
    /// never records or advances lifecycle state itself.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_execution_ownership(&self) -> &'static str {
        self.player_ownership.browser_name()
    }

    /// Route bound observations through the single owner of current execution.
    /// Detached handles are queried directly by their language wrapper.
    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_observation<T, E: Into<AuthoringFailure>>(
        &mut self,
        handle: &noon::Mobject,
        authored: impl FnOnce(&noon::Mobject) -> Result<T, E>,
        effective: impl FnOnce(
            &mut crate::SemanticExecutionPlayer,
            &noon::Mobject,
        ) -> Result<T, AuthoringFailure>,
    ) -> Result<T, AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("live execution session is running in the semantic engine".into());
        }
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
            return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                .with_message("mobject belongs to another authoring store"));
        }
        handle.validate().map_err(AuthoringFailure::from)?;
        if !self.identities.contains_key(&handle.node_id()) {
            return Err("mobject is not bound to this canonical Scene".into());
        }
        if !self.returned_player_is_stale() {
            if let Some(player) = self.player_ownership.local_mut() {
                return effective(player, handle);
            }
        }
        authored(handle).map_err(Into::into)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_layout(
        &mut self,
        handle: &noon::Mobject,
    ) -> Result<(f64, f64, f64, f64), AuthoringFailure> {
        self.mobject_observation(handle, authored_mobject_layout, |player, handle| {
            let observed = player.live_effective_layout(handle)?;
            Ok((
                observed.center.0,
                observed.center.1,
                observed.width,
                observed.height,
            ))
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_path_query(
        &mut self,
        handle: &noon::Mobject,
    ) -> Result<noon::PathQuery, AuthoringFailure> {
        self.mobject_observation(
            handle,
            noon::Mobject::path_query,
            crate::SemanticExecutionPlayer::live_effective_path_query,
        )
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_line_endpoints(
        &mut self,
        handle: &noon::Mobject,
    ) -> Result<noon::ManimLineEndpoints, AuthoringFailure> {
        self.mobject_observation(
            handle,
            noon::Mobject::manim_line_endpoints,
            crate::SemanticExecutionPlayer::live_effective_line_endpoints,
        )
    }

    #[cfg(target_arch = "wasm32")]
    fn mobject_fill_color(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<Option<noon_core::Color>, AuthoringFailure> {
        self.mobject_observation(
            target,
            noon::Mobject::fill_color,
            crate::SemanticExecutionPlayer::live_effective_fill_color,
        )
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_stroke_color(
        &mut self,
        target: &noon::Mobject,
    ) -> Result<Option<noon_core::Color>, AuthoringFailure> {
        self.mobject_observation(
            target,
            noon::Mobject::stroke_color,
            crate::SemanticExecutionPlayer::live_effective_stroke_color,
        )
    }
    #[cfg(target_arch = "wasm32")]
    fn mobject_stroke_width(&mut self, target: &noon::Mobject) -> Result<f64, AuthoringFailure> {
        self.mobject_observation(
            target,
            noon::Mobject::stroke_width,
            crate::SemanticExecutionPlayer::live_effective_stroke_width,
        )
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_color(
        &mut self,
        handle: &noon::Mobject,
    ) -> Result<noon_core::Color, AuthoringFailure> {
        self.mobject_observation(
            handle,
            noon::Mobject::manim_color,
            crate::SemanticExecutionPlayer::live_effective_manim_color,
        )
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_fill_opacity(&mut self, handle: &noon::Mobject) -> Result<f64, AuthoringFailure> {
        self.mobject_observation(handle, noon::Mobject::fill_opacity, |player, handle| {
            // Reject resource paints before reading their lowered scalar projection.
            handle.fill_opacity().map_err(AuthoringFailure::from)?;
            Ok(player.live_effective(handle)?.fill_opacity())
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn mobject_stroke_opacity(&mut self, handle: &noon::Mobject) -> Result<f64, AuthoringFailure> {
        self.mobject_observation(handle, noon::Mobject::stroke_opacity, |player, handle| {
            // Reject resource paints before reading their lowered scalar projection.
            handle.stroke_opacity().map_err(AuthoringFailure::from)?;
            Ok(player.live_effective(handle)?.stroke_opacity())
        })
    }

    /// Inert bounds-dependent construction observes this runtime for bound targets,
    /// while fresh detached targets retain their shared authored layout.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_underline(
        &mut self,
        handle: &noon::Mobject,
        buff: f64,
    ) -> Result<noon::ManimGeometryOptions, AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("live execution session is running in the semantic engine".into());
        }
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
            return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                .with_message("mobject belongs to another authoring store"));
        }
        handle.validate().map_err(AuthoringFailure::from)?;
        let mut bounds = handle
            .layout_bounds()
            .map_err(AuthoringFailure::from)?
            .ok_or("Underline target has no layout bounds")?;
        if self.identities.contains_key(&handle.node_id()) {
            let (x, y, width, height) = self.mobject_layout(handle)?;
            bounds = noon_core::Bounds2D64 {
                min_x: x - width * 0.5,
                max_x: x + width * 0.5,
                min_y: y - height * 0.5,
                max_y: y + height * 0.5,
            };
        }
        noon::ManimGeometryOptions::underline(bounds, buff).map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_pre_execution_signal_authoring(&self) -> Result<(), String> {
        if !self.player_ownership.is_unstarted() {
            return Err(
                "signal declarations and bindings must be authored before canonical execution begins"
                    .into(),
            );
        }
        Ok(())
    }

    /// Create one scalar signal in this context's shared semantic store.
    #[cfg(any(target_arch = "wasm32", test))]
    fn create_value_tracker(
        &mut self,
        initial: f64,
    ) -> Result<noon::ValueTracker, AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("live execution session is running in the semantic engine".into());
        }
        match self.player_ownership.local_mut() {
            Some(player) => player.live_value_tracker(initial),
            None => self
                .scene
                .value_tracker(initial)
                .map_err(AuthoringFailure::from),
        }
    }

    /// Associate one store-owned detached tracker with this Scene.
    #[cfg(any(target_arch = "wasm32", test))]
    fn associate_value_tracker(
        &mut self,
        tracker: &noon::ValueTracker,
    ) -> Result<(), AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("live execution session is running in the semantic engine".into());
        }
        match self.player_ownership.local_mut() {
            Some(player) => player.live_associate_value_tracker(tracker),
            None => self
                .scene
                .associate_value_tracker(tracker)
                .map_err(AuthoringFailure::from),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn pointer_position_signal(&self) -> Result<noon::NativeVectorSignal, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .pointer_position_signal()
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn viewport_size_signal(&self) -> Result<noon::NativeVectorSignal, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .viewport_size_signal()
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn wheel_delta_signal(&self) -> Result<noon::NativeVectorSignal, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .wheel_delta_signal()
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn key_state_signal(
        &self,
        code: String,
        initial: bool,
    ) -> Result<noon::NativeBoolSignal, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .key_state_signal(code, initial)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn control_signal(
        &self,
        name: String,
        initial: f64,
    ) -> Result<noon::ValueTracker, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .control_signal(name, initial)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn pointer_down_events(&self, button: u8) -> Result<noon::ValueTracker, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .pointer_down_events(button)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn wheel_events(&self) -> Result<noon::ValueTracker, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene.wheel_events().map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn control_commit_events(&self, name: String) -> Result<noon::ValueTracker, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .control_commit_events(name)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_native_translation(
        &self,
        object: &noon::Mobject,
        signal: &noon::NativeVectorSignal,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .bind_native_translation(object, signal)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_rotation(
        &self,
        object: &noon::Mobject,
        signal: &noon::ValueTracker,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .bind_rotation(object, signal)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_opacity(
        &self,
        object: &noon::Mobject,
        signal: &noon::ValueTracker,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .bind_opacity(object, signal)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_presence(
        &self,
        object: &noon::Mobject,
        signal: &noon::NativeBoolSignal,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .bind_presence(object, signal)
            .map_err(AuthoringFailure::from)
    }

    /// Build only the common `offset + tracker * direction` semantic expression.
    #[cfg(any(target_arch = "wasm32", test))]
    fn tracker_position(
        &self,
        tracker: &noon::ValueTracker,
        direction: SemanticVec3,
        offset: SemanticVec3,
    ) -> Result<noon::TrackerPosition, AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .position_from_tracker(tracker, direction, offset)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn bind_tracker_position(
        &self,
        object: &noon::Mobject,
        position: &noon::TrackerPosition,
    ) -> Result<(), AuthoringFailure> {
        self.require_pre_execution_signal_authoring()?;
        self.scene
            .bind_position(object, position)
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn tracker_value(&mut self, tracker: &noon::ValueTracker) -> Result<f64, AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("semantic execution session is running in the semantic engine".into());
        }
        match self.player_ownership.local_mut() {
            Some(player) => player.live_effective_signal(tracker),
            None => self
                .scene
                .value_tracker_value(tracker)
                .map_err(AuthoringFailure::from),
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn set_tracker_value(
        &mut self,
        tracker: &noon::ValueTracker,
        value: f64,
    ) -> Result<(), AuthoringFailure> {
        if self.player_ownership.is_transferred() {
            return Err("semantic execution session is running in the semantic engine".into());
        }
        match self.player_ownership.local_mut() {
            Some(player) => player.live_set_signal(tracker, value),
            None => self
                .scene
                .set_value(tracker, value)
                .map_err(AuthoringFailure::from),
        }
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
    /// An ordinary wait always belongs to the live execution cursor, including
    /// the first operation in an empty scene. Pre-execution scalar declaration
    /// keeps using the explicit `authored_wait` entry point.
    #[cfg(any(target_arch = "wasm32", test))]
    fn ordinary_wait(&mut self, duration: f64) -> Result<f64, AuthoringFailure> {
        if self.player_ownership.local().is_some()
            && self.active_live_player()?.has_required_callbacks()
        {
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
            .ok_or_else(|| AuthoringFailure::from("live execution player has no handoff duration"))
    }

    /// Begin one ordinary wait without advancing it.
    ///
    /// This exists for the async worker continuation path. The returned endpoint is derived
    /// from the player-owned segment; no Python or JavaScript cursor is created.
    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_wait(&mut self, duration: f64) -> Result<f64, AuthoringFailure> {
        if self.player_ownership.is_unstarted() {
            if self.scene.time() != 0.0 {
                return Err(
                    "ordinary asynchronous wait cannot follow pre-execution canonical timing"
                        .into(),
                );
            }
            // A wait has no animation extent, but the presentation clock still needs a
            // positive valid range before its session-derived deadline replaces it.
            let mut player = self.build_live_player(duration.max(1.0), 0)?;
            let end_time = player.live_wait(duration)?;
            // Shared admission is fallible. Publish the first runtime only once
            // it owns a valid segment; rejection leaves this context unstarted.
            self.player_ownership = PlayerOwnership::Active(player);
            return Ok(end_time);
        }
        let player = self.active_live_player()?;
        player.live_wait(duration)
    }

    /// Read only the live runtime's authored handoff duration.
    ///
    /// Returns `None` until a live session exists.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_handoff_duration(&self) -> Option<f64> {
        self.player_ownership
            .local()
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
        if !self.player_ownership.is_unstarted() {
            return Err("declare live animations before beginning execution".into());
        }
        self.scene.declare_transform_to(source, target, options)
    }

    /// Read the shared session's direct-root membership for one retained wrapper.
    ///
    /// This lets Python update only its derived wrapper attachment after a completed
    /// FadeOut without storing lifecycle state or adding metadata to the player receipt.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_contains_mobject(&mut self, target: &noon::Mobject) -> Result<bool, AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), target.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        target.validate().map_err(AuthoringFailure::from)?;
        if !self.identities.contains_key(&target.node_id()) {
            return Err("live Mobject is not bound to this canonical Scene".into());
        }
        self.active_live_player()?.live_contains(target)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn ordinary_play_affine_lifecycle(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        direction: noon::AffineLifecycleDirection,
        endpoint: noon::AffineLifecycleEndpoint,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        let bootstrapped = self.player_ownership.is_unstarted();
        if bootstrapped {
            self.live_player(bootstrap_duration)?;
        }
        if self.active_live_player()?.has_required_callbacks() {
            if bootstrapped {
                self.player_ownership = PlayerOwnership::Unstarted;
            }
            return Err(
                "ordinary endpoint-only animation cannot execute required callbacks; use a continuation"
                    .into(),
            );
        }
        let end = self.begin_ordinary_affine_lifecycle(id, target, direction, endpoint, options)?;
        let player = self.active_live_player()?;
        player
            .live_advance_segment_to(end)
            .map_err(|error| error.to_string())?;
        player
            .live_complete_segment()
            .map_err(|error| error.to_string())?;
        player
            .live_handoff_duration()
            .ok_or_else(|| "live execution player has no handoff duration".to_owned())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn begin_ordinary_affine_lifecycle(
        &mut self,
        id: ObjectId,
        target: &noon::Mobject,
        direction: noon::AffineLifecycleDirection,
        endpoint: noon::AffineLifecycleEndpoint,
        options: noon_core::AnimationOptions,
    ) -> Result<f64, String> {
        let bootstrap_duration = self
            .live_handoff_duration()
            .unwrap_or_else(|| self.scene.time())
            .max(options.run_time.unwrap_or(1.0));
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), target.integration_store()) {
            return Err(
                "ordinary affine lifecycle mobject belongs to another authoring store".into(),
            );
        }
        target.validate().map_err(|error| error.to_string())?;
        let node = target.node_id();
        let is_bound = match (self.bindings.get(&id), self.identities.get(&node)) {
            (None, None) => false,
            (Some(bound_node), Some(bound_id)) if *bound_node == node && *bound_id == id => true,
            _ => return Err("ordinary affine lifecycle target has a conflicting binding".into()),
        };
        if direction == noon::AffineLifecycleDirection::IntroduceFrom && is_bound {
            return Err(format!("canonical object {} is already bound", id.get()));
        }
        let player = self.active_or_bootstrap_live_player(bootstrap_duration)?;
        let end = player
            .live_declare_and_activate_affine_lifecycle(target, direction, endpoint, options)?;
        if !is_bound {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(end)
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
        player
            .live_advance_segment_to(end)
            .map_err(|error| error.to_string())?;
        player
            .live_complete_segment()
            .map_err(|error| error.to_string())?;
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
        fn request(child: &OrdinaryCompositionChild) -> noon::AnimationCompositionRequest<'_> {
            match child {
                OrdinaryCompositionChild::FocusOn { focus, options } => {
                    noon::AnimationCompositionRequest::FocusOn {
                        focus: *focus,
                        options: *options,
                    }
                }
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
                OrdinaryCompositionChild::FamilyTransformTo {
                    source,
                    target_state,
                    options,
                } => noon::AnimationCompositionRequest::FamilyTransformTo {
                    source,
                    target_state,
                    options: *options,
                },
                OrdinaryCompositionChild::Indicate {
                    target,
                    indication,
                    options,
                } => noon::AnimationCompositionRequest::Indicate {
                    target,
                    indication: *indication,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilyIndicate {
                    target,
                    indication,
                    options,
                } => noon::AnimationCompositionRequest::FamilyIndicate {
                    target,
                    indication: *indication,
                    options: *options,
                },
                OrdinaryCompositionChild::DrawBorderThenFill {
                    target,
                    outline,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::DrawBorderThenFill {
                    target,
                    outline: *outline,
                    options: *options,
                },
                OrdinaryCompositionChild::PassingFlash {
                    target,
                    time_width,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::PassingFlash {
                    target,
                    time_width: *time_width,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilyDrawBorderThenFill {
                    target,
                    outline,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::FamilyDrawBorderThenFill {
                    target,
                    outline: *outline,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilySubsetDisplay {
                    target,
                    mode,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::FamilySubsetDisplay {
                    target,
                    mode: *mode,
                    options: *options,
                },
                OrdinaryCompositionChild::TextWrite {
                    target,
                    reverse_member_order,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::TextWrite {
                    target,
                    reverse_member_order: *reverse_member_order,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilyTextWrite {
                    target,
                    reverse_member_order,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::FamilyTextWrite {
                    target,
                    reverse_member_order: *reverse_member_order,
                    options: *options,
                },
                OrdinaryCompositionChild::TextReveal {
                    target,
                    reverse,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::TextReveal {
                    target,
                    reverse: *reverse,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilyReveal {
                    target,
                    reverse,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::FamilyReveal {
                    target,
                    reverse: *reverse,
                    options: *options,
                },
                OrdinaryCompositionChild::Rotate {
                    target,
                    angle,
                    pivot,
                    options,
                    ..
                } => match pivot {
                    Some(pivot) => noon::AnimationCompositionRequest::ManimRotate {
                        target,
                        angle: *angle,
                        pivot: *pivot,
                        options: *options,
                    },
                    None => noon::AnimationCompositionRequest::Rotate {
                        target,
                        angle: *angle,
                        options: *options,
                    },
                },
                OrdinaryCompositionChild::ValueTracker {
                    tracker,
                    target,
                    options,
                } => noon::AnimationCompositionRequest::ValueTracker {
                    tracker,
                    target: *target,
                    options: *options,
                },
                OrdinaryCompositionChild::Wait { duration } => {
                    noon::AnimationCompositionRequest::Wait {
                        duration: *duration,
                    }
                }
                OrdinaryCompositionChild::Add {
                    target, options, ..
                } => noon::AnimationCompositionRequest::Add {
                    target,
                    options: *options,
                },
                OrdinaryCompositionChild::Fade {
                    target,
                    direction,
                    endpoint,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::Fade {
                    target,
                    direction: *direction,
                    endpoint: *endpoint,
                    options: *options,
                },
                OrdinaryCompositionChild::FamilyFade {
                    target,
                    direction,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::FamilyFade {
                    target,
                    direction: *direction,
                    options: *options,
                },
                OrdinaryCompositionChild::Create {
                    target, options, ..
                } => noon::AnimationCompositionRequest::Create {
                    target,
                    options: *options,
                },
                OrdinaryCompositionChild::Uncreate {
                    target, options, ..
                } => noon::AnimationCompositionRequest::Uncreate {
                    target,
                    options: *options,
                },
                OrdinaryCompositionChild::AffineLifecycle {
                    target,
                    direction,
                    endpoint,
                    options,
                    ..
                } => noon::AnimationCompositionRequest::AffineLifecycle {
                    target,
                    direction: *direction,
                    endpoint: *endpoint,
                    options: *options,
                },
                OrdinaryCompositionChild::Composition {
                    kind,
                    children,
                    options,
                } => noon::AnimationCompositionRequest::Composition {
                    kind: *kind,
                    children: children.iter().map(request).collect(),
                    options: *options,
                },
            }
        }
        let requests = children.iter().map(request).collect::<Vec<_>>();
        let composition = noon::AnimationCompositionRequest::Composition {
            kind,
            children: requests,
            options: composition_options,
        };
        let end = if self.player_ownership.is_unstarted() {
            self.prepare_local_player_for_run()?;
            let mut player = self.build_live_player(bootstrap_duration, 0)?;
            if !allow_required_callbacks && player.has_required_callbacks() {
                return Err("ordinary composition with required callbacks needs an asynchronous continuation".into());
            }
            let end = player.live_declare_and_activate_composition(&composition, play_options)?;
            self.player_ownership = PlayerOwnership::Active(player);
            end
        } else {
            let player = self.active_live_player()?;
            if !allow_required_callbacks && player.has_required_callbacks() {
                return Err("ordinary composition with required callbacks needs an asynchronous continuation".into());
            }
            player.live_declare_and_activate_composition(&composition, play_options)?
        };
        fn bindings<'a>(
            child: &'a OrdinaryCompositionChild,
            output: &mut Vec<(ObjectId, &'a noon::Mobject)>,
        ) {
            match child {
                OrdinaryCompositionChild::TransformTo {
                    entering_id,
                    source,
                    ..
                } => {
                    if let Some(id) = entering_id {
                        output.push((*id, source));
                    }
                }
                OrdinaryCompositionChild::Rotate {
                    entering_id,
                    target,
                    ..
                }
                | OrdinaryCompositionChild::Fade {
                    entering_id,
                    target,
                    ..
                }
                | OrdinaryCompositionChild::Create {
                    entering_id,
                    target,
                    ..
                }
                | OrdinaryCompositionChild::Uncreate {
                    entering_id,
                    target,
                    ..
                }
                | OrdinaryCompositionChild::AffineLifecycle {
                    entering_id,
                    target,
                    ..
                } => {
                    if let Some(id) = entering_id {
                        output.push((*id, target));
                    }
                }
                OrdinaryCompositionChild::DrawBorderThenFill {
                    entering_id,
                    target,
                    ..
                }
                | OrdinaryCompositionChild::PassingFlash {
                    entering_id,
                    target,
                    ..
                } => {
                    if let Some(id) = entering_id {
                        output.push((*id, target));
                    }
                }
                OrdinaryCompositionChild::FamilyDrawBorderThenFill { entering, .. } => {
                    output.extend(entering.iter().map(|(id, target)| (*id, target)));
                }
                OrdinaryCompositionChild::FamilySubsetDisplay { entering, .. } => {
                    output.extend(entering.iter().map(|(id, target)| (*id, target)));
                }
                OrdinaryCompositionChild::FamilyFade { entering, .. }
                | OrdinaryCompositionChild::FamilyTextWrite { entering, .. }
                | OrdinaryCompositionChild::FamilyReveal { entering, .. } => {
                    output.extend(entering.iter().map(|(id, target)| (*id, target)));
                }
                OrdinaryCompositionChild::Add {
                    entering_id,
                    target,
                    ..
                } => output.push((*entering_id, target)),
                OrdinaryCompositionChild::TextWrite {
                    entering_id,
                    target,
                    ..
                } => {
                    if let Some(id) = entering_id {
                        output.push((*id, target));
                    }
                }
                OrdinaryCompositionChild::TextReveal {
                    entering_id,
                    target,
                    ..
                } => {
                    if let Some(id) = entering_id {
                        output.push((*id, target));
                    }
                }
                OrdinaryCompositionChild::FocusOn { .. }
                | OrdinaryCompositionChild::Wait { .. } => {}
                OrdinaryCompositionChild::ValueTracker { .. } => {}
                OrdinaryCompositionChild::FamilyTransformTo { .. }
                | OrdinaryCompositionChild::Indicate { .. }
                | OrdinaryCompositionChild::FamilyIndicate { .. } => {}
                OrdinaryCompositionChild::Composition { children, .. } => {
                    for child in children {
                        bindings(child, output);
                    }
                }
            }
        }
        let mut entering = Vec::new();
        for child in children {
            bindings(child, &mut entering);
        }
        for (id, target) in entering {
            self.bindings.insert(id, target.node_id());
            self.identities.insert(target.node_id(), id);
        }
        Ok(end)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn validate_ordinary_mixed_composition(
        &self,
        children: &[OrdinaryCompositionChild],
        composition_options: noon_core::AnimationOptions,
        play_options: noon_core::AnimationOptions,
    ) -> Result<(), String> {
        // A continuation cannot restart a pre-authored Rust timeline at zero.
        // This is the common admission guard for every request shape.
        if self.player_ownership.is_unstarted() && self.scene.time() != 0.0 {
            return Err("ordinary composition cannot follow pre-execution canonical timing".into());
        }
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
                // Value-only requests have no wrapper identity to validate. Shared
                // transaction preparation validates construction and animation options.
                OrdinaryCompositionChild::FocusOn { .. } => continue,
                OrdinaryCompositionChild::Wait { duration } => {
                    if !duration.is_finite() || *duration < 0.0 {
                        return Err(
                            "ordinary composition wait duration must be finite and non-negative"
                                .into(),
                        );
                    }
                    continue;
                }
                OrdinaryCompositionChild::ValueTracker {
                    tracker,
                    target,
                    options,
                } => {
                    if !tracker.is_in_store(self.scene.integration_store()) {
                        return Err(
                            "ordinary composition ValueTracker belongs to another authoring store"
                                .into(),
                        );
                    }
                    self.scene
                        .integration_store()
                        .borrow()
                        .semantic_signal_state(tracker.node_id())
                        .map_err(|error| error.to_string())?;
                    if !target.is_finite() {
                        return Err(
                            "ordinary composition ValueTracker target must be finite".into()
                        );
                    }
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    continue;
                }
                OrdinaryCompositionChild::FamilySubsetDisplay {
                    target,
                    entering,
                    options,
                    ..
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target.integration_store(),
                    ) {
                        return Err(
                            "ordinary subset-display family belongs to another authoring store"
                                .into(),
                        );
                    }
                    target.validate().map_err(|error| error.to_string())?;
                    let direct_members = self
                        .scene
                        .integration_store()
                        .borrow()
                        .semantic_family_members_checked(target.node_id())
                        .map_err(|error| error.to_string())?
                        .to_vec();
                    let expected_entering = direct_members
                        .iter()
                        .copied()
                        .filter(|id| !self.identities.contains_key(id))
                        .collect::<BTreeSet<_>>();
                    let supplied_entering = entering
                        .iter()
                        .map(|(_, member)| member.node_id())
                        .collect::<BTreeSet<_>>();
                    if supplied_entering != expected_entering {
                        return Err("ordinary subset-display wrapper identities do not match detached direct members".into());
                    }
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    for (id, member) in entering {
                        if !std::rc::Rc::ptr_eq(
                            self.scene.integration_store(),
                            member.integration_store(),
                        ) {
                            return Err(
                                "ordinary subset-display member belongs to another authoring store"
                                    .into(),
                            );
                        }
                        member.validate().map_err(|error| error.to_string())?;
                        if self.bindings.contains_key(id)
                            || self.identities.contains_key(&member.node_id())
                            || !ids.insert(*id)
                            || !entering_nodes.insert(member.node_id())
                        {
                            return Err(
                                "ordinary subset-display entering identity is already bound".into(),
                            );
                        }
                    }
                    continue;
                }
                OrdinaryCompositionChild::FamilyFade {
                    target,
                    entering,
                    options,
                    ..
                }
                | OrdinaryCompositionChild::FamilyTextWrite {
                    target,
                    entering,
                    options,
                    ..
                }
                | OrdinaryCompositionChild::FamilyReveal {
                    target,
                    entering,
                    options,
                    ..
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target.integration_store(),
                    ) {
                        return Err(
                            "ordinary family animation belongs to another authoring store".into(),
                        );
                    }
                    target.validate().map_err(|error| error.to_string())?;
                    let family_leaves = self
                        .scene
                        .integration_store()
                        .borrow()
                        .ordered_leaf_nodes(target.node_id())
                        .map_err(|e| e.to_string())?;
                    let expected_entering = family_leaves
                        .iter()
                        .copied()
                        .filter(|id| !self.identities.contains_key(id))
                        .collect::<BTreeSet<_>>();
                    let supplied_entering = entering
                        .iter()
                        .map(|(_, member)| member.node_id())
                        .collect::<BTreeSet<_>>();
                    if supplied_entering != expected_entering {
                        return Err(
                            "ordinary family wrapper identities do not match detached leaves"
                                .into(),
                        );
                    }
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        if matches!(
                            child,
                            OrdinaryCompositionChild::FamilyTextWrite { .. }
                                | OrdinaryCompositionChild::FamilyReveal { .. }
                        ) {
                            // Text glyph realization owns reversal. Preserve the
                            // authored option while preflighting the remaining shape.
                            noon_core::AnimationOptions::new().reverse_rate_function(false)
                        } else {
                            noon_core::AnimationOptions::new()
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    for (id, member) in entering {
                        if !std::rc::Rc::ptr_eq(
                            self.scene.integration_store(),
                            member.integration_store(),
                        ) {
                            return Err(
                                "ordinary family member belongs to another authoring store".into(),
                            );
                        }
                        member.validate().map_err(|error| error.to_string())?;
                        if self.bindings.contains_key(id)
                            || self.identities.contains_key(&member.node_id())
                            || !ids.insert(*id)
                            || !entering_nodes.insert(member.node_id())
                        {
                            return Err("ordinary family entering identity is already bound".into());
                        }
                    }
                    continue;
                }
                OrdinaryCompositionChild::FamilyDrawBorderThenFill {
                    target,
                    entering,
                    options,
                    ..
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target.integration_store(),
                    ) {
                        return Err(
                            "ordinary family DrawBorderThenFill belongs to another authoring store"
                                .into(),
                        );
                    }
                    target.validate().map_err(|error| error.to_string())?;
                    let family_leaves = self
                        .scene
                        .integration_store()
                        .borrow()
                        .ordered_leaf_nodes(target.node_id())
                        .map_err(|e| e.to_string())?;
                    let expected_entering = family_leaves
                        .iter()
                        .copied()
                        .filter(|id| !self.identities.contains_key(id))
                        .collect::<BTreeSet<_>>();
                    let supplied_entering = entering
                        .iter()
                        .map(|(_, member)| member.node_id())
                        .collect::<BTreeSet<_>>();
                    if supplied_entering != expected_entering {
                        return Err("ordinary family DrawBorderThenFill wrapper identities do not match detached family leaves".into());
                    }
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    for (id, member) in entering {
                        if !std::rc::Rc::ptr_eq(
                            self.scene.integration_store(),
                            member.integration_store(),
                        ) {
                            return Err("ordinary family DrawBorderThenFill member belongs to another authoring store".into());
                        }
                        member.validate().map_err(|error| error.to_string())?;
                        if self.bindings.contains_key(id)
                            || self.identities.contains_key(&member.node_id())
                            || !ids.insert(*id)
                            || !entering_nodes.insert(member.node_id())
                        {
                            return Err("ordinary family DrawBorderThenFill entering identity is already bound".into());
                        }
                    }
                    continue;
                }
                OrdinaryCompositionChild::FamilyTransformTo {
                    source,
                    target_state,
                    options,
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        source.integration_store(),
                    ) || !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target_state.integration_store(),
                    ) {
                        return Err(
                            "ordinary family Transform belongs to another authoring store".into(),
                        );
                    }
                    source.validate().map_err(|error| error.to_string())?;
                    target_state.validate().map_err(|error| error.to_string())?;
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    continue;
                }
                OrdinaryCompositionChild::FamilyIndicate {
                    target, options, ..
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target.integration_store(),
                    ) {
                        return Err(
                            "ordinary family Indicate belongs to another authoring store".into(),
                        );
                    }
                    target.validate().map_err(|error| error.to_string())?;
                    noon_core::resolve_animation_options(
                        noon_core::AnimationDefaults::MANIM,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                    continue;
                }
                OrdinaryCompositionChild::Composition {
                    kind: _,
                    children,
                    options,
                } => {
                    self.validate_ordinary_mixed_composition(
                        children,
                        *options,
                        noon_core::AnimationOptions::new(),
                    )?;
                    continue;
                }
                OrdinaryCompositionChild::TransformTo {
                    entering_id,
                    source,
                    target,
                    options,
                    ..
                } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        target.integration_store(),
                    ) {
                        return Err(
                            "ordinary composition target belongs to another authoring store".into(),
                        );
                    }
                    target.validate().map_err(|error| error.to_string())?;
                    if self.identities.contains_key(&target.node_id()) {
                        return Err(
                            "ordinary composition TransformTo target state must be detached".into(),
                        );
                    }
                    (*entering_id, source, *options)
                }
                OrdinaryCompositionChild::Indicate {
                    target, options, ..
                } => (None, target, *options),
                OrdinaryCompositionChild::DrawBorderThenFill {
                    entering_id,
                    target,
                    options,
                    ..
                }
                | OrdinaryCompositionChild::PassingFlash {
                    entering_id,
                    target,
                    options,
                    ..
                } => (*entering_id, target, *options),
                OrdinaryCompositionChild::Rotate {
                    entering_id,
                    target,
                    angle,
                    options,
                    ..
                } => {
                    if !angle.is_finite() {
                        return Err("ordinary Rotate angle must be finite".into());
                    }
                    (*entering_id, target, *options)
                }
                OrdinaryCompositionChild::Add {
                    entering_id,
                    target,
                    options,
                } => (Some(*entering_id), target, *options),
                OrdinaryCompositionChild::TextWrite {
                    entering_id,
                    target,
                    options,
                    ..
                } => (*entering_id, target, *options),
                OrdinaryCompositionChild::TextReveal {
                    entering_id,
                    target,
                    options,
                    ..
                } => (*entering_id, target, *options),
                OrdinaryCompositionChild::Fade {
                    entering_id,
                    target,
                    options,
                    ..
                }
                | OrdinaryCompositionChild::Create {
                    entering_id,
                    target,
                    options,
                }
                | OrdinaryCompositionChild::Uncreate {
                    entering_id,
                    target,
                    options,
                }
                | OrdinaryCompositionChild::AffineLifecycle {
                    entering_id,
                    target,
                    options,
                    ..
                } => (*entering_id, target, *options),
            };
            if !std::rc::Rc::ptr_eq(self.scene.integration_store(), target.integration_store()) {
                return Err(
                    "ordinary composition target belongs to another authoring store".into(),
                );
            }
            target.validate().map_err(|error| error.to_string())?;
            let resolved = if matches!(child, OrdinaryCompositionChild::Add { .. }) {
                noon_core::resolve_add_animation_options(
                    noon_core::AnimationDefaults::MANIM,
                    options,
                    noon_core::AnimationOptions::new(),
                )
            } else {
                noon_core::resolve_animation_options(
                    noon_core::AnimationDefaults::MANIM,
                    options,
                    if matches!(
                        child,
                        OrdinaryCompositionChild::TextWrite { .. }
                            | OrdinaryCompositionChild::TextReveal { .. }
                            | OrdinaryCompositionChild::Uncreate { .. }
                    ) {
                        // Glyph realization owns reversal; keep the original child
                        // options for shared schedule lowering after shape validation.
                        noon_core::AnimationOptions::new().reverse_rate_function(false)
                    } else {
                        noon_core::AnimationOptions::new()
                    },
                )
            };
            resolved.map_err(|error| error.to_string())?;
            match entering_id {
                Some(id) => {
                    let node = target.node_id();
                    let detached = !self
                        .contains_mobject(target)
                        .map_err(|error| error.to_string())?;
                    let binding_available =
                        match (self.bindings.get(&id), self.identities.get(&node)) {
                            (None, None) => detached,
                            (Some(bound_node), Some(bound_id))
                                if *bound_node == node && *bound_id == id =>
                            {
                                detached
                            }
                            _ => false,
                        };
                    if !binding_available || !ids.insert(id) || !entering_nodes.insert(node) {
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
    fn live_target_editor(
        &mut self,
        source: &noon::Mobject,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), source.integration_store()) {
            return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                .with_message("mobject belongs to another authoring store"));
        }
        source.validate().map_err(AuthoringFailure::from)?;
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => source.target_editor().map_err(AuthoringFailure::from),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_target_editor(source)
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_family(
        &mut self,
        members: &[noon::MobjectFamilyMember<'_>],
        z_index: f64,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_family(members, z_index)
            }
            PlayerOwnership::Unstarted => {
                Err("live family creation requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_shift_family(
        &mut self,
        family: &noon::MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_shift_family(family, x, y)
            }
            PlayerOwnership::Unstarted => {
                Err("live family shift requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    /// Delegate live family layout to the reusable Rust semantic facade.
    #[cfg(any(target_arch = "wasm32", test))]
    fn live_arrange_family(
        &mut self,
        family: &noon::MobjectFamily,
        options: &noon::FamilyArrangeOptions,
    ) -> Result<(), AuthoringFailure> {
        self.active_live_player()?
            .live_arrange_family(family, options)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn prepare_family_subset_display(
        &mut self,
        family: &noon::MobjectFamily,
    ) -> Result<(), String> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), family.integration_store()) {
            return Err("subset-display family belongs to another authoring store".into());
        }
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted => family.prepare_subset_display(),
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .prepare_family_subset_display(family),
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_become_mobject(
        &mut self,
        target: &noon::Mobject,
        other: &noon::Mobject,
        options: noon::ManimBecomeOptions,
    ) -> Result<(), AuthoringFailure> {
        for object in [target, other] {
            if !std::rc::Rc::ptr_eq(self.scene.integration_store(), object.integration_store()) {
                return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                    .with_message(
                        "become objects and canonical context belong to different authoring stores",
                    ));
            }
        }
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_become_mobject(target, other, options),
            PlayerOwnership::Unstarted => {
                Err("live become requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_create_manim_geometry(
        &mut self,
        options: noon::ManimGeometryOptions,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => self
                .active_live_player()?
                .live_create_manim_geometry(options),
            PlayerOwnership::Unstarted => {
                Err("live primitive construction requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_create_text(&mut self, text: noon::Text) -> Result<noon::Mobject, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_create_text(text)
            }
            PlayerOwnership::Unstarted => {
                Err("live Text construction requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_create_typst(&mut self, text: noon::Typst) -> Result<noon::Mobject, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_create_typst(text)
            }
            PlayerOwnership::Unstarted => {
                Err("live Typst construction requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_create_math_typst(
        &mut self,
        text: noon::MathTypst,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        match &mut self.player_ownership {
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_create_math_typst(text)
            }
            PlayerOwnership::Unstarted => {
                Err("live MathTypst construction requires an active canonical session".into())
            }
            PlayerOwnership::Transferred(_) => {
                Err("live execution session is running in the semantic engine".into())
            }
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_add_mobject(
        &mut self,
        id: ObjectId,
        handle: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        self.edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Mobject {
                wrapper_id: Some(id),
                handle: handle.clone(),
            }],
            bindings: vec![(id, handle.clone())],
        })
    }

    fn edit_membership(&mut self, batch: SceneMembershipBatch) -> Result<(), AuthoringFailure> {
        let mut new_bindings = Vec::new();
        let mut seen_ids = BTreeSet::new();
        let mut seen_nodes = BTreeSet::new();
        for (wrapper_id, handle) in &batch.bindings {
            if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
                return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                    .with_message("membership mobject belongs to another authoring store"));
            }
            handle.validate().map_err(AuthoringFailure::from)?;
            let node = handle.node_id();
            if !seen_ids.insert(*wrapper_id) || !seen_nodes.insert(node) {
                return Err(AuthoringFailure::new(
                    "invalid_input",
                    "boundary.duplicate_binding",
                    "membership batch contains a duplicate mobject binding",
                ));
            }
            match (self.bindings.get(wrapper_id), self.identities.get(&node)) {
                (Some(bound_node), Some(bound_id))
                    if *bound_node == node && *bound_id == *wrapper_id => {}
                (None, None)
                    if matches!(
                        batch.kind,
                        SceneMembershipBatchKind::Add | SceneMembershipBatchKind::Replace
                    ) =>
                {
                    new_bindings.push((*wrapper_id, node));
                }
                _ => {
                    return Err(format!(
                        "canonical object {} has inconsistent membership binding",
                        wrapper_id.get()
                    )
                    .into());
                }
            }
        }
        let mut borrowed = Vec::with_capacity(batch.members.len());
        for member in &batch.members {
            match member {
                OwnedSceneMembershipMember::Mobject { wrapper_id, handle } => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        handle.integration_store(),
                    ) {
                        return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                            .with_message(
                                "membership mobject belongs to another authoring store",
                            ));
                    }
                    handle.validate().map_err(AuthoringFailure::from)?;
                    let node = handle.node_id();
                    if let Some(wrapper_id) = wrapper_id {
                        if !batch
                            .bindings
                            .iter()
                            .any(|(id, bound)| *id == *wrapper_id && bound.node_id() == node)
                        {
                            return Err(format!(
                                "canonical object {} has no validated membership binding",
                                wrapper_id.get()
                            )
                            .into());
                        }
                    } else if matches!(
                        batch.kind,
                        SceneMembershipBatchKind::Add | SceneMembershipBatchKind::Replace
                    ) {
                        return Err(AuthoringFailure::new(
                            "invalid_input",
                            "boundary.missing_binding",
                            "added membership mobject has no wrapper binding",
                        ));
                    }
                    borrowed.push(noon::MobjectFamilyMember::Mobject(handle));
                }
                OwnedSceneMembershipMember::Family(family) => {
                    if !std::rc::Rc::ptr_eq(
                        self.scene.integration_store(),
                        family.integration_store(),
                    ) {
                        return Err(AuthoringFailure::from(noon::AuthoringError::ForeignStore)
                            .with_message("membership family belongs to another authoring store"));
                    }
                    family.validate().map_err(AuthoringFailure::from)?;
                    if !seen_nodes.insert(family.node_id()) {
                        return Err(AuthoringFailure::new(
                            "invalid_input",
                            "boundary.duplicate_family",
                            "membership batch contains a duplicate family",
                        ));
                    }
                    borrowed.push(noon::MobjectFamilyMember::Family(family));
                }
            }
        }
        let request = match batch.kind {
            SceneMembershipBatchKind::Add => noon::SceneMembershipRequest::Add(&borrowed),
            SceneMembershipBatchKind::Remove => noon::SceneMembershipRequest::Remove(&borrowed),
            SceneMembershipBatchKind::Clear => {
                if !borrowed.is_empty() {
                    return Err(AuthoringFailure::new(
                        "invalid_input",
                        "boundary.clear_members",
                        "Clear membership batch must not contain members",
                    ));
                }
                noon::SceneMembershipRequest::Clear
            }
            SceneMembershipBatchKind::Replace => {
                let [old, new] = borrowed.as_slice() else {
                    return Err(AuthoringFailure::new(
                        "invalid_input",
                        "boundary.replace_arity",
                        "Replace membership batch requires exactly old and new",
                    ));
                };
                noon::SceneMembershipRequest::Replace {
                    old: *old,
                    new: *new,
                }
            }
        };
        #[cfg(not(any(target_arch = "wasm32", test)))]
        self.scene
            .edit_membership(request)
            .map_err(AuthoringFailure::from)?;
        #[cfg(any(target_arch = "wasm32", test))]
        match &mut self.player_ownership {
            PlayerOwnership::Unstarted if self.scene.time() == 0.0 => {
                self.scene
                    .edit_membership(request)
                    .map_err(AuthoringFailure::from)?;
            }
            PlayerOwnership::Active(_) | PlayerOwnership::Returned(_) => {
                self.active_live_player()?.live_edit_membership(request)?;
            }
            PlayerOwnership::Unstarted => {
                return Err("membership edit cannot follow pre-execution canonical timing".into());
            }
            PlayerOwnership::Transferred(_) => {
                return Err("live execution session is running in the semantic engine".into());
            }
        }
        for (id, node) in new_bindings {
            self.bindings.insert(id, node);
            self.identities.insert(node, id);
        }
        Ok(())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn root_membership_keys(&self) -> Result<Vec<String>, AuthoringFailure> {
        self.scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(self.scene.root())
            .map(|members| {
                members
                    .iter()
                    .map(|node| format!("{}:{}", node.slot(), node.generation()))
                    .collect()
            })
            .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn contains_mobject(&self, target: &noon::Mobject) -> Result<bool, AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), target.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        target.validate().map_err(AuthoringFailure::from)?;
        noon_core::semantic_scene_root_contains(
            &self.scene.integration_store().borrow(),
            self.scene.root(),
            target.node_id(),
        )
        .map_err(AuthoringFailure::from)
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_remove_mobject(&mut self, handle: &noon::Mobject) -> Result<(), AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), handle.integration_store()) {
            return Err(noon::AuthoringError::ForeignStore.into());
        }
        let node = handle.node_id();
        let id = *self
            .identities
            .get(&node)
            .ok_or("live Mobject is not bound to this Scene")?;
        self.edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Remove,
            members: vec![OwnedSceneMembershipMember::Mobject {
                wrapper_id: Some(id),
                handle: handle.clone(),
            }],
            bindings: vec![(id, handle.clone())],
        })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn live_replace_content(
        &mut self,
        target: &noon::Mobject,
        source: &noon::Mobject,
    ) -> Result<(), AuthoringFailure> {
        if !std::rc::Rc::ptr_eq(self.scene.integration_store(), target.integration_store())
            || !std::rc::Rc::ptr_eq(self.scene.integration_store(), source.integration_store())
        {
            return Err(noon::AuthoringError::ForeignStore.into());
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
        if let Some(player) = self.player_ownership.local_mut() {
            player.rebind_transport(duration, transport_session)?;
        } else {
            let player = self.build_live_player(duration, transport_session)?;
            self.player_ownership = PlayerOwnership::Active(player);
        }
        self.player_ownership.transfer()
    }

    /// Return a player after endpoint setup or renderer recovery. This preserves
    /// the one runtime so reattachment never lowers a parallel session.
    #[cfg(any(target_arch = "wasm32", test))]
    fn return_execution_player(
        &mut self,
        player: crate::SemanticExecutionPlayer,
    ) -> Result<(), RejectedPlayerReturn> {
        if let Err(reason) = self.validate_execution_player_return(&player) {
            return Err(RejectedPlayerReturn {
                reason,
                player: Box::new(player),
            });
        }
        self.player_ownership = PlayerOwnership::Returned(player);
        Ok(())
    }

    /// Validate by reference before consuming a player; a rejected return cannot
    /// replace the rightful lease or mutate its publication/continuation state.
    #[cfg(any(target_arch = "wasm32", test))]
    fn validate_execution_player_return(
        &self,
        player: &crate::SemanticExecutionPlayer,
    ) -> Result<(), PlayerReturnError> {
        self.player_ownership.validate_return(
            player,
            self.scene.integration_store(),
            self.scene.root(),
        )
    }

    /// Resume the exact returned player for a newly-authored continuation segment.
    ///
    /// Unlike a new endpoint/recovery handoff, this keeps the existing transport encoder,
    /// resource bundle, session sequence, and snapshot state. The authoring continuation
    /// may only resume a player after it has declared one supported pending segment.
    #[cfg(any(target_arch = "wasm32", test))]
    fn resume_execution_player(&mut self) -> Result<crate::SemanticExecutionPlayer, String> {
        let PlayerOwnership::Returned(player) = &self.player_ownership else {
            return Err("semantic continuation player is not returned to this context".into());
        };
        if !player.has_pending_live_segment() {
            return Err("semantic continuation has no pending segment to resume".into());
        }
        player
            .require_callback_progression_available()
            .map_err(|error| error.to_string())?;
        self.player_ownership.transfer()
    }

    /// Encode final authored changes through the returned player's existing worker
    /// transport. This neither leases nor advances the completed runtime.
    #[cfg(any(target_arch = "wasm32", test))]
    fn drain_returned_publication_json(&mut self) -> Result<Option<String>, String> {
        let PlayerOwnership::Returned(player) = &mut self.player_ownership else {
            return Err("final publication requires a returned execution player".into());
        };
        player
            .require_callback_progression_available()
            .map_err(|error| error.to_string())?;
        if player.has_pending_live_segment() {
            return Err("final publication requires a completed continuation segment".into());
        }
        player.drain_delta_json()
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn authored_mobject_layout(
    handle: &noon::Mobject,
) -> Result<(f64, f64, f64, f64), noon::AuthoringError> {
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

#[cfg(target_arch = "wasm32")]
fn checked_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use noon_core::{Color, Style, Transform2D, Vec2};
    use wasm_bindgen::prelude::*;

    use super::*;
    use crate::authoring_error::js_error as typed_js_error;

    /// A rejected ownership return retains the consumed WASM player wrapper.
    /// Its projected JS Error retains `takePlayer()` to recover that exact player;
    /// returning it to its rightful context requires no lowering or cloning.
    #[wasm_bindgen]
    pub struct WasmExecutionPlayerReturnError {
        rejected: RejectedPlayerReturn,
    }

    #[wasm_bindgen]
    impl WasmExecutionPlayerReturnError {
        #[wasm_bindgen(getter)]
        pub fn message(&self) -> String {
            self.rejected.to_string()
        }

        #[wasm_bindgen(js_name = toString)]
        pub fn to_js_string(&self) -> String {
            self.message()
        }

        /// Consume this rejection and restore ownership of the original player.
        #[wasm_bindgen(js_name = takePlayer)]
        pub fn take_player(self) -> crate::SemanticExecutionPlayer {
            *self.rejected.player
        }
    }

    impl WasmExecutionPlayerReturnError {
        fn into_js_error(self) -> JsValue {
            // Pyodide requires a real Error, not an Error-like WASM class.
            // Binding the existing consuming method retains the rejected player
            // in exactly one Rust owner; no player clone or host lease is added.
            let error = typed_js_error(AuthoringFailure::from(self.rejected.reason));
            let rejection = JsValue::from(self);
            let take_player: js_sys::Function =
                js_sys::Reflect::get(&rejection, &JsValue::from_str("takePlayer"))
                    .expect("WASM rejection exposes takePlayer")
                    .unchecked_into();
            js_sys::Reflect::set(
                &error,
                &JsValue::from_str("takePlayer"),
                &take_player.bind0(&rejection),
            )
            .expect("new Error accepts its recovery capability");
            error
        }
    }

    use crate::authoring_error::js_error;

    // The callback overlay is an explicit codec boundary. Preserve codec causes
    // without assigning them an authoring category based on their diagnostics.
    fn callback_codec_error(error: impl std::error::Error + 'static) -> JsValue {
        typed_js_error(AuthoringFailure::unclassified("callback.codec", &error))
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

    fn parse_fade_endpoint(
        scale_factor: f64,
        translation: &str,
        x: f64,
        y: f64,
    ) -> Result<noon::FadeEndpoint, JsValue> {
        let vector = SemanticVec3::new(x, y, 0.0);
        let translation = match translation {
            "shift" => noon::FadeTranslation::Shift(vector),
            "point" => noon::FadeTranslation::Point(vector),
            _ => {
                return Err(js_error(format!(
                    "ordinary fade translation must be \"shift\" or \"point\", got {translation:?}"
                )))
            }
        };
        Ok(noon::FadeEndpoint::new(scale_factor, translation))
    }

    fn parse_affine_lifecycle_direction(
        value: &str,
    ) -> Result<noon::AffineLifecycleDirection, JsValue> {
        match value {
            "introduce-from" => Ok(noon::AffineLifecycleDirection::IntroduceFrom),
            "remove-to" => Ok(noon::AffineLifecycleDirection::RemoveTo),
            _ => Err(js_error(format!(
                "ordinary affine lifecycle direction must be \"introduce-from\" or \"remove-to\", got {value:?}"
            ))),
        }
    }

    fn parse_affine_lifecycle_endpoint(
        value: &str,
        x: f64,
        y: f64,
        rotation_offset: f64,
        point_color: Option<Color>,
    ) -> Result<noon::AffineLifecycleEndpoint, JsValue> {
        match value {
            "point" => Ok(noon::AffineLifecycleEndpoint::Point {
                x,
                y,
                rotation_offset,
                point_color,
            }),
            "effective-center" => Ok(noon::AffineLifecycleEndpoint::EffectiveCenter),
            _ => Err(js_error(format!(
                "ordinary affine lifecycle endpoint must be \"point\" or \"effective-center\", got {value:?}"
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

    /// Inert typed language-wrapper batch. Appending handles performs no semantic
    /// mutation; the canonical context consumes the complete batch atomically.
    #[wasm_bindgen]
    pub struct WasmSceneMembershipBatch {
        inner: SceneMembershipBatch,
    }

    impl WasmSceneMembershipBatch {
        pub(crate) fn copy_references(&self) -> Result<Vec<noon::MobjectFamilyMember<'_>>, String> {
            if self.inner.kind != SceneMembershipBatchKind::Add {
                return Err("copy references require an add batch".into());
            }
            self.inner.family_members()
        }

        pub(crate) fn create_family(
            &self,
            store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
            z_index: f64,
        ) -> Result<noon::MobjectFamily, AuthoringFailure> {
            self.inner.create_family(store, z_index)
        }

        pub(crate) fn edit_family(
            &self,
            family: &noon::MobjectFamily,
        ) -> Result<Vec<bool>, AuthoringFailure> {
            self.inner.edit_family(family)
        }
    }

    #[wasm_bindgen]
    impl WasmSceneMembershipBatch {
        #[wasm_bindgen(constructor)]
        pub fn new(kind: &str) -> Result<WasmSceneMembershipBatch, JsValue> {
            let kind = match kind {
                "add" => SceneMembershipBatchKind::Add,
                "remove" => SceneMembershipBatchKind::Remove,
                "clear" => SceneMembershipBatchKind::Clear,
                "replace" => SceneMembershipBatchKind::Replace,
                _ => {
                    return Err(js_error(format!(
                        "membership batch kind must be add, remove, clear, or replace; got {kind:?}"
                    )))
                }
            };
            Ok(Self {
                inner: SceneMembershipBatch {
                    kind,
                    members: Vec::new(),
                    bindings: Vec::new(),
                },
            })
        }

        #[wasm_bindgen(js_name = appendMobject)]
        pub fn append_mobject(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let wrapper_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("membership object ID", object_id)?)
            };
            self.inner
                .members
                .push(OwnedSceneMembershipMember::Mobject {
                    wrapper_id,
                    handle: handle.semantic_mobject().clone(),
                });
            Ok(())
        }

        /// Reserve a derived Python wrapper identity for a leaf admitted through
        /// an authoritative family member. It does not add another request member.
        #[wasm_bindgen(js_name = reserveMobjectBinding)]
        pub fn reserve_mobject_binding(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let wrapper_id = parse_object_id("membership object ID", object_id)?;
            self.inner
                .bindings
                .push((wrapper_id, handle.semantic_mobject().clone()));
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamily)]
        pub fn append_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<(), JsValue> {
            self.inner.members.push(OwnedSceneMembershipMember::Family(
                handle.semantic_family()?,
            ));
            Ok(())
        }
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
    pub struct WasmAnimationCompositionBuilder {
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

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> f64 {
            if direction_x < 0.0 {
                self.center_x - self.width * 0.5
            } else if direction_x > 0.0 {
                self.center_x + self.width * 0.5
            } else {
                self.center_x
            }
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, _direction_x: f64, direction_y: f64) -> f64 {
            if direction_y < 0.0 {
                self.center_y - self.height * 0.5
            } else if direction_y > 0.0 {
                self.center_y + self.height * 0.5
            } else {
                self.center_y
            }
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

    impl WasmAnimationCompositionBuilder {
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
                pivot: None,
                options: noon_core::AnimationOptions::new()
                    .run_time(child_run_time)
                    .rate_func(rate_function),
            });
            Ok(())
        }

        fn options(
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<noon_core::AnimationOptions, JsValue> {
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            Ok(noon_core::AnimationOptions::new()
                .run_time(child_run_time)
                .rate_func(rate_function))
        }

        fn optional_options(
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<noon_core::AnimationOptions, JsValue> {
            let mut options = noon_core::AnimationOptions::new();
            if let Some(run_time) = child_run_time {
                options = options.run_time(run_time);
            }
            if let Some(rate_function) = rate_function {
                let rate_function = noon_core::RateFunction::from_semantic_id(&rate_function)
                    .ok_or_else(|| {
                        js_error(format!(
                            "unsupported animation rate function semantic ID {rate_function:?}"
                        ))
                    })?;
                options = options.rate_func(rate_function);
            }
            Ok(options)
        }

        fn family_options(
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<noon_core::AnimationOptions, JsValue> {
            let mut options = Self::optional_options(child_run_time, rate_function)?;
            if let Some(lag_ratio) = lag_ratio {
                options = options.lag_ratio(lag_ratio);
            }
            Ok(options)
        }

        fn push_target(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            options: noon_core::AnimationOptions,
            child: impl FnOnce(
                ObjectId,
                noon::Mobject,
                noon_core::AnimationOptions,
            ) -> OrdinaryCompositionChild,
        ) -> Result<(), JsValue> {
            self.children.push(child(
                parse_object_id("object ID", object_id)?,
                target.semantic_mobject().clone(),
                options,
            ));
            Ok(())
        }

        fn push_entering_lifecycle(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            endpoint: &str,
            x: f64,
            y: f64,
            rotation_offset: f64,
            point_red: Option<f64>,
            point_green: Option<f64>,
            point_blue: Option<f64>,
            point_alpha: Option<f64>,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let direction = parse_affine_lifecycle_direction(direction)?;
            let endpoint = parse_affine_lifecycle_endpoint(
                endpoint,
                x,
                y,
                rotation_offset,
                callback_color(
                    "point_color",
                    point_red,
                    point_green,
                    point_blue,
                    point_alpha,
                )?,
            )?;
            let options = Self::options(child_run_time, rate_function)?;
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("object ID", object_id)?)
            };
            self.children
                .push(OrdinaryCompositionChild::AffineLifecycle {
                    entering_id,
                    target: target.semantic_mobject().clone(),
                    direction,
                    endpoint,
                    options,
                });
            Ok(())
        }
    }

    #[wasm_bindgen]
    impl WasmAnimationCompositionBuilder {
        #[wasm_bindgen(js_name = setCompositionRateFunction)]
        pub fn set_composition_rate_function(
            &mut self,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.composition_options = self.composition_options.rate_func(rate_function);
            Ok(())
        }

        #[wasm_bindgen(js_name = setPlayRateFunction)]
        pub fn set_play_rate_function(&mut self, rate_function: &str) -> Result<(), JsValue> {
            let rate_function = noon_core::RateFunction::from_semantic_id(rate_function)
                .ok_or_else(|| {
                    js_error(format!(
                        "unsupported animation rate function semantic ID {rate_function:?}"
                    ))
                })?;
            self.play_options = self.play_options.rate_func(rate_function);
            Ok(())
        }

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

        /// Inert request: the native live session validates the effective pivot.
        #[allow(clippy::too_many_arguments)]
        #[wasm_bindgen(js_name = appendManimRotate)]
        pub fn append_manim_rotate(
            &mut self,
            object_id: Option<String>,
            target: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
            pivot_kind: &str,
            pivot_x: f64,
            pivot_y: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<(), JsValue> {
            let pivot = match pivot_kind {
                "center" => noon::ManimRotationPivot::Center,
                "point" => noon::ManimRotationPivot::Point(pivot_x, pivot_y),
                "edge" => noon::ManimRotationPivot::Edge(pivot_x, pivot_y),
                _ => return Err(js_error("unknown procedural rotation pivot kind")),
            };
            self.children.push(OrdinaryCompositionChild::Rotate {
                entering_id: object_id
                    .as_deref()
                    .map(|id| parse_object_id("object ID", id))
                    .transpose()?,
                target: target.semantic_mobject().clone(),
                angle,
                pivot: Some(pivot),
                options: Self::optional_options(child_run_time, rate_function)?,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendWait)]
        pub fn append_wait(&mut self, child_run_time: f64) -> Result<(), JsValue> {
            if !child_run_time.is_finite() || child_run_time < 0.0 {
                return Err(js_error("wait duration must be finite and non-negative"));
            }
            self.children.push(OrdinaryCompositionChild::Wait {
                duration: child_run_time,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendValueTracker)]
        pub fn append_value_tracker(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            target: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<(), JsValue> {
            if !target.is_finite() {
                return Err(js_error("ValueTracker target must be finite"));
            }
            tracker.tracker_in(&tracker.store)?;
            self.children.push(OrdinaryCompositionChild::ValueTracker {
                tracker: tracker.tracker.clone(),
                target,
                options: Self::optional_options(child_run_time, rate_function)?,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilyTransformTo)]
        pub fn append_family_transform_to(
            &mut self,
            source: &crate::WasmAuthoringFamilyHandle,
            target_state: &crate::WasmAuthoringFamilyHandle,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            self.children
                .push(OrdinaryCompositionChild::FamilyTransformTo {
                    source: source.semantic_family()?,
                    target_state: target_state.semantic_family()?,
                    options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendIndicateMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_indicate_mobject(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            scale_factor: f64,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let color = callback_color(
                "indication color",
                Some(red),
                Some(green),
                Some(blue),
                Some(alpha),
            )?
            .expect("all indication color channels were supplied");
            self.children.push(OrdinaryCompositionChild::Indicate {
                target: target.semantic_mobject().clone(),
                indication: noon::IndicateOptions::new(scale_factor, color),
                options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendIndicateFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_indicate_family(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            scale_factor: f64,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let color = callback_color(
                "indication color",
                Some(red),
                Some(green),
                Some(blue),
                Some(alpha),
            )?
            .expect("all indication color channels were supplied");
            self.children
                .push(OrdinaryCompositionChild::FamilyIndicate {
                    target: target.semantic_family()?,
                    indication: noon::IndicateOptions::new(scale_factor, color),
                    options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFocusOn)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_focus_on(
            &mut self,
            x: f64,
            y: f64,
            opacity: f64,
            red: f64,
            green: f64,
            blue: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<(), JsValue> {
            let color =
                callback_color("focus color", Some(red), Some(green), Some(blue), Some(1.0))?
                    .expect("all focus color channels supplied");
            self.children.push(OrdinaryCompositionChild::FocusOn {
                focus: noon::FocusOnOptions {
                    point: (x, y),
                    opacity,
                    color,
                },
                options: Self::optional_options(child_run_time, rate_function)?,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendPassingFlash)]
        pub fn append_passing_flash(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            time_width: f64,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<(), JsValue> {
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("PassingFlash object ID", object_id)?)
            };
            self.children.push(OrdinaryCompositionChild::PassingFlash {
                entering_id,
                target: target.semantic_mobject().clone(),
                time_width,
                options: Self::optional_options(child_run_time, rate_function)?
                    .introducer(true)
                    .remover(true),
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendDrawBorderThenFillMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_draw_border_then_fill_mobject(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            stroke_width: f64,
            red: Option<f64>,
            green: Option<f64>,
            blue: Option<f64>,
            alpha: Option<f64>,
            phase_rate_function: &str,
            introducer: bool,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let color = callback_color("outline color", red, green, blue, alpha)?;
            let phase = noon_core::RateFunction::from_semantic_id(phase_rate_function).ok_or_else(
                || {
                    js_error(format!(
                        "unsupported outline phase rate function {phase_rate_function:?}"
                    ))
                },
            )?;
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("DrawBorderThenFill object ID", object_id)?)
            };
            self.children
                .push(OrdinaryCompositionChild::DrawBorderThenFill {
                    entering_id,
                    target: target.semantic_mobject().clone(),
                    outline: noon::DrawBorderThenFillOptions::new(stroke_width, color)
                        .with_phase_rate_function(phase),
                    options: Self::options(child_run_time, rate_function)?.introducer(introducer),
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendDrawBorderThenFillFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_draw_border_then_fill_family(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            stroke_width: f64,
            red: Option<f64>,
            green: Option<f64>,
            blue: Option<f64>,
            alpha: Option<f64>,
            phase_rate_function: &str,
            introducer: bool,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let color = callback_color("outline color", red, green, blue, alpha)?;
            let phase = noon_core::RateFunction::from_semantic_id(phase_rate_function).ok_or_else(
                || {
                    js_error(format!(
                        "unsupported outline phase rate function {phase_rate_function:?}"
                    ))
                },
            )?;
            self.children
                .push(OrdinaryCompositionChild::FamilyDrawBorderThenFill {
                    target: target.semantic_family()?,
                    entering: Vec::new(),
                    outline: noon::DrawBorderThenFillOptions::new(stroke_width, color)
                        .with_phase_rate_function(phase),
                    options: Self::family_options(child_run_time, rate_function, lag_ratio)?
                        .introducer(introducer),
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendDrawBorderThenFillFamilyEntering)]
        pub fn append_draw_border_then_fill_family_entering(
            &mut self,
            object_id: &str,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::FamilyDrawBorderThenFill {
                target, entering, ..
            }) = self.children.last_mut()
            else {
                return Err(js_error(
                    "family entering member must follow FamilyDrawBorderThenFill",
                ));
            };
            if !std::rc::Rc::ptr_eq(
                target.integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error(
                    "family entering member belongs to another authoring store",
                ));
            }
            entering.push((
                parse_object_id("DrawBorderThenFill family object ID", object_id)?,
                member.semantic_mobject().clone(),
            ));
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilySubsetDisplay)]
        pub fn append_family_subset_display(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            mode: &str,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
        ) -> Result<(), JsValue> {
            let mode = match mode {
                "increasing-floor" => noon::SubsetDisplayMode::IncreasingFloor,
                "one-by-one-ceil" => noon::SubsetDisplayMode::OneByOneCeil,
                _ => return Err(js_error("unknown subset-display mode")),
            };
            self.children
                .push(OrdinaryCompositionChild::FamilySubsetDisplay {
                    target: target.semantic_family()?,
                    entering: Vec::new(),
                    mode,
                    options: Self::optional_options(child_run_time, rate_function)?,
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilySubsetDisplayEntering)]
        pub fn append_family_subset_display_entering(
            &mut self,
            object_id: &str,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::FamilySubsetDisplay {
                target, entering, ..
            }) = self.children.last_mut()
            else {
                return Err(js_error(
                    "family entering member must follow FamilySubsetDisplay",
                ));
            };
            if !std::rc::Rc::ptr_eq(
                target.integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error(
                    "family entering member belongs to another authoring store",
                ));
            }
            entering.push((
                parse_object_id("subset-display family object ID", object_id)?,
                member.semantic_mobject().clone(),
            ));
            Ok(())
        }

        /// Append one plain-Text Write/Unwrite leaf. Glyph membership, default
        /// duration/lag, admission, and phase scheduling remain shared Rust semantics.
        #[wasm_bindgen(js_name = appendTextWrite)]
        pub fn append_text_write(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            reverse_member_order: bool,
            introducer: bool,
            remover: bool,
            reverse_rate_function: bool,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let options = Self::family_options(child_run_time, rate_function, lag_ratio)?
                .introducer(introducer)
                .remover(remover)
                .reverse_rate_function(reverse_rate_function);
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("Text Write object ID", object_id)?)
            };
            self.children.push(OrdinaryCompositionChild::TextWrite {
                entering_id,
                target: target.semantic_mobject().clone(),
                reverse_member_order,
                options,
            });
            Ok(())
        }

        /// Append one plain-Text family Write/Unwrite. Rust resolves global glyph
        /// count, default timing, lifecycle, and per-leaf drivers atomically.
        #[wasm_bindgen(js_name = appendFamilyTextWrite)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_family_text_write(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            reverse_member_order: bool,
            introducer: bool,
            remover: bool,
            reverse_rate_function: bool,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let options = Self::family_options(child_run_time, rate_function, lag_ratio)?
                .introducer(introducer)
                .remover(remover)
                .reverse_rate_function(reverse_rate_function);
            self.children
                .push(OrdinaryCompositionChild::FamilyTextWrite {
                    target: target.semantic_family()?,
                    entering: Vec::new(),
                    reverse_member_order,
                    options,
                });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilyTextWriteEntering)]
        pub fn append_family_text_write_entering(
            &mut self,
            object_id: &str,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::FamilyTextWrite {
                target, entering, ..
            }) = self.children.last_mut()
            else {
                return Err(js_error(
                    "family entering member must follow FamilyTextWrite",
                ));
            };
            if !std::rc::Rc::ptr_eq(
                target.integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error(
                    "Text family entering member belongs to another authoring store",
                ));
            }
            entering.push((
                parse_object_id("Text family object ID", object_id)?,
                member.semantic_mobject().clone(),
            ));
            Ok(())
        }

        /// Append one plain-Text Create/Uncreate. Rust owns reveal membership,
        /// lifecycle defaults, and glyph scheduling.
        #[wasm_bindgen(js_name = appendTextReveal)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_text_reveal(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            reverse: bool,
            introducer: Option<bool>,
            remover: Option<bool>,
            reverse_rate_function: Option<bool>,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let mut options = Self::family_options(child_run_time, rate_function, lag_ratio)?;
            if let Some(introducer) = introducer {
                options = options.introducer(introducer);
            }
            if let Some(remover) = remover {
                options = options.remover(remover);
            }
            if let Some(reverse_rate_function) = reverse_rate_function {
                options = options.reverse_rate_function(reverse_rate_function);
            }
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("Text reveal object ID", object_id)?)
            };
            self.children.push(OrdinaryCompositionChild::TextReveal {
                entering_id,
                target: target.semantic_mobject().clone(),
                reverse,
                options,
            });
            Ok(())
        }

        /// Append one shared-family Create/Uncreate. Rust resolves authoritative
        /// leaf order and member spans without frontend scheduling.
        #[wasm_bindgen(js_name = appendFamilyReveal)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_family_reveal(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            reverse: bool,
            introducer: Option<bool>,
            remover: Option<bool>,
            reverse_rate_function: Option<bool>,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            let mut options = Self::family_options(child_run_time, rate_function, lag_ratio)?;
            if let Some(introducer) = introducer {
                options = options.introducer(introducer);
            }
            if let Some(remover) = remover {
                options = options.remover(remover);
            }
            if let Some(reverse_rate_function) = reverse_rate_function {
                options = options.reverse_rate_function(reverse_rate_function);
            }
            self.children.push(OrdinaryCompositionChild::FamilyReveal {
                target: target.semantic_family()?,
                entering: Vec::new(),
                reverse,
                options,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilyRevealEntering)]
        pub fn append_family_reveal_entering(
            &mut self,
            object_id: &str,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::FamilyReveal {
                target, entering, ..
            }) = self.children.last_mut()
            else {
                return Err(js_error("family entering member must follow FamilyReveal"));
            };
            if !std::rc::Rc::ptr_eq(
                target.integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error(
                    "family reveal member belongs to another authoring store",
                ));
            }
            entering.push((
                parse_object_id("family reveal object ID", object_id)?,
                member.semantic_mobject().clone(),
            ));
            Ok(())
        }

        #[wasm_bindgen(js_name = appendAdd)]
        pub fn append_add(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let options = Self::options(child_run_time, rate_function)?;
            self.push_target(
                object_id,
                target,
                options,
                |entering_id, target, options| OrdinaryCompositionChild::Add {
                    entering_id,
                    target,
                    options,
                },
            )
        }

        #[wasm_bindgen(js_name = appendFade)]
        pub fn append_fade(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            scale_factor: f64,
            translation: &str,
            x: f64,
            y: f64,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let direction = parse_fade_direction(direction)?;
            let endpoint = parse_fade_endpoint(scale_factor, translation, x, y)?;
            let options = Self::options(child_run_time, rate_function)?;
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("object ID", object_id)?)
            };
            self.children.push(OrdinaryCompositionChild::Fade {
                entering_id,
                target: target.semantic_mobject().clone(),
                direction,
                endpoint,
                options,
            });
            Ok(())
        }

        /// Append a shared appearance/lifecycle fade over authoritative family leaves.
        #[wasm_bindgen(js_name = appendFamilyFade)]
        pub fn append_family_fade(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            direction: &str,
            child_run_time: Option<f64>,
            rate_function: Option<String>,
            lag_ratio: Option<f64>,
        ) -> Result<(), JsValue> {
            self.children.push(OrdinaryCompositionChild::FamilyFade {
                target: target.semantic_family()?,
                entering: Vec::new(),
                direction: parse_fade_direction(direction)?,
                options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendFamilyFadeEntering)]
        pub fn append_family_fade_entering(
            &mut self,
            object_id: &str,
            member: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let Some(OrdinaryCompositionChild::FamilyFade {
                target, entering, ..
            }) = self.children.last_mut()
            else {
                return Err(js_error("family entering member must follow FamilyFade"));
            };
            if !std::rc::Rc::ptr_eq(
                target.integration_store(),
                member.semantic_mobject().integration_store(),
            ) {
                return Err(js_error(
                    "fade family entering member belongs to another authoring store",
                ));
            }
            entering.push((
                parse_object_id("fade family object ID", object_id)?,
                member.semantic_mobject().clone(),
            ));
            Ok(())
        }

        #[wasm_bindgen(js_name = appendCreate)]
        pub fn append_create(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            let options = Self::options(child_run_time, rate_function)?;
            self.push_target(
                object_id,
                target,
                options,
                |entering_id, target, options| OrdinaryCompositionChild::Create {
                    entering_id: Some(entering_id),
                    target,
                    options,
                },
            )
        }

        #[wasm_bindgen(js_name = appendUncreate)]
        pub fn append_uncreate(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            child_run_time: f64,
            rate_function: &str,
            remover: bool,
            reverse_rate_function: bool,
        ) -> Result<(), JsValue> {
            let entering_id = if object_id.is_empty() {
                None
            } else {
                Some(parse_object_id("Uncreate object ID", object_id)?)
            };
            self.children.push(OrdinaryCompositionChild::Uncreate {
                entering_id,
                target: target.semantic_mobject().clone(),
                options: Self::options(child_run_time, rate_function)?
                    .remover(remover)
                    .reverse_rate_function(reverse_rate_function),
            });
            Ok(())
        }

        #[wasm_bindgen(js_name = appendAffineLifecycle)]
        #[allow(clippy::too_many_arguments)]
        pub fn append_affine_lifecycle(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            endpoint: &str,
            x: f64,
            y: f64,
            rotation_offset: f64,
            point_red: Option<f64>,
            point_green: Option<f64>,
            point_blue: Option<f64>,
            point_alpha: Option<f64>,
            child_run_time: f64,
            rate_function: &str,
        ) -> Result<(), JsValue> {
            self.push_entering_lifecycle(
                object_id,
                target,
                direction,
                endpoint,
                x,
                y,
                rotation_offset,
                point_red,
                point_green,
                point_blue,
                point_alpha,
                child_run_time,
                rate_function,
            )
        }

        #[wasm_bindgen(js_name = appendComposition)]
        pub fn append_composition(&mut self, nested: WasmAnimationCompositionBuilder) {
            self.children.push(OrdinaryCompositionChild::Composition {
                kind: nested.kind,
                children: nested.children,
                options: nested.composition_options,
            });
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
            self.tracker.detached_value().map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = setDetachedValue)]
        pub fn set_detached_value(&self, value: f64) -> Result<(), JsValue> {
            self.tracker
                .set_detached_value(value)
                .map_err(typed_js_error)
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
        #[wasm_bindgen(js_name = beginMembershipBatch)]
        pub fn begin_membership_batch(
            &self,
            kind: &str,
        ) -> Result<WasmSceneMembershipBatch, JsValue> {
            WasmSceneMembershipBatch::new(kind)
        }

        /// Consume one complete typed membership request and publish it once.
        #[wasm_bindgen(js_name = editMembership)]
        pub fn edit_membership(&mut self, batch: WasmSceneMembershipBatch) -> Result<(), JsValue> {
            self.inner
                .edit_membership(batch.inner)
                .map_err(typed_js_error)
        }

        /// Return the authoritative direct-root semantic identities in painter order.
        #[wasm_bindgen(js_name = rootMembershipKeys)]
        pub fn root_membership_keys(&self) -> Result<Vec<String>, JsValue> {
            self.inner.root_membership_keys().map_err(typed_js_error)
        }

        /// Query authoritative recursive membership without enumerating the scene.
        #[wasm_bindgen(js_name = containsMobject)]
        pub fn contains_mobject(
            &self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<bool, JsValue> {
            target.id_in_store(
                self.inner.scene.integration_store(),
                "scene membership query",
            )?;
            self.inner
                .contains_mobject(target.semantic_mobject())
                .map_err(typed_js_error)
        }

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
            let transform = noon::integration::rotate_effective_transform_about_point(
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

        /// Resolve a typed family only in this authoring store before a pinned read.
        #[wasm_bindgen(js_name = callbackFamilyKeys)]
        pub fn callback_family_keys(
            &self,
            handle: &crate::WasmAuthoringFamilyHandle,
            revision: &str,
        ) -> Result<Vec<String>, JsValue> {
            let family = handle.semantic_family()?;
            if !std::rc::Rc::ptr_eq(
                self.inner.scene.integration_store(),
                family.integration_store(),
            ) {
                return Err(typed_js_error(noon::AuthoringError::ForeignStore));
            }
            let revision = noon_core::SceneRevision::new(
                revision.parse::<u64>().map_err(callback_codec_error)?,
            );
            family
                .callback_leaf_nodes(revision)
                .map(|nodes| {
                    nodes
                        .into_iter()
                        .map(|node| format!("{}:{}", node.slot(), node.generation()))
                        .collect()
                })
                .map_err(typed_js_error)
        }

        /// One callback-boundary projection of an already prepared effective operation.
        /// Input rows are the pinned read cache plus preceding ordered overlay writes.
        #[wasm_bindgen(js_name = callbackFamilyPaint)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_family_paint(
            &self,
            handle: &crate::WasmAuthoringFamilyHandle,
            revision: &str,
            operation: &str,
            styles_json: &str,
            has_color: bool,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            width: Option<f64>,
            opacity: Option<f64>,
        ) -> Result<String, JsValue> {
            self.callback_family_keys(handle, revision)?;
            let family = handle.semantic_family()?;
            let color = crate::authoring_mobject::family_color(has_color, red, green, blue, alpha)
                .map_err(js_error)?;
            let operation = match operation {
                "Color" => noon::FamilyPaint::Color(
                    color.ok_or_else(|| js_error("family color is required"))?,
                ),
                "Fill" => noon::FamilyPaint::Fill { color, opacity },
                "Stroke" => noon::FamilyPaint::Stroke {
                    color,
                    width,
                    opacity,
                },
                "Opacity" => noon::FamilyPaint::Opacity(
                    opacity.ok_or_else(|| js_error("family opacity is required"))?,
                ),
                _ => return Err(js_error("unknown family paint operation")),
            };
            let revision = noon_core::SceneRevision::new(
                revision.parse::<u64>().map_err(callback_codec_error)?,
            );
            // This is the existing Python callback view/overlay codec boundary,
            // never an authored scene or a native/direct-WASM engine boundary.
            let rows: Vec<(u32, u32, Style)> =
                serde_json::from_str(styles_json).map_err(callback_codec_error)?;
            let styles: BTreeMap<_, _> = rows
                .into_iter()
                .map(|(slot, generation, style)| {
                    (noon_core::SemanticNodeId::new(slot, generation), style)
                })
                .collect();
            let changes = family
                .prepare_callback_paint(revision, operation, |node| {
                    styles
                        .get(&node)
                        .copied()
                        .ok_or(noon::ExecutionSessionCallbackError::UnknownObject(node))
                })
                .map_err(typed_js_error)?;
            let rows: Vec<_> = changes
                .into_iter()
                .map(|(node, style)| (node.slot(), node.generation(), style))
                .collect();
            serde_json::to_string(&rows).map_err(callback_codec_error)
        }

        /// Prepare a family translation over the existing callback read/overlay
        /// codec boundary. Native/direct-WASM callbacks use the same typed Rust
        /// operation; this function introduces no worker message or runtime.
        #[wasm_bindgen(js_name = callbackFamilyShift)]
        pub fn callback_family_shift(
            &self,
            handle: &crate::WasmAuthoringFamilyHandle,
            revision: &str,
            rows_json: &str,
            x: f64,
            y: f64,
        ) -> Result<String, JsValue> {
            self.callback_family_keys(handle, revision)?;
            let family = handle.semantic_family()?;
            let revision = noon_core::SceneRevision::new(
                revision.parse::<u64>().map_err(callback_codec_error)?,
            );
            let rows: Vec<(u32, u32, Transform2D, Option<noon_core::Rect>)> =
                serde_json::from_str(rows_json).map_err(callback_codec_error)?;
            let rows: BTreeMap<_, _> = rows
                .into_iter()
                .map(|(slot, generation, transform, bounds)| {
                    (
                        noon_core::SemanticNodeId::new(slot, generation),
                        (transform, bounds),
                    )
                })
                .collect();
            let changes = family
                .prepare_callback_translation(revision, x, y, |node| {
                    rows.get(&node)
                        .copied()
                        .ok_or(noon::ExecutionSessionCallbackError::UnknownObject(node))
                })
                .map_err(|error| match error {
                    // Consume the existing selection/read mapper; do not replace
                    // #1350's callback mapping or #1354's producer signatures.
                    noon::FamilyCallbackTranslationError::Family(error) => typed_js_error(error),
                    noon::FamilyCallbackTranslationError::Translation(error) => {
                        typed_js_error(AuthoringFailure::new(
                            "invalid_input",
                            "callback.family.translation",
                            error,
                        ))
                    }
                })?;
            let rows: Vec<_> = changes
                .into_iter()
                .map(|change| {
                    (
                        change.node.slot(),
                        change.node.generation(),
                        change.transform,
                        change.bounds,
                    )
                })
                .collect();
            serde_json::to_string(&rows).map_err(callback_codec_error)
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
                noon::integration::effective_style_with_color(style, red, green, blue, alpha)
                    .map_err(js_error)?,
            ))
        }

        /// Apply shared paint opacity while leaving whole-object opacity with its owner.
        #[wasm_bindgen(js_name = callbackPaintSetOpacity)]
        #[allow(clippy::too_many_arguments)]
        pub fn callback_paint_set_opacity(
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
                noon::integration::effective_style_with_paint_opacity(style, opacity)
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
                noon::integration::effective_style_with_stroke_color(
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
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = callbackMatchLineTransform)]
        pub fn callback_match_line_transform(
            &self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &WasmCallbackLineTarget,
        ) -> Result<WasmCallbackTransform, JsValue> {
            source.id_in_store(
                self.inner.scene.integration_store(),
                "Line.match_points source",
            )?;
            if !std::rc::Rc::ptr_eq(self.inner.scene.integration_store(), &target.store) {
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
                .map_err(typed_js_error)
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
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = createValueTracker)]
        pub fn create_value_tracker(
            &mut self,
            initial: f64,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self
                .inner
                .create_value_tracker(initial)
                .map_err(typed_js_error)?;
            Ok(WasmValueTrackerHandle::from_tracker(
                tracker,
                std::rc::Rc::clone(self.inner.scene.integration_store()),
            ))
        }

        #[wasm_bindgen(js_name = associateValueTracker)]
        pub fn associate_value_tracker(
            &mut self,
            tracker: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
            self.inner
                .associate_value_tracker(tracker)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = pointerPositionSignal)]
        pub fn pointer_position_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self
                .inner
                .pointer_position_signal()
                .map_err(typed_js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = viewportSizeSignal)]
        pub fn viewport_size_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self.inner.viewport_size_signal().map_err(typed_js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = wheelDeltaSignal)]
        pub fn wheel_delta_signal(&mut self) -> Result<WasmNativeVectorSignalHandle, JsValue> {
            let signal = self.inner.wheel_delta_signal().map_err(typed_js_error)?;
            Ok(WasmNativeVectorSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
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
                .map_err(typed_js_error)?;
            Ok(WasmNativeBoolSignalHandle {
                signal,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = controlSignal)]
        pub fn control_signal(
            &mut self,
            name: String,
            initial: f64,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self
                .inner
                .control_signal(name, initial)
                .map_err(typed_js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
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
                .map_err(typed_js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = wheelEvents)]
        pub fn wheel_events(&mut self) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self.inner.wheel_events().map_err(typed_js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = controlCommitEvents)]
        pub fn control_commit_events(
            &mut self,
            name: String,
        ) -> Result<WasmValueTrackerHandle, JsValue> {
            let tracker = self
                .inner
                .control_commit_events(name)
                .map_err(typed_js_error)?;
            Ok(WasmValueTrackerHandle {
                tracker,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = bindNativeTranslation)]
        pub fn bind_native_translation(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmNativeVectorSignalHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(
                self.inner.scene.integration_store(),
                "native translation binding",
            )?;
            let signal = signal.signal_in(self.inner.scene.integration_store())?;
            self.inner
                .bind_native_translation(object.semantic_mobject(), signal)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = bindRotation)]
        pub fn bind_rotation(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.integration_store(), "rotation binding")?;
            let signal = signal.tracker_in(self.inner.scene.integration_store())?;
            self.inner
                .bind_rotation(object.semantic_mobject(), signal)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = bindOpacity)]
        pub fn bind_opacity(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmValueTrackerHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.integration_store(), "opacity binding")?;
            let signal = signal.tracker_in(self.inner.scene.integration_store())?;
            self.inner
                .bind_opacity(object.semantic_mobject(), signal)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = bindPresence)]
        pub fn bind_presence(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            signal: &WasmNativeBoolSignalHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(self.inner.scene.integration_store(), "presence binding")?;
            let signal = signal.signal_in(self.inner.scene.integration_store())?;
            self.inner
                .bind_presence(object.semantic_mobject(), signal)
                .map_err(typed_js_error)
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
            let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
            let position = self
                .inner
                .tracker_position(
                    tracker,
                    SemanticVec3::new(direction_x, direction_y, 0.0),
                    SemanticVec3::new(offset_x, offset_y, 0.0),
                )
                .map_err(typed_js_error)?;
            Ok(WasmTrackerPositionHandle {
                position,
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = bindTrackerPosition)]
        pub fn bind_tracker_position(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            position: &WasmTrackerPositionHandle,
        ) -> Result<(), JsValue> {
            object.id_in_store(
                self.inner.scene.integration_store(),
                "tracker position binding",
            )?;
            let position = position.position_in(self.inner.scene.integration_store())?;
            self.inner
                .bind_tracker_position(object.semantic_mobject(), position)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = valueTrackerValue)]
        pub fn value_tracker_value(
            &mut self,
            tracker: &WasmValueTrackerHandle,
        ) -> Result<f64, JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
            self.inner.tracker_value(tracker).map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = setValueTracker)]
        pub fn set_value_tracker(
            &mut self,
            tracker: &WasmValueTrackerHandle,
            value: f64,
        ) -> Result<(), JsValue> {
            let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
            self.inner
                .set_tracker_value(tracker, value)
                .map_err(typed_js_error)
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
            self.inner.ordinary_wait(duration).map_err(typed_js_error)
        }

        /// Begin one ordinary wait for an async continuation without fast-forwarding it.
        #[wasm_bindgen(js_name = beginOrdinaryWait)]
        pub fn begin_ordinary_wait(&mut self, duration: f64) -> Result<f64, JsValue> {
            self.inner
                .begin_ordinary_wait(duration)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = beginOrdinaryCompositionBuilder)]
        pub fn begin_ordinary_composition_builder(
            &self,
            kind: &str,
            composition_run_time: Option<f64>,
            composition_lag_ratio: f64,
            play_run_time: Option<f64>,
        ) -> Result<WasmAnimationCompositionBuilder, JsValue> {
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
            let mut play_options = noon_core::AnimationOptions::new();
            if let Some(run_time) = play_run_time {
                play_options = play_options.run_time(run_time);
            }
            Ok(WasmAnimationCompositionBuilder {
                kind,
                children: Vec::new(),
                composition_options,
                play_options,
            })
        }

        #[wasm_bindgen(js_name = ordinaryCanPlayComposition)]
        pub fn ordinary_can_play_composition(
            &self,
            candidate: &WasmAnimationCompositionBuilder,
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
            candidate: WasmAnimationCompositionBuilder,
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
            candidate: WasmAnimationCompositionBuilder,
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

        /// Read the returned owner's current frame without creating or advancing a player.
        #[wasm_bindgen(js_name = liveDebugFrameJson)]
        pub fn live_debug_frame_json(&mut self) -> Result<String, JsValue> {
            self.inner
                .active_live_player()
                .map(|player| player.debug_frame_json())
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

        #[wasm_bindgen(js_name = beginUnderline)]
        pub fn begin_underline(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            buff: f64,
        ) -> Result<crate::WasmManimGeometryOptions, JsValue> {
            self.inner
                .begin_underline(handle.semantic_mobject(), buff)
                .map(crate::WasmManimGeometryOptions::from_options)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectLayout)]
        pub fn query_mobject_layout(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<WasmMobjectLayoutObservation, JsValue> {
            let (center_x, center_y, width, height) = self
                .inner
                .mobject_layout(handle.semantic_mobject())
                .map_err(typed_js_error)?;
            Ok(WasmMobjectLayoutObservation {
                center_x,
                center_y,
                width,
                height,
            })
        }

        /// Read a family through the same live session that owns its effective leaves.
        #[wasm_bindgen(js_name = queryFamilyLayout)]
        pub fn query_family_layout(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<WasmMobjectLayoutObservation, JsValue> {
            let family = handle.semantic_family()?;
            let layout = self
                .inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_family_layout(&family)
                .map_err(typed_js_error)?;
            Ok(WasmMobjectLayoutObservation {
                center_x: layout.center.0,
                center_y: layout.center.1,
                width: layout.width,
                height: layout.height,
            })
        }

        #[wasm_bindgen(js_name = liveMoveFamilyToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_move_family_to_point(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            x: f64,
            y: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_family_to(
                    &family,
                    noon::LiveLayoutTarget::Point(x, y),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMoveFamilyToMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_move_family_to_mobject(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_family_to(
                    &family,
                    noon::LiveLayoutTarget::Mobject(target.semantic_mobject()),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMoveFamilyToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_move_family_to_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringFamilyHandle,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            let target = target.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_family_to(
                    &family,
                    noon::LiveLayoutTarget::Family(&target),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveRescaleToFit)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_rescale_to_fit(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            length: f64,
            dimension: u32,
            stretch: bool,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_rescale_to_fit(
                    &source.anchor,
                    length,
                    dimension.try_into().map_err(typed_js_error)?,
                    stretch,
                    pivot,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveReplaceLayout)]
        pub fn live_replace_layout(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            target: &crate::authoring_mobject::WasmLayoutAnchor,
            dimension: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_replace_layout(
                    &source.anchor,
                    &target.anchor,
                    dimension.try_into().map_err(typed_js_error)?,
                    stretch,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMatchDimSize)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_match_dim_size(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            target: &crate::authoring_mobject::WasmLayoutAnchor,
            dimension: u32,
            stretch: bool,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_match_dim_size(
                    &source.anchor,
                    &target.anchor,
                    dimension.try_into().map_err(typed_js_error)?,
                    stretch,
                    pivot,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveNextLayoutTo)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_next_layout_to(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            target: &crate::authoring_mobject::WasmLayoutAnchor,
            aligner: &crate::authoring_mobject::WasmLayoutAnchor,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_next_layout_to_aligned(
                    &source.anchor,
                    noon::LiveLayoutTarget::Anchor(&target.anchor),
                    &aligner.anchor,
                    noon::ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (edge_x, edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveNextLayoutToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_next_layout_to_point(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            x: f64,
            y: f64,
            aligner: &crate::authoring_mobject::WasmLayoutAnchor,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_next_layout_to_aligned(
                    &source.anchor,
                    noon::LiveLayoutTarget::Point(x, y),
                    &aligner.anchor,
                    noon::ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (edge_x, edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAlignFamilyOnFrame)]
        pub fn live_align_family_on_frame(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_align_family_on_frame(&family, (direction_x, direction_y), buff)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAlignFamilyToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_align_family_to_point(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            x: f64,
            y: f64,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_align_family_to(
                    &family,
                    noon::LiveLayoutTarget::Point(x, y),
                    (axis_x, axis_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAlignFamilyToMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_align_family_to_mobject(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_align_family_to(
                    &family,
                    noon::LiveLayoutTarget::Mobject(target.semantic_mobject()),
                    (axis_x, axis_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAlignFamilyToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_align_family_to_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringFamilyHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            let target = target.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_align_family_to(
                    &family,
                    noon::LiveLayoutTarget::Family(&target),
                    (axis_x, axis_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectPath)]
        pub fn query_mobject_path(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<crate::WasmPathQuery, JsValue> {
            self.inner
                .mobject_path_query(handle.semantic_mobject())
                .map(crate::WasmPathQuery::from_query)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectLineEndpoints)]
        pub fn query_mobject_line_endpoints(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<crate::WasmManimLineEndpoints, JsValue> {
            self.inner
                .mobject_line_endpoints(handle.semantic_mobject())
                .map(crate::WasmManimLineEndpoints::from_endpoints)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectFillOpacity)]
        pub fn query_mobject_fill_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<f64, JsValue> {
            self.inner
                .mobject_fill_opacity(handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectStrokeOpacity)]
        pub fn query_mobject_stroke_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<f64, JsValue> {
            self.inner
                .mobject_stroke_opacity(handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = queryMobjectFillColor)]
        pub fn query_mobject_fill_color(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Option<crate::WasmManimColor>, JsValue> {
            self.inner
                .mobject_fill_color(target.semantic_mobject())
                .map(|color| color.map(crate::WasmManimColor::from_color))
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = queryMobjectStrokeColor)]
        pub fn query_mobject_stroke_color(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<Option<crate::WasmManimColor>, JsValue> {
            self.inner
                .mobject_stroke_color(target.semantic_mobject())
                .map(|color| color.map(crate::WasmManimColor::from_color))
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = queryMobjectStrokeWidth)]
        pub fn query_mobject_stroke_width(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<f64, JsValue> {
            self.inner
                .mobject_stroke_width(target.semantic_mobject())
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveSetColorGradient)]
        pub fn live_set_color_gradient(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            values: &[f64],
        ) -> Result<(), JsValue> {
            let colors =
                crate::authoring_mobject::gradient_colors(values).map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_color_gradient(&target.semantic_mobject(), &colors)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveSetFamilyColorGradient)]
        pub fn live_set_family_color_gradient(
            &mut self,
            target: &crate::WasmAuthoringFamilyHandle,
            values: &[f64],
        ) -> Result<(), JsValue> {
            let colors =
                crate::authoring_mobject::gradient_colors(values).map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_color_gradient(&target.semantic_family()?, &colors)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = queryMobjectColor)]
        pub fn query_mobject_color(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<crate::WasmManimColor, JsValue> {
            self.inner
                .mobject_color(handle.semantic_mobject())
                .map(crate::WasmManimColor::from_color)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = declareLiveTransformTo)]
        pub fn declare_live_transform_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            run_time: f64,
            rate_function: &str,
        ) -> Result<WasmDeclaredAnimationHandle, JsValue> {
            source.id_in_store(
                self.inner.scene.integration_store(),
                "animation declaration",
            )?;
            target.id_in_store(
                self.inner.scene.integration_store(),
                "animation declaration",
            )?;
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
                store: std::rc::Rc::clone(self.inner.scene.integration_store()),
            })
        }

        #[wasm_bindgen(js_name = ordinaryPlayAffineLifecycle)]
        pub fn ordinary_play_affine_lifecycle(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            endpoint: &str,
            x: f64,
            y: f64,
            rotation_offset: f64,
            point_red: Option<f64>,
            point_green: Option<f64>,
            point_blue: Option<f64>,
            point_alpha: Option<f64>,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(
                self.inner.scene.integration_store(),
                "ordinary affine lifecycle",
            )?;
            let direction = parse_affine_lifecycle_direction(direction)?;
            let endpoint = parse_affine_lifecycle_endpoint(
                endpoint,
                x,
                y,
                rotation_offset,
                callback_color(
                    "point_color",
                    point_red,
                    point_green,
                    point_blue,
                    point_alpha,
                )?,
            )?;
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
                .ordinary_play_affine_lifecycle(
                    id,
                    target.semantic_mobject(),
                    direction,
                    endpoint,
                    options,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = beginOrdinaryAffineLifecycle)]
        pub fn begin_ordinary_affine_lifecycle(
            &mut self,
            object_id: &str,
            target: &crate::WasmAuthoringMobjectHandle,
            direction: &str,
            endpoint: &str,
            x: f64,
            y: f64,
            rotation_offset: f64,
            point_red: Option<f64>,
            point_green: Option<f64>,
            point_blue: Option<f64>,
            point_alpha: Option<f64>,
            run_time: f64,
            rate_function: &str,
        ) -> Result<f64, JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            target.id_in_store(
                self.inner.scene.integration_store(),
                "ordinary affine lifecycle",
            )?;
            let direction = parse_affine_lifecycle_direction(direction)?;
            let endpoint = parse_affine_lifecycle_endpoint(
                endpoint,
                x,
                y,
                rotation_offset,
                callback_color(
                    "point_color",
                    point_red,
                    point_green,
                    point_blue,
                    point_alpha,
                )?,
            )?;
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
                .begin_ordinary_affine_lifecycle(
                    id,
                    target.semantic_mobject(),
                    direction,
                    endpoint,
                    options,
                )
                .map_err(js_error)
        }

        /// Query shared root membership after an exact fade completion. Python
        /// uses it only to attach/detach its derived wrapper identity.
        #[wasm_bindgen(js_name = liveContainsMobject)]
        pub fn live_contains_mobject(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<bool, JsValue> {
            target.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .live_contains_mobject(target.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveTargetEditor)]
        pub fn live_target_editor(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            source.id_in_store(self.inner.scene.integration_store(), "live target editor")?;
            self.inner
                .live_target_editor(source.semantic_mobject())
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(typed_js_error)
        }

        /// Copy the complete family through one coherent live publication.
        #[wasm_bindgen(js_name = liveCopyFamily)]
        pub fn live_copy_family(
            &mut self,
            source: &crate::WasmAuthoringFamilyHandle,
            references: WasmSceneMembershipBatch,
        ) -> Result<crate::WasmFamilyCopy, JsValue> {
            let source = source.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_copy_family(
                    &source,
                    &references.copy_references().map_err(typed_js_error)?,
                )
                .map(crate::WasmFamilyCopy::from_copy)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveBecomeFamily)]
        pub fn live_become_family(
            &mut self,
            source: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringFamilyHandle,
            match_height: bool,
            match_width: bool,
            match_center: bool,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_become_family(
                    &source.semantic_family()?,
                    &target.semantic_family()?,
                    noon::ManimBecomeOptions {
                        match_height,
                        match_width,
                        match_center,
                        stretch,
                    },
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetPointsAsCorners)]
        pub fn live_set_points_as_corners(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            values: Vec<f64>,
        ) -> Result<(), JsValue> {
            let points = crate::authoring_geometry::points(&values)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_points_as_corners(source.semantic_mobject(), &points)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveStartNewPath)]
        pub fn live_start_new_path(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            let point = crate::authoring_geometry::point(point_x, point_y)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_start_new_path(source.semantic_mobject(), point)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAddLineTo)]
        pub fn live_add_line_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            let point = crate::authoring_geometry::point(point_x, point_y)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_add_line_to(source.semantic_mobject(), point)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAddQuadraticBezierCurveTo)]
        pub fn live_add_quadratic_bezier_curve_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            control_x: f64,
            control_y: f64,
            anchor_x: f64,
            anchor_y: f64,
        ) -> Result<(), JsValue> {
            let control = crate::authoring_geometry::point(control_x, control_y)?;
            let anchor = crate::authoring_geometry::point(anchor_x, anchor_y)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_add_quadratic_bezier_curve_to(source.semantic_mobject(), control, anchor)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAddCubicBezierCurveTo)]
        pub fn live_add_cubic_bezier_curve_to(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            control1_x: f64,
            control1_y: f64,
            control2_x: f64,
            control2_y: f64,
            anchor_x: f64,
            anchor_y: f64,
        ) -> Result<(), JsValue> {
            let control1 = crate::authoring_geometry::point(control1_x, control1_y)?;
            let control2 = crate::authoring_geometry::point(control2_x, control2_y)?;
            let anchor = crate::authoring_geometry::point(anchor_x, anchor_y)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_add_cubic_bezier_curve_to(
                    source.semantic_mobject(),
                    control1,
                    control2,
                    anchor,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveInsertNCurves)]
        pub fn live_insert_n_curves(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            additional: u32,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_insert_n_curves(object.semantic_mobject(), additional as usize)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveReverseDirection)]
        pub fn live_reverse_direction(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_reverse_direction(object.semantic_mobject())
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveSubcurve)]
        pub fn live_subcurve(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            a: f64,
            b: f64,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_subcurve(source.semantic_mobject(), a, b)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = livePointwiseBecomePartial)]
        pub fn live_pointwise_become_partial(
            &mut self,
            object: &crate::WasmAuthoringMobjectHandle,
            source: &crate::WasmAuthoringMobjectHandle,
            a: f64,
            b: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_pointwise_become_partial(
                    object.semantic_mobject(),
                    source.semantic_mobject(),
                    a,
                    b,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveClosePath)]
        pub fn live_close_path(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_close_path(source.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMatchPoints)]
        pub fn live_match_points(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_match_points(source.semantic_mobject(), target.semantic_mobject())
                .map_err(typed_js_error)
        }

        /// Replace one object's content and presentation through the shared semantic owner.
        #[wasm_bindgen(js_name = liveBecomeMobject)]
        pub fn live_become_mobject(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            other: &crate::WasmAuthoringMobjectHandle,
            match_height: bool,
            match_width: bool,
            match_center: bool,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.inner
                .live_become_mobject(
                    target.semantic_mobject(),
                    other.semantic_mobject(),
                    noon::ManimBecomeOptions {
                        match_height,
                        match_width,
                        match_center,
                        stretch,
                    },
                )
                .map_err(typed_js_error)
        }

        /// Publish a fully configured geometry through the current live session.
        #[wasm_bindgen(js_name = liveCreateManimGeometry)]
        pub fn live_create_manim_geometry(
            &mut self,
            candidate: crate::WasmManimGeometryOptions,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            self.inner
                .live_create_manim_geometry(candidate.options)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(typed_js_error)
        }

        /// Shape and publish one detached plain Text object through the current
        /// retained session. The object has no root membership or execution row
        /// until a later lifecycle operation admits it.
        #[wasm_bindgen(js_name = liveCreateManimText)]
        pub fn live_create_manim_text(
            &mut self,
            source: &str,
            font_family: &str,
            font_size: f64,
            line_spacing: f64,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            opacity: f64,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            let text =
                crate::authoring_mobject::manim_text(source, font_family, font_size, line_spacing)
                    .map_err(typed_js_error)?
                    .color(Color::rgba(
                        checked_f32("text red", red)?,
                        checked_f32("text green", green)?,
                        checked_f32("text blue", blue)?,
                        checked_f32("text alpha", alpha)?,
                    ))
                    .set_opacity(checked_f32("text opacity", opacity)?);
            self.inner
                .live_create_text(text)
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(typed_js_error)
        }

        /// Compile and publish one detached Typst or MathTypst object through
        /// the current retained session.
        #[wasm_bindgen(js_name = liveCreateManimTypst)]
        pub fn live_create_manim_typst(
            &mut self,
            source: &str,
            math: bool,
            font_size: f64,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            opacity: f64,
        ) -> Result<crate::WasmAuthoringMobjectHandle, JsValue> {
            let font_size = crate::authoring_mobject::text_authoring_f32("font size", font_size)
                .map_err(typed_js_error)?;
            let color = Color::rgba(
                checked_f32("text red", red)?,
                checked_f32("text green", green)?,
                checked_f32("text blue", blue)?,
                checked_f32("text alpha", alpha)?,
            );
            let opacity = checked_f32("text opacity", opacity)?;
            let result = if math {
                self.inner.live_create_math_typst(
                    noon::MathTypst::new(source)
                        .with_font_size(font_size)
                        .color(color)
                        .set_opacity(opacity),
                )
            } else {
                self.inner.live_create_typst(
                    noon::Typst::new(source)
                        .with_font_size(font_size)
                        .color(color)
                        .set_opacity(opacity),
                )
            };
            result
                .map(crate::WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = livePlayAnimation)]
        pub fn live_play_animation(
            &mut self,
            animation: &WasmDeclaredAnimationHandle,
        ) -> Result<f64, JsValue> {
            let declaration = animation.declaration_in(self.inner.scene.integration_store())?;
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
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAdvanceSegmentTo)]
        pub fn live_advance_segment_to(&mut self, time: f64) -> Result<bool, JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_advance_segment_to(time)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveCompleteSegment)]
        pub fn live_complete_segment(&mut self) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_complete_segment()
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveEvaluate)]
        pub fn live_evaluate(&mut self, time: f64) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_evaluate(time)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = prepareExecutionRun)]
        pub fn prepare_execution_run(&mut self) -> Result<(), JsValue> {
            self.inner.prepare_execution_run().map_err(js_error)
        }

        #[wasm_bindgen(js_name = prepareFamilySubsetDisplay)]
        pub fn prepare_family_subset_display(
            &mut self,
            family: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .prepare_family_subset_display(&family.semantic_family()?)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = liveSetTranslation)]
        pub fn live_set_translation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_translation(handle.semantic_mobject(), x, y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMoveToLayout)]
        pub fn live_move_to_layout(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            target: &crate::authoring_mobject::WasmLayoutAnchor,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_to(
                    handle.semantic_mobject(),
                    noon::LiveLayoutTarget::Anchor(&target.anchor),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMoveToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_move_to_point(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_to(
                    handle.semantic_mobject(),
                    noon::LiveLayoutTarget::Point(x, y),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveMoveToMobject)]
        pub fn live_move_to_mobject(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            target.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_move_to(
                    handle.semantic_mobject(),
                    noon::LiveLayoutTarget::Mobject(target.semantic_mobject()),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(typed_js_error)
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
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_fill(handle.semantic_mobject(), red, green, blue, opacity)
                .map_err(typed_js_error)
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
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_fill_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveDisableFill)]
        pub fn live_disable_fill(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_disable_fill(handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetFillOpacity)]
        pub fn live_set_fill_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_fill_opacity(handle.semantic_mobject(), opacity)
                .map_err(typed_js_error)
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
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(typed_js_error)
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
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_stroke(handle.semantic_mobject(), red, green, blue, opacity)
                .map_err(typed_js_error)
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
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_stroke_color(handle.semantic_mobject(), red, green, blue, alpha)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveDisableStroke)]
        pub fn live_disable_stroke(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_disable_stroke(handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetStrokeOpacity)]
        pub fn live_set_stroke_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_stroke_opacity(handle.semantic_mobject(), opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetOpacity)]
        pub fn live_set_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_opacity(handle.semantic_mobject(), opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetObjectOpacity)]
        pub fn live_set_object_opacity(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_object_opacity(handle.semantic_mobject(), opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveAdd)]
        pub fn live_add(
            &mut self,
            object_id: &str,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = parse_object_id("object ID", object_id)?;
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .live_add_mobject(id, handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveRemove)]
        pub fn live_remove(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .live_remove_mobject(handle.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveReplaceContent)]
        pub fn live_replace_content(
            &mut self,
            target: &crate::WasmAuthoringMobjectHandle,
            source: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            target.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            source.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .live_replace_content(target.semantic_mobject(), source.semantic_mobject())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveShift)]
        pub fn live_shift(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_shift(handle.semantic_mobject(), x, y)
                .map_err(typed_js_error)
        }

        /// Create a complete detached family through the current live publication.
        #[wasm_bindgen(js_name = liveCreateFamily)]
        pub fn live_create_family(
            &mut self,
            batch: WasmSceneMembershipBatch,
            z_index: f64,
        ) -> Result<crate::WasmAuthoringFamilyHandle, JsValue> {
            if batch.inner.kind != SceneMembershipBatchKind::Add {
                return Err(typed_js_error("family creation requires an add batch"));
            }
            let members = batch.inner.family_members().map_err(typed_js_error)?;
            self.inner
                .live_family(&members, z_index)
                .map(crate::WasmAuthoringFamilyHandle::from_semantic_family)
                .map_err(typed_js_error)
        }

        /// Commit all requested direct-member edits before returning wrapper decisions.
        #[wasm_bindgen(js_name = liveEditFamilyMembership)]
        pub fn live_edit_family_membership(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            batch: WasmSceneMembershipBatch,
        ) -> Result<Vec<u8>, JsValue> {
            let adding = match batch.inner.kind {
                SceneMembershipBatchKind::Add => true,
                SceneMembershipBatchKind::Remove => false,
                _ => return Err(typed_js_error("family membership requires add or remove")),
            };
            let family = handle.semantic_family()?;
            let members = batch.inner.family_members().map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_edit_family_members(&family, &members, adding)
                .map(|changed| changed.into_iter().map(u8::from).collect())
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveShiftFamily)]
        pub fn live_shift_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            if !std::rc::Rc::ptr_eq(
                self.inner.scene.integration_store(),
                family.integration_store(),
            ) {
                return Err(typed_js_error(
                    "family and canonical context belong to different authoring stores",
                ));
            }
            self.inner
                .live_shift_family(&family, x, y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetStyle)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_style(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
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
        ) -> Result<(), JsValue> {
            let update = crate::authoring_mobject::style_update(
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
            .map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_style(&source.semantic_mobject(), update)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveMatchStyle)]
        pub fn live_match_style(
            &mut self,
            source: &crate::WasmAuthoringMobjectHandle,
            target: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_match_style(&source.semantic_mobject(), &target.semantic_mobject())
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveSetFamilyStyle)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_family_style(
            &mut self,
            source: &crate::WasmAuthoringFamilyHandle,
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
        ) -> Result<(), JsValue> {
            let update = crate::authoring_mobject::style_update(
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
            .map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_style(&source.semantic_family()?, update)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveMatchFamilyStyle)]
        pub fn live_match_family_style(
            &mut self,
            source: &crate::WasmAuthoringFamilyHandle,
            target: &crate::WasmAuthoringFamilyHandle,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_match_family_style(&source.semantic_family()?, &target.semantic_family()?)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveSetFamilyColor)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_family_color(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_color(&family, red, green, blue, alpha)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetFamilyFill)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_family_fill(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            has_color: bool,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            let color = crate::authoring_mobject::family_color(has_color, red, green, blue, alpha)
                .map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_fill(&family, color, opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetFamilyStroke)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_family_stroke(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            has_color: bool,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            width: Option<f64>,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            let color = crate::authoring_mobject::family_color(has_color, red, green, blue, alpha)
                .map_err(typed_js_error)?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_stroke(&family, color, width, opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetFamilyOpacity)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_set_family_opacity(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            opacity: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;

            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_family_opacity(&family, opacity)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveArrangeFamilyInGrid)]
        pub fn live_arrange_family_in_grid(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            options: &crate::authoring_mobject::WasmFamilyGridOptions,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_arrange_family_in_grid(&family, &options.options)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveScaleFamily)]
        pub fn live_scale_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_family(&family, x, y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveArrangeFamily)]
        pub fn live_arrange_family(
            &mut self,
            handle: &crate::WasmAuthoringFamilyHandle,
            options: &crate::authoring_mobject::WasmFamilyArrangeOptions,
        ) -> Result<(), JsValue> {
            let family = handle.semantic_family()?;
            self.inner
                .live_arrange_family(&family, &options.options)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetScale)]
        pub fn live_set_scale(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_scale(handle.semantic_mobject(), x, y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveScale)]
        pub fn live_scale(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale(handle.semantic_mobject(), x, y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetRotation)]
        pub fn live_set_rotation(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            angle: f64,
        ) -> Result<(), JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_rotation(handle.semantic_mobject(), angle)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveScaleLayout)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_scale_layout(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            scale_x: f64,
            scale_y: f64,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_layout(&source.anchor, scale_x, scale_y, pivot)
                .map_err(typed_js_error)
        }
        #[wasm_bindgen(js_name = liveRotateLayout)]
        pub fn live_rotate_layout(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            angle: f64,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_rotate_layout(&source.anchor, angle, pivot)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveSetZIndex)]
        pub fn live_set_z_index(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            value: f64,
            family: bool,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_set_z_index(&source.anchor, value, family)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveFlipLayout)]
        #[allow(clippy::too_many_arguments)]
        pub fn live_flip_layout(
            &mut self,
            source: &crate::authoring_mobject::WasmLayoutAnchor,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_flip_layout(
                    &source.anchor,
                    noon::SemanticVec3::new(axis_x, axis_y, axis_z),
                    pivot,
                )
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveEffectiveMobject)]
        pub fn live_effective_mobject(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
        ) -> Result<WasmLiveMobjectState, JsValue> {
            handle.id_in_store(
                self.inner.scene.integration_store(),
                "live execution context",
            )?;
            Ok(WasmLiveMobjectState {
                state: self
                    .inner
                    .active_live_player()
                    .map_err(typed_js_error)?
                    .live_effective(handle.semantic_mobject())
                    .map_err(typed_js_error)?,
            })
        }

        #[wasm_bindgen(js_name = returnExecutionPlayer)]
        pub fn return_execution_player(
            &mut self,
            player: crate::SemanticExecutionPlayer,
        ) -> Result<(), JsValue> {
            self.inner
                .return_execution_player(player)
                .map_err(|rejected| WasmExecutionPlayerReturnError { rejected }.into_js_error())
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
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{
        AnimationOptions, Color, HostCallbackId, RateFunction, SemanticMutationTransaction,
        SemanticVec3, Vec2,
    };

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

    fn membership_mobject(id: u64, handle: &noon::Mobject) -> OwnedSceneMembershipMember {
        OwnedSceneMembershipMember::Mobject {
            wrapper_id: Some(ObjectId::new(id)),
            handle: handle.clone(),
        }
    }

    fn begin_request(
        context: &mut CanonicalAuthoringScene,
        child: OrdinaryCompositionChild,
    ) -> Result<f64, String> {
        context.begin_ordinary_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[child],
            AnimationOptions::new().rate_func(RateFunction::Linear),
            AnimationOptions::new(),
        )
    }

    fn play_request(
        context: &mut CanonicalAuthoringScene,
        child: OrdinaryCompositionChild,
    ) -> Result<f64, String> {
        context.ordinary_play_mixed_composition(
            noon_core::SemanticAnimationCompositionKind::Parallel,
            &[child],
            AnimationOptions::new().rate_func(RateFunction::Linear),
            AnimationOptions::new(),
        )
    }

    fn fade_request(
        entering_id: Option<ObjectId>,
        target: &noon::Mobject,
        direction: SemanticFadeDirection,
        endpoint: noon::FadeEndpoint,
        options: AnimationOptions,
    ) -> OrdinaryCompositionChild {
        OrdinaryCompositionChild::Fade {
            entering_id,
            target: target.clone(),
            direction,
            endpoint,
            options,
        }
    }

    fn create_request(
        entering_id: Option<ObjectId>,
        target: &noon::Mobject,
        options: AnimationOptions,
    ) -> OrdinaryCompositionChild {
        OrdinaryCompositionChild::Create {
            entering_id,
            target: target.clone(),
            options,
        }
    }

    fn uncreate_request(
        entering_id: Option<ObjectId>,
        target: &noon::Mobject,
        options: AnimationOptions,
    ) -> OrdinaryCompositionChild {
        OrdinaryCompositionChild::Uncreate {
            entering_id,
            target: target.clone(),
            options,
        }
    }

    fn create_requests(
        children: &[(ObjectId, noon::Mobject, AnimationOptions)],
    ) -> Vec<OrdinaryCompositionChild> {
        children
            .iter()
            .map(|(id, target, options)| create_request(Some(*id), target, *options))
            .collect()
    }

    #[test]
    fn family_argument_batches_reject_scene_binding_metadata() {
        let scene = noon::Scene::new();
        let object = scene.circle(0.2).unwrap();
        let revision = scene.integration_store().borrow().scene_revision();
        let mut batch = SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Mobject {
                wrapper_id: None,
                handle: object.clone(),
            }],
            bindings: Vec::new(),
        };
        assert_eq!(batch.family_members().unwrap().len(), 1);
        let family = batch
            .create_family(std::rc::Rc::clone(scene.integration_store()), 0.0)
            .unwrap();
        let committed = scene.integration_store().borrow().scene_revision();
        assert_eq!(committed, revision.checked_next().unwrap());
        assert_eq!(batch.edit_family(&family).unwrap(), vec![false]);
        batch.kind = SceneMembershipBatchKind::Remove;
        assert!(batch
            .create_family(std::rc::Rc::clone(scene.integration_store()), 0.0)
            .is_err());
        assert_eq!(batch.edit_family(&family).unwrap(), vec![true]);
        batch.kind = SceneMembershipBatchKind::Clear;
        let revision = scene.integration_store().borrow().scene_revision();
        assert!(batch.edit_family(&family).is_err());
        batch.bindings.push((ObjectId::new(1), object.clone()));
        assert!(batch.family_members().is_err());
        batch.bindings.clear();
        batch.members = vec![membership_mobject(1, &object)];
        assert!(batch.family_members().is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn canonical_membership_batch_commits_bindings_only_after_atomic_shared_edit() {
        let mut context = CanonicalAuthoringScene::default();
        let first = context.scene.circle(0.5).unwrap();
        let second = context.scene.square(0.5).unwrap();
        assert!(!context.contains_mobject(&first).unwrap());
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Add,
                members: vec![
                    membership_mobject(0, &first),
                    membership_mobject(1, &second),
                ],
                bindings: vec![
                    (ObjectId::new(0), first.clone()),
                    (ObjectId::new(1), second.clone()),
                ],
            })
            .unwrap();
        assert!(context.contains_mobject(&first).unwrap());
        assert!(context.contains_mobject(&second).unwrap());
        assert_eq!(
            context.root_membership_keys().unwrap(),
            vec![
                format!(
                    "{}:{}",
                    first.node_id().slot(),
                    first.node_id().generation()
                ),
                format!(
                    "{}:{}",
                    second.node_id().slot(),
                    second.node_id().generation()
                ),
            ]
        );

        let replacement = context.scene.rectangle(0.5, 1.0).unwrap();
        let before = context.root_membership_keys().unwrap();
        let error = context.edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Replace,
            members: vec![
                membership_mobject(0, &first),
                membership_mobject(0, &replacement),
            ],
            bindings: vec![
                (ObjectId::new(0), first.clone()),
                (ObjectId::new(0), replacement.clone()),
            ],
        });
        assert!(error.is_err());
        assert_eq!(context.root_membership_keys().unwrap(), before);
        assert!(!context.identities.contains_key(&replacement.node_id()));

        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Replace,
                members: vec![
                    membership_mobject(0, &first),
                    membership_mobject(2, &replacement),
                ],
                bindings: vec![
                    (ObjectId::new(0), first.clone()),
                    (ObjectId::new(2), replacement.clone()),
                ],
            })
            .unwrap();
        assert_eq!(
            context.root_membership_keys().unwrap(),
            vec![
                format!(
                    "{}:{}",
                    replacement.node_id().slot(),
                    replacement.node_id().generation()
                ),
                format!(
                    "{}:{}",
                    second.node_id().slot(),
                    second.node_id().generation()
                ),
            ]
        );
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Clear,
                members: Vec::new(),
                bindings: Vec::new(),
            })
            .unwrap();
        assert!(context.root_membership_keys().unwrap().is_empty());
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
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Clear,
                members: Vec::new(),
                bindings: Vec::new(),
            })
            .unwrap();
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
        let members = first.members().unwrap();
        let revision = store.borrow().scene_revision();
        assert!(first.create_camera_frame(ObjectId::new(5)).is_err());
        assert_eq!(first.members().unwrap(), members);
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
        let revision = first.scene.integration_store().borrow().scene_revision();
        assert!(first.bind_mobject(ObjectId::new(0), &foreign).is_err());
        assert!(first.members().unwrap().is_empty());
        assert_eq!(
            first.scene.integration_store().borrow().scene_revision(),
            revision
        );
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
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .live_add_mobject(ObjectId::new(0), &collision)
            .is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
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

        assert_eq!(
            context.bindings.get(&ObjectId::new(2)),
            Some(&appended.node_id())
        );
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

    #[test]
    fn mixed_bind_events_lower_to_one_ordered_execution_stream() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.5).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let label = context.scene.text(noon::Text::new("A")).unwrap();
        context.bind_mobject(ObjectId::new(1), &label).unwrap();
        let square = context.scene.rectangle(1.0, 1.0).unwrap();
        context.bind_mobject(ObjectId::new(2), &square).unwrap();

        let execution = context.lower_execution().unwrap();
        assert_eq!(
            execution
                .frame()
                .objects
                .iter()
                .map(|object| Some(object.id))
                .collect::<Vec<_>>(),
            [&circle, &label, &square]
                .map(|handle| execution.execution_object_id(handle.node_id()))
        );
        assert!(execution.frame().objects[1].text().is_some());
        let resource = label.state().unwrap().content.text().unwrap();
        let store = context.scene.integration_store().borrow();
        let text = store.text_resources().get(resource).unwrap();
        assert_eq!(text.kind, noon_core::TextSourceKind::Plain);
        assert_eq!(text.source.as_ref(), "A");
    }

    #[test]
    fn family_roots_lower_to_bound_semantic_leaves() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.circle(0.5).unwrap();
        let right = context.scene.square(0.5).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Add,
                members: vec![OwnedSceneMembershipMember::Family(family)],
                bindings: vec![
                    (ObjectId::new(4), left.clone()),
                    (ObjectId::new(9), right.clone()),
                ],
            })
            .unwrap();

        let execution = context.lower_execution().unwrap();
        assert_eq!(
            execution
                .frame()
                .objects
                .iter()
                .map(|object| Some(object.id))
                .collect::<Vec<_>>(),
            [&left, &right].map(|handle| execution.execution_object_id(handle.node_id()))
        );
    }

    #[test]
    fn native_text_layout_and_presentation_reach_typed_execution() {
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
        let execution = context.lower_execution().unwrap();
        assert!(execution.frame().objects[0].text().is_some());
        assert_eq!(
            execution.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        let state = label.state().unwrap();
        let store = context.scene.integration_store().borrow();
        let text = store
            .text_resources()
            .get(state.content.text().unwrap())
            .unwrap();
        assert_eq!(text.source.as_ref(), "A\nB");
        assert_eq!(text.runs.len(), 2);
        assert_eq!(text.runs[0].font_size, 36.0);
        assert!((text.runs[0].transform.ty - text.runs[1].transform.ty - 54.0).abs() < 1e-6);
        assert_eq!(
            state.transform.scale.x,
            f64::from(noon::integration::NATIVE_POINT_TO_SCENE_SCALE)
        );
    }

    #[test]
    fn typst_and_math_typst_share_resources_with_typed_execution() {
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
        let execution = context.lower_execution().unwrap();
        assert_eq!(execution.frame().objects.len(), 2);
        assert!(execution
            .frame()
            .objects
            .iter()
            .all(|object| object.text().is_some()));
        assert_eq!(
            execution.frame().objects[0].transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(
            execution.frame().objects[1].transform.translation,
            Vec2::new(-1.0, 0.5)
        );
        assert_eq!(
            label.state().unwrap().style.fill,
            Some(noon_core::SemanticPaint::Solid(noon_core::YELLOW))
        );
        assert_eq!(equation.state().unwrap().style.object_opacity, 0.5);
        let store = context.scene.integration_store().borrow();
        for (handle, kind, source) in [
            (&label, noon_core::TextSourceKind::Typst, "*Noon*"),
            (
                &equation,
                noon_core::TextSourceKind::MathTypst,
                "frac(x, 2)",
            ),
        ] {
            let resource = handle.state().unwrap().content.text().unwrap();
            let text = store.text_resources().get(resource).unwrap();
            assert_eq!(text.kind, kind);
            assert_eq!(text.source.as_ref(), source);
        }
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
        let store = std::rc::Rc::clone(context.scene.integration_store());

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
        let revision = context.scene.integration_store().borrow().scene_revision();

        let mut target = context.live_target_editor(&circle).unwrap();
        target.set_translation(2.0, -1.0).unwrap();

        assert!(context.player_ownership.is_unstarted());
        assert!(
            context
                .scene
                .integration_store()
                .borrow()
                .scene_revision()
                .get()
                > revision.get(),
            "the detached authored target must be published without bootstrapping a player"
        );
        assert_eq!(
            target.state().unwrap().transform.translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
    }

    #[test]
    fn live_become_preserves_ownership_across_handoff_and_return() {
        let mut context = CanonicalAuthoringScene::default();
        let source = context.scene.circle(0.5).unwrap();
        let mut target = context.scene.rectangle(2.0, 1.0).unwrap();
        target.set_translation(3.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &source).unwrap();
        context.live_player(1.0).unwrap();
        let id = source.node_id();
        let target_before = target.state().unwrap();
        context
            .live_become_mobject(
                &source,
                &target,
                noon::ManimBecomeOptions {
                    match_height: true,
                    match_center: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(source.node_id(), id);
        assert_eq!(source.center().unwrap(), (0.0, 0.0));
        assert_eq!(source.width().unwrap(), 2.0);
        assert_eq!(target.state().unwrap(), target_before);

        let handed_off = context.take_execution_player(1.0, 41).unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();
        let before = source.state().unwrap();
        let error = context
            .live_become_mobject(&source, &target, Default::default())
            .unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("unclassified", "unclassified")
        );
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(source.state().unwrap(), before);
        context.return_execution_player(handed_off).unwrap();
        context
            .live_become_mobject(&source, &target, Default::default())
            .unwrap();
        assert_eq!(source.center().unwrap(), (3.0, -1.0));
        assert_eq!(context.live_execution_ownership(), "returned");
        context.live_target_editor(&source).unwrap();
    }

    #[test]
    fn live_family_target_publication_keeps_the_execution_owner_coherent() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context.live_player(1.0).unwrap();
        let target = context.live_target_editor(&circle).unwrap();
        let family = context
            .live_family(&[noon::MobjectFamilyMember::Mobject(&target)], 0.0)
            .unwrap();
        let nested = context
            .live_family(&[noon::MobjectFamilyMember::Family(&family)], 0.0)
            .unwrap();
        assert_eq!(
            context
                .scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(nested.node_id())
                .unwrap(),
            &[family.node_id()]
        );
        let foreign = noon::Scene::new().circle(0.2).unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .live_family(&[noon::MobjectFamilyMember::Mobject(&foreign)], 0.0)
            .is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        // Another owner-mediated mutation remains valid after both publications
        // and the rejected foreign member: no stale execution revision is hidden.
        context.live_target_editor(&circle).unwrap();
    }

    #[test]
    fn live_created_group_arrange_and_target_preserve_returned_player_after_prior_plays() {
        let mut context = CanonicalAuthoringScene::default();
        let anchor = context.scene.circle(0.3).unwrap();
        context.bind_mobject(ObjectId::new(0), &anchor).unwrap();
        let options = AnimationOptions::new()
            .run_time(0.5)
            .rate_func(RateFunction::Linear);

        let mut first_target = anchor.target_editor().unwrap();
        first_target.set_translation(0.5, 0.0).unwrap();
        play_request(
            &mut context,
            bound_transform_child(&anchor, first_target.clone(), options),
        )
        .unwrap();
        let second_target = context.live_target_editor(&anchor).unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_set_translation(&second_target, 1.0, 0.0)
            .unwrap();
        play_request(
            &mut context,
            bound_transform_child(&anchor, second_target.clone(), options),
        )
        .unwrap();
        let handoff = context.live_handoff_duration().unwrap();
        let returned = context.take_execution_player(handoff, 17).unwrap();
        context.return_execution_player(returned).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");

        let left = context
            .live_create_manim_geometry(noon::ManimGeometryOptions::circle(0.15).unwrap())
            .unwrap();
        let right = context
            .live_create_manim_geometry(noon::ManimGeometryOptions::square(0.3).unwrap())
            .unwrap();
        let batch = SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![
                OwnedSceneMembershipMember::Mobject {
                    wrapper_id: None,
                    handle: left.clone(),
                },
                OwnedSceneMembershipMember::Mobject {
                    wrapper_id: None,
                    handle: right.clone(),
                },
            ],
            bindings: Vec::new(),
        };
        let pair = context
            .live_family(&batch.family_members().unwrap(), 0.0)
            .unwrap();
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_edit_family_members(&pair, &[(&right).into()], false)
                .unwrap(),
            vec![true]
        );
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_edit_family_members(&pair, &[(&right).into(), (&right).into()], true)
                .unwrap(),
            vec![false, true]
        );
        context
            .live_arrange_family(
                &pair,
                &noon::FamilyArrangeOptions::new(1.0, 0.0, 0.15, true),
            )
            .unwrap();
        let player = context.active_live_player().unwrap();
        let before = player.live_family_layout(&pair).unwrap();
        player
            .live_move_family_to(
                &pair,
                noon::LiveLayoutTarget::Point(before.center.0, before.center.1),
                (0.0, 0.0),
                (1.0, 1.0),
            )
            .unwrap();
        let placement = noon::LayoutAnchor::from(&pair);
        player
            .live_next_layout_to_aligned(
                &placement,
                noon::LiveLayoutTarget::Point(before.center.0, before.center.1),
                &placement,
                noon::ManimNextToArgs {
                    direction: (0.0, 0.0),
                    buff: 0.0,
                    aligned_edge: (0.0, 0.0),
                    mask: (1.0, 1.0),
                },
            )
            .unwrap();
        player
            .live_align_family_to(&pair, noon::LiveLayoutTarget::Family(&pair), (1.0, 1.0))
            .unwrap();
        assert_eq!(player.live_family_layout(&pair).unwrap(), before);

        let arranged_left = left.center().unwrap();
        let arranged_right = right.center().unwrap();
        assert!((arranged_right.0 - arranged_left.0 - 0.45).abs() < 1e-6);
        assert_eq!(context.live_execution_ownership(), "returned");
        // Repeated explicit `Scene.live_execution()` helpers may adjust the loop
        // duration, but must preserve the returned continuation lease.
        context.live_player(handoff).unwrap();
        context.live_add_mobject(ObjectId::new(1), &left).unwrap();
        context.live_player(handoff).unwrap();
        context.live_add_mobject(ObjectId::new(2), &right).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");

        let copied = context
            .active_live_player()
            .unwrap()
            .live_copy_family(&pair, &[])
            .unwrap();
        let left_target = copied.mobject(&left).unwrap();
        let right_target = copied.mobject(&right).unwrap();
        assert_ne!(left_target.node_id(), left.node_id());
        assert_ne!(right_target.node_id(), right.node_id());
        let target_pair = copied.root().clone();
        context.live_shift_family(&target_pair, 0.0, 1.0).unwrap();

        let family_play = [OrdinaryCompositionChild::FamilyTransformTo {
            source: pair,
            target_state: target_pair,
            options: AnimationOptions::new()
                .run_time(1.2)
                .rate_func(RateFunction::Smooth)
                .lag_ratio(0.5),
        }];
        let end = context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &family_play,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        let mut resumed = context.resume_execution_player().unwrap();
        resumed.live_advance_segment_to(end).unwrap();
        resumed.live_complete_segment().unwrap();
        assert_eq!(
            resumed
                .live_effective(&left)
                .unwrap()
                .transform
                .translation
                .y,
            1.0
        );
        assert!(
            (f64::from(
                resumed
                    .live_effective(&left)
                    .unwrap()
                    .transform
                    .translation
                    .x,
            ) - arranged_left.0)
                .abs()
                < 1e-6
        );
        assert_eq!(
            resumed
                .live_effective(&right)
                .unwrap()
                .transform
                .translation
                .y,
            1.0
        );
        assert!(
            (f64::from(
                resumed
                    .live_effective(&right)
                    .unwrap()
                    .transform
                    .translation
                    .x,
            ) - arranged_right.0)
                .abs()
                < 1e-6
        );
        context.return_execution_player(resumed).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
    }

    #[test]
    fn target_editor_rejects_a_transferred_player_without_authored_fallback() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context.live_player(1.0).unwrap();
        let player = context.take_execution_player(1.0, 17).unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();

        assert!(context.live_target_editor(&circle).is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
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
            play_request(
                &mut context,
                bound_transform_child(&circle, first_target.clone(), options)
            )
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
            play_request(
                &mut context,
                bound_transform_child(&circle, second_target.clone(), second_options)
            )
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
        let revision = context.scene.integration_store().borrow().scene_revision();

        context
            .validate_ordinary_mixed_composition(&children, composition, play)
            .unwrap();
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let mut unsupported_target = right.target_editor().unwrap();
        unsupported_target.set_stroke_cap("round").unwrap();
        let unsupported = [bound_transform_child(&right, unsupported_target, child)];
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &unsupported,
                composition,
                play,
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

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
    fn cleared_text_reenters_composition_with_its_existing_wrapper_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let label = context.scene.text(noon::Text::new("stable")).unwrap();
        let id = ObjectId::new(7);
        context.bind_mobject(id, &label).unwrap();
        context.live_player(1.0).unwrap();
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::Clear,
                members: Vec::new(),
                bindings: Vec::new(),
            })
            .unwrap();
        assert!(!context.live_contains_mobject(&label).unwrap());
        assert_eq!(context.bindings.get(&id), Some(&label.node_id()));
        assert_eq!(context.identities.get(&label.node_id()), Some(&id));

        let target = context.live_target_editor(&label).unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_shift(&target, 2.0, -1.0)
            .unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let child = OrdinaryCompositionChild::TransformTo {
            entering_id: Some(id),
            source: label.clone(),
            target,
            interpolation: noon_core::SemanticTransformInterpolation::Affine,
            options,
        };
        context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &[child],
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();

        assert!(context.live_contains_mobject(&label).unwrap());
        assert_eq!(context.bindings.get(&id), Some(&label.node_id()));
        assert_eq!(context.identities.get(&label.node_id()), Some(&id));
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&label)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0),
        );
    }

    #[test]
    fn focus_on_creates_and_removes_a_spotlight_without_wrapper_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let revision = context.scene.integration_store().borrow().scene_revision();
        let options = AnimationOptions::new().rate_func(RateFunction::Linear);
        let child = |opacity| OrdinaryCompositionChild::FocusOn {
            focus: noon::FocusOnOptions {
                opacity,
                ..noon::FocusOnOptions::new((2.0, 1.0))
            },
            options,
        };
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &[child(-1.0)],
                options,
                AnimationOptions::new()
            )
            .is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &[child(0.2)],
                    options,
                    AnimationOptions::new()
                )
                .unwrap(),
            2.0
        );
        assert!(context.members().unwrap().is_empty());
        assert!(context.bindings.is_empty());
    }

    #[test]
    fn passing_flash_publishes_wrapper_binding_only_after_atomic_admission() {
        let mut context = CanonicalAuthoringScene::default();
        let line = context.scene.line((-2.0, 0.0), (2.0, 0.0)).unwrap();
        let id = ObjectId::new(0);
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear)
            .introducer(true)
            .remover(true);
        let composition = AnimationOptions::new().rate_func(RateFunction::Linear);
        let revision = context.scene.integration_store().borrow().scene_revision();
        let invalid = OrdinaryCompositionChild::PassingFlash {
            entering_id: Some(id),
            target: line.clone(),
            time_width: 0.0,
            options,
        };
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &[invalid],
                composition,
                AnimationOptions::new(),
            )
            .is_err());
        assert!(context.bindings.is_empty());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let valid = OrdinaryCompositionChild::PassingFlash {
            entering_id: Some(id),
            target: line.clone(),
            time_width: 0.25,
            options,
        };
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &[valid],
                    composition,
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        assert!(!context.live_contains_mobject(&line).unwrap());
        assert_eq!(context.bindings.get(&id), Some(&line.node_id()));
        assert_eq!(context.identities.get(&line.node_id()), Some(&id));
    }

    #[test]
    fn ordinary_draw_border_then_fill_leaf_and_family_publish_atomically() {
        let mut context = CanonicalAuthoringScene::default();
        let leaf = context.scene.circle(0.3).unwrap();
        let left = context.scene.square(0.5).unwrap();
        let right = context.scene.circle(0.25).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        let outline = noon::DrawBorderThenFillOptions::new(0.04, Some(noon_core::YELLOW))
            .with_phase_rate_function(RateFunction::Linear);
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear)
            .introducer(true);
        let composition = AnimationOptions::new().rate_func(RateFunction::Linear);

        // Missing one detached family identity is rejected before a player,
        // semantic admission, or facade binding can be published.
        let invalid = [OrdinaryCompositionChild::FamilyDrawBorderThenFill {
            target: family.clone(),
            entering: vec![(ObjectId::new(1), left.clone())],
            outline,
            options,
        }];
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &invalid,
                composition,
                AnimationOptions::new(),
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(context.bindings.is_empty());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let valid = [
            OrdinaryCompositionChild::DrawBorderThenFill {
                entering_id: Some(ObjectId::new(0)),
                target: leaf.clone(),
                outline,
                options,
            },
            OrdinaryCompositionChild::FamilyDrawBorderThenFill {
                target: family,
                entering: vec![
                    (ObjectId::new(1), left.clone()),
                    (ObjectId::new(2), right.clone()),
                ],
                outline,
                options: options.lag_ratio(0.5),
            },
        ];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &valid,
                    composition,
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        for target in [&leaf, &left, &right] {
            assert!(context.live_contains_mobject(target).unwrap());
        }
        assert_eq!(context.bindings.len(), 3);
        let publication: crate::RetainedExecutionDeltaEnvelope = serde_json::from_str(
            &context
                .active_live_player()
                .unwrap()
                .initial_delta_json()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(publication.objects.len(), 3);
    }

    #[test]
    fn ordinary_text_write_composes_with_a_distinct_text_transform_atomically() {
        let mut context = CanonicalAuthoringScene::default();
        let moving = context.scene.text(noon::Text::new("MOVE")).unwrap();
        let writing = context.scene.text(noon::Text::new("WRITE")).unwrap();
        context.bind_mobject(ObjectId::new(0), &moving).unwrap();
        let mut moving_target = moving.target_editor().unwrap();
        moving_target.shift(1.0, 0.0).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [
            bound_transform_child(&moving, moving_target, options),
            OrdinaryCompositionChild::TextWrite {
                entering_id: Some(ObjectId::new(1)),
                target: writing.clone(),
                reverse_member_order: false,
                options: AnimationOptions::new(),
            },
        ];

        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &children,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    options,
                )
                .unwrap(),
            1.0
        );
        assert!(context.live_contains_mobject(&writing).unwrap());
        assert_eq!(
            context
                .active_live_player()
                .unwrap()
                .live_effective(&moving)
                .unwrap()
                .transform
                .translation,
            Vec2::new(1.0, 0.0)
        );
        assert_eq!(
            context.bindings.get(&ObjectId::new(1)),
            Some(&writing.node_id())
        );

        let unwrite = [OrdinaryCompositionChild::TextWrite {
            entering_id: None,
            target: writing.clone(),
            reverse_member_order: true,
            options: options
                .introducer(false)
                .remover(true)
                .reverse_rate_function(true),
        }];
        context
            .validate_ordinary_mixed_composition(
                &unwrite,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .unwrap();
        context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &unwrite,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .unwrap();
        assert!(!context.live_contains_mobject(&writing).unwrap());

        let mut rejected = CanonicalAuthoringScene::default();
        let shared = rejected.scene.text(noon::Text::new("ONE")).unwrap();
        let mut target = shared.target_editor().unwrap();
        target.shift(1.0, 0.0).unwrap();
        let conflicting = [
            OrdinaryCompositionChild::TextWrite {
                entering_id: Some(ObjectId::new(0)),
                target: shared.clone(),
                reverse_member_order: false,
                options: AnimationOptions::new(),
            },
            OrdinaryCompositionChild::TransformTo {
                entering_id: Some(ObjectId::new(1)),
                source: shared,
                target,
                interpolation: noon_core::SemanticTransformInterpolation::Affine,
                options,
            },
        ];
        let revision = rejected.scene.integration_store().borrow().scene_revision();
        assert!(rejected
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &conflicting,
                AnimationOptions::new(),
                options,
            )
            .is_err());
        assert!(rejected.player_ownership.is_unstarted());
        assert!(rejected.bindings.is_empty());
        assert_eq!(
            rejected.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn ordinary_text_family_fade_composes_with_disjoint_text_write() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.text(noon::Text::new("LEFT")).unwrap();
        let right = context.scene.text(noon::Text::new("RIGHT")).unwrap();
        let writing = context.scene.text(noon::Text::new("WRITE")).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        let family_options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear)
            .lag_ratio(0.25);
        let write_options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let entering = [
            OrdinaryCompositionChild::FamilyFade {
                target: family.clone(),
                entering: vec![
                    (ObjectId::new(0), left.clone()),
                    (ObjectId::new(1), right.clone()),
                ],
                direction: SemanticFadeDirection::In,
                options: family_options,
            },
            OrdinaryCompositionChild::TextWrite {
                entering_id: Some(ObjectId::new(2)),
                target: writing.clone(),
                reverse_member_order: false,
                options: write_options,
            },
        ];

        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &entering,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        for target in [&left, &right, &writing] {
            assert!(context.contains_mobject(target).unwrap());
        }
        assert_eq!(context.bindings.len(), 3);

        let leaving = [OrdinaryCompositionChild::FamilyFade {
            target: family,
            entering: Vec::new(),
            direction: SemanticFadeDirection::Out,
            options: AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.25),
        }];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &leaving,
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .unwrap(),
            3.0
        );
        assert!(!context.contains_mobject(&left).unwrap());
        assert!(!context.contains_mobject(&right).unwrap());
        assert!(context.contains_mobject(&writing).unwrap());
    }

    #[test]
    fn ordinary_text_family_write_binds_and_removes_recursive_membership() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.text(noon::Text::new("LEFT")).unwrap();
        let right = context.scene.text(noon::Text::new("RIGHT")).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        let write = [OrdinaryCompositionChild::FamilyTextWrite {
            target: family.clone(),
            entering: vec![
                (ObjectId::new(0), left.clone()),
                (ObjectId::new(1), right.clone()),
            ],
            reverse_member_order: false,
            options: AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.2)
                .introducer(true),
        }];

        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &write,
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        assert!(context.contains_mobject(&left).unwrap());
        assert!(context.contains_mobject(&right).unwrap());
        assert_eq!(context.bindings.len(), 2);

        let unwrite = [OrdinaryCompositionChild::FamilyTextWrite {
            target: family,
            entering: Vec::new(),
            reverse_member_order: true,
            options: AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.2)
                .introducer(false)
                .remover(true)
                .reverse_rate_function(true),
        }];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &unwrite,
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .unwrap(),
            3.0
        );
        assert!(!context.contains_mobject(&left).unwrap());
        assert!(!context.contains_mobject(&right).unwrap());
    }

    #[test]
    fn ordinary_text_reveal_and_mixed_family_reveal_publish_recursive_membership() {
        let mut context = CanonicalAuthoringScene::default();
        let single = context.scene.text(noon::Text::new("SINGLE")).unwrap();
        let text = context.scene.text(noon::Text::new("GROUP")).unwrap();
        let circle = context.scene.circle(0.25).unwrap();
        let family = context
            .scene
            .family(&[(&text).into(), (&circle).into()])
            .unwrap();
        let create_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth)
            .lag_ratio(1.0)
            .introducer(true);
        let create = [
            OrdinaryCompositionChild::TextReveal {
                entering_id: Some(ObjectId::new(0)),
                target: single.clone(),
                reverse: false,
                options: create_options,
            },
            OrdinaryCompositionChild::FamilyReveal {
                target: family.clone(),
                entering: vec![
                    (ObjectId::new(1), text.clone()),
                    (ObjectId::new(2), circle.clone()),
                ],
                reverse: false,
                options: create_options,
            },
        ];

        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &create,
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .unwrap(),
            1.0
        );
        for target in [&single, &text, &circle] {
            assert!(context.contains_mobject(target).unwrap());
        }
        assert_eq!(context.bindings.len(), 3);

        let uncreate_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth)
            .lag_ratio(1.0)
            .introducer(false)
            .remover(true)
            .reverse_rate_function(true);
        let uncreate = [
            OrdinaryCompositionChild::TextReveal {
                entering_id: None,
                target: single.clone(),
                reverse: true,
                options: uncreate_options,
            },
            OrdinaryCompositionChild::FamilyReveal {
                target: family,
                entering: Vec::new(),
                reverse: true,
                options: uncreate_options,
            },
        ];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &uncreate,
                    AnimationOptions::new(),
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        for target in [&single, &text, &circle] {
            assert!(!context.contains_mobject(target).unwrap());
        }
    }

    #[test]
    fn ordinary_subset_display_prepares_and_publishes_family_atomically() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.square(0.5).unwrap();
        let right = context.scene.circle(0.25).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        context.prepare_family_subset_display(&family).unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let composition = AnimationOptions::new().rate_func(RateFunction::Linear);

        let invalid = [OrdinaryCompositionChild::FamilySubsetDisplay {
            target: family.clone(),
            entering: vec![(ObjectId::new(0), left.clone())],
            mode: noon::SubsetDisplayMode::IncreasingFloor,
            options,
        }];
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &invalid,
                composition,
                AnimationOptions::new(),
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(context.bindings.is_empty());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let valid = [OrdinaryCompositionChild::FamilySubsetDisplay {
            target: family,
            entering: vec![
                (ObjectId::new(0), left.clone()),
                (ObjectId::new(1), right.clone()),
            ],
            mode: noon::SubsetDisplayMode::OneByOneCeil,
            options,
        }];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &valid,
                    composition,
                    AnimationOptions::new(),
                )
                .unwrap(),
            2.0
        );
        assert!(context.live_contains_mobject(&left).unwrap());
        assert!(context.live_contains_mobject(&right).unwrap());
        assert_eq!(context.bindings.len(), 2);
    }

    #[test]
    fn precreated_detached_family_prepares_after_a_returned_wait() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.square(0.5).unwrap();
        let right = context.scene.circle(0.25).unwrap();
        let family = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();

        context.begin_ordinary_wait(1.0).unwrap();
        let mut player = context.take_execution_player(2.0, 17).unwrap();
        player.live_advance_segment_to(1.0).unwrap();
        player.live_complete_segment().unwrap();
        context.return_execution_player(player).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");

        context.prepare_family_subset_display(&family).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [OrdinaryCompositionChild::FamilySubsetDisplay {
            target: family,
            entering: vec![
                (ObjectId::new(0), left.clone()),
                (ObjectId::new(1), right.clone()),
            ],
            mode: noon::SubsetDisplayMode::OneByOneCeil,
            options,
        }];
        assert_eq!(
            context
                .ordinary_play_mixed_composition(
                    noon_core::SemanticAnimationCompositionKind::Parallel,
                    &children,
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                    AnimationOptions::new().rate_func(RateFunction::Linear),
                )
                .unwrap(),
            2.0
        );
        assert_eq!(context.bindings.len(), 2);
        assert!(context.live_contains_mobject(&left).unwrap());
        assert!(context.live_contains_mobject(&right).unwrap());
    }

    #[test]
    fn ordinary_composition_converts_nested_typed_children_before_atomic_admission() {
        let mut context = CanonicalAuthoringScene::default();
        let bound = context.scene.circle(0.4).unwrap();
        let add_target = context.scene.square(0.5).unwrap();
        let create_target = context.scene.circle(0.3).unwrap();
        let fade_target = context.scene.square(0.6).unwrap();
        let lifecycle_target = context.scene.circle(0.2).unwrap();
        context.bind_mobject(ObjectId::new(0), &bound).unwrap();

        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = vec![
            OrdinaryCompositionChild::Wait { duration: 0.25 },
            OrdinaryCompositionChild::Uncreate {
                entering_id: None,
                target: bound.clone(),
                options: options.remover(true).reverse_rate_function(true),
            },
            OrdinaryCompositionChild::Add {
                entering_id: ObjectId::new(1),
                target: add_target.clone(),
                options: AnimationOptions::new().run_time(0.0),
            },
            OrdinaryCompositionChild::Create {
                entering_id: Some(ObjectId::new(2)),
                target: create_target.clone(),
                options,
            },
            OrdinaryCompositionChild::Fade {
                entering_id: Some(ObjectId::new(3)),
                target: fade_target.clone(),
                direction: SemanticFadeDirection::In,
                endpoint: noon::FadeEndpoint::default(),
                options,
            },
            OrdinaryCompositionChild::AffineLifecycle {
                entering_id: Some(ObjectId::new(4)),
                target: lifecycle_target.clone(),
                direction: noon::AffineLifecycleDirection::IntroduceFrom,
                endpoint: noon::AffineLifecycleEndpoint::Point {
                    x: -1.0,
                    y: 1.0,
                    rotation_offset: 0.0,
                    point_color: None,
                },
                options,
            },
            OrdinaryCompositionChild::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Sequence,
                children: vec![OrdinaryCompositionChild::Wait { duration: 0.1 }],
                options,
            },
        ];
        let composition = AnimationOptions::new().rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);

        let end = context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .unwrap();
        assert!(end > 0.0);
        assert!(!context.live_contains_mobject(&bound).unwrap());
        assert!(context.live_contains_mobject(&add_target).unwrap());
        assert!(context.live_contains_mobject(&create_target).unwrap());
        assert!(context.live_contains_mobject(&fade_target).unwrap());
        assert!(context.live_contains_mobject(&lifecycle_target).unwrap());

        let revision = context.scene.integration_store().borrow().scene_revision();
        let foreign = CanonicalAuthoringScene::default();
        let foreign_target = foreign.scene.circle(0.2).unwrap();
        let invalid = [OrdinaryCompositionChild::Add {
            entering_id: ObjectId::new(9),
            target: foreign_target,
            options: AnimationOptions::new().run_time(0.0),
        }];
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &invalid,
                composition,
                play,
            )
            .is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn canonical_family_composition_rejects_crosswrites_and_captures_between_segments() {
        let mut context = CanonicalAuthoringScene::default();
        let left = context.scene.square(0.5).unwrap();
        let right = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &left).unwrap();
        context.bind_mobject(ObjectId::new(1), &right).unwrap();
        let source = context
            .scene
            .family(&[(&left).into(), (&right).into()])
            .unwrap();
        let mut left_target = left.target_editor().unwrap();
        left_target.set_translation(-2.0, 1.0).unwrap();
        let mut right_target = right.target_editor().unwrap();
        right_target.set_translation(2.0, -1.0).unwrap();
        let target = context
            .scene
            .family(&[(&left_target).into(), (&right_target).into()])
            .unwrap();
        let invalid_target = context.scene.family(&[(&left_target).into()]).unwrap();
        let transform_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
            .lag_ratio(0.25);
        let indicate_options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::ThereAndBack);
        let invalid = [OrdinaryCompositionChild::FamilyTransformTo {
            source: source.clone(),
            target_state: invalid_target,
            options: transform_options,
        }];
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &invalid,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let children = [
            OrdinaryCompositionChild::FamilyTransformTo {
                source: source.clone(),
                target_state: target,
                options: transform_options,
            },
            OrdinaryCompositionChild::Indicate {
                target: left.clone(),
                indication: noon::IndicateOptions::default(),
                options: indicate_options,
            },
            OrdinaryCompositionChild::FamilyIndicate {
                target: source,
                indication: noon::IndicateOptions::default(),
                options: indicate_options,
            },
        ];
        assert!(context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Sequence,
                &children,
                AnimationOptions::new(),
                AnimationOptions::new(),
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        // Completion barriers publish the preceding transform before Indicate
        // captures its effective source and shared family center.
        for (index, child) in children.iter().enumerate() {
            assert_eq!(
                context
                    .ordinary_play_mixed_composition(
                        noon_core::SemanticAnimationCompositionKind::Sequence,
                        std::slice::from_ref(child),
                        AnimationOptions::new(),
                        AnimationOptions::new(),
                    )
                    .unwrap(),
                (index + 1) as f64
            );
        }
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
    fn ordinary_mixed_candidate_activates_scalar_and_object_or_rolls_back_together() {
        let mut context = CanonicalAuthoringScene::default();
        let square = context.scene.square(0.8).unwrap();
        context.bind_mobject(ObjectId::new(0), &square).unwrap();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let composition = AnimationOptions::new().rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);

        let invalid = [
            OrdinaryCompositionChild::Rotate {
                entering_id: None,
                target: square.clone(),
                angle: std::f64::consts::PI,
                pivot: None,
                options,
            },
            OrdinaryCompositionChild::ValueTracker {
                tracker: tracker.clone(),
                target: f64::NAN,
                options,
            },
        ];
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &invalid,
                composition,
                play,
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let valid = [
            OrdinaryCompositionChild::Rotate {
                entering_id: None,
                target: square.clone(),
                angle: std::f64::consts::PI,
                pivot: None,
                options,
            },
            OrdinaryCompositionChild::ValueTracker {
                tracker: tracker.clone(),
                target: 4.0,
                options,
            },
        ];
        let end = context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &valid,
                composition,
                play,
            )
            .unwrap();
        assert_eq!(end, 1.0);
        let player = context.active_live_player().unwrap();
        player.live_advance_segment_to(0.5).unwrap();
        assert_eq!(player.live_effective_signal(&tracker).unwrap(), 2.0);
        assert!(
            (player.live_effective(&square).unwrap().transform.rotation
                - std::f32::consts::FRAC_PI_2)
                .abs()
                < 1e-12
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
                pivot: None,
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
            .apply(&mut context.scene.integration_store().borrow_mut())
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

        let revision = context.scene.integration_store().borrow().scene_revision();
        let endpoint_only_error = context
            .ordinary_play_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .unwrap_err();
        assert!(endpoint_only_error.contains("needs an asynchronous continuation"));
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

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
        let revision = context.scene.integration_store().borrow().scene_revision();

        assert!(context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &children,
                composition,
                play,
            )
            .is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert!(context.player_ownership.is_unstarted());
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
        play_request(
            &mut context,
            bound_transform_child(&source, setup_target.clone(), child),
        )
        .unwrap();
        let player = context.take_execution_player(1.0, 73).unwrap();
        context.return_execution_player(player).unwrap();
        let composition = AnimationOptions::new()
            .lag_ratio(0.0)
            .rate_func(RateFunction::Linear);
        let play = AnimationOptions::new().rate_func(RateFunction::Linear);
        let revision = context.scene.integration_store().borrow().scene_revision();
        let (publication, frame, handoff_duration) = {
            let player = context.player_ownership.local_mut().unwrap();
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
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        let player = context.player_ownership.local_mut().unwrap();
        assert_eq!(player.live_handoff_duration(), handoff_duration);
        assert!(!player.has_pending_live_segment());
        let session = player.session_mut_for_test();
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame(), &frame);
    }

    #[test]
    fn returned_live_primitive_stays_detached_through_an_intervening_removal() {
        let mut context = CanonicalAuthoringScene::default();
        let leaving = context.scene.square(0.5).unwrap();
        let label = context.scene.circle(0.2).unwrap();
        context.bind_mobject(ObjectId::new(0), &leaving).unwrap();
        context.bind_mobject(ObjectId::new(1), &label).unwrap();

        // Python constructs these pulses after a completed continuation barrier.
        // Exercise that returned-player mutation path rather than creating the
        // FadeIn target before execution bootstrap.
        context.live_player(1.0).unwrap();
        let returned = context.take_execution_player(1.0, 73).unwrap();
        context.return_execution_player(returned).unwrap();
        let pulse = context
            .live_create_manim_geometry(noon::ManimGeometryOptions::circle(0.05).unwrap())
            .unwrap();
        context
            .active_live_player()
            .unwrap()
            .live_move_to(
                &pulse,
                noon::LiveLayoutTarget::Point(2.0, -1.0),
                (0.0, 0.0),
                (1.0, 1.0),
            )
            .unwrap();
        {
            let store = context.scene.integration_store().borrow();
            let node = store.node(pulse.node_id()).unwrap();
            assert_eq!(node.residency(), noon_core::SemanticNodeResidency::Detached);
            assert!(node.parents().is_empty());
        }

        let options = AnimationOptions::new()
            .run_time(0.1)
            .rate_func(RateFunction::Linear);
        let removal = [OrdinaryCompositionChild::Fade {
            entering_id: None,
            target: label,
            direction: SemanticFadeDirection::Out,
            endpoint: noon::FadeEndpoint::default(),
            options,
        }];
        let end = context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &removal,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        let mut player = context.resume_execution_player().unwrap();
        player.live_advance_segment_to(end).unwrap();
        player.live_complete_segment().unwrap();
        context.return_execution_player(player).unwrap();

        {
            let store = context.scene.integration_store().borrow();
            let node = store.node(pulse.node_id()).unwrap();
            assert_eq!(node.residency(), noon_core::SemanticNodeResidency::Detached);
            assert!(node.parents().is_empty());
        }
        let mixed = [
            OrdinaryCompositionChild::Fade {
                entering_id: None,
                target: leaving,
                direction: SemanticFadeDirection::Out,
                endpoint: noon::FadeEndpoint::default(),
                options,
            },
            OrdinaryCompositionChild::Fade {
                entering_id: Some(ObjectId::new(2)),
                target: pulse.clone(),
                direction: SemanticFadeDirection::In,
                endpoint: noon::FadeEndpoint::new(
                    0.15,
                    noon::FadeTranslation::Shift(SemanticVec3::ZERO),
                ),
                options,
            },
        ];
        context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &mixed,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        assert!(context.live_contains_mobject(&pulse).unwrap());
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
        assert!(context.player_ownership.is_unstarted());

        let stale = source.target_editor().unwrap();
        let mut removal = SemanticMutationTransaction::new();
        removal.remove_node(stale.node_id());
        removal
            .apply(&mut context.scene.integration_store().borrow_mut())
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
        assert!(context.player_ownership.is_unstarted());
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
        {
            let error = context.mobject_layout(&circle).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        context.return_execution_player(player).unwrap();
        assert_eq!(
            context.mobject_layout(&circle).unwrap(),
            (2.0, -1.0, 2.0, 2.0)
        );
    }

    #[test]
    fn ordinary_line_and_paint_queries_follow_runtime_ownership() {
        let mut context = CanonicalAuthoringScene::default();
        let mut line = context.scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        line.set_fill(0.0, 1.0, 0.0, 0.2).unwrap();
        line.set_stroke_color(0.0, 0.0, 1.0, 1.0).unwrap();
        line.set_stroke_opacity(0.8).unwrap();
        line.set_object_opacity(0.3).unwrap();
        assert!(context.mobject_line_endpoints(&line).is_err());
        assert!(context.mobject_path_query(&line).is_err());
        assert!(context.mobject_fill_opacity(&line).is_err());
        assert!(context.mobject_stroke_opacity(&line).is_err());
        context.bind_mobject(ObjectId::new(0), &line).unwrap();
        let mut target = line.target_editor().unwrap();
        target.set_translation(4.0, -2.0).unwrap();
        target.set_stroke_color(1.0, 0.0, 0.0, 1.0).unwrap();
        target.set_stroke_opacity(0.4).unwrap();
        let animation = context
            .declare_live_transform_to(
                &line,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(
            context.mobject_line_endpoints(&line).unwrap().start,
            (-1.0, 0.0)
        );
        assert_eq!(
            context.mobject_color(&line).unwrap(),
            Color::rgb(0.0, 1.0, 0.0)
        );
        {
            let player = context.live_player(2.0).unwrap();
            player.live_play_animation(&animation).unwrap();
            player.live_advance_segment_to(1.0).unwrap();
        }
        let observed = context.mobject_line_endpoints(&line).unwrap();
        assert_eq!(observed.start, (1.0, -1.0));
        assert_eq!(observed.end, (3.0, -1.0));
        let path = context.mobject_path_query(&line).unwrap();
        assert_eq!(path.start().unwrap(), observed.start);
        assert_eq!(path.end().unwrap(), observed.end);
        let color = context.mobject_color(&line).unwrap();
        assert!((context.mobject_fill_opacity(&line).unwrap() - 0.2).abs() < 1e-6);
        assert!((context.mobject_stroke_opacity(&line).unwrap() - 0.6).abs() < 1e-6);
        assert_eq!(line.stroke_opacity().unwrap(), 0.8);
        assert_eq!(color, Color::rgb(0.0, 1.0, 0.0));
        assert_eq!(line.manim_line_endpoints().unwrap().start, (-1.0, 0.0));

        let player = context.take_execution_player(2.0, 17).unwrap();
        assert!(context.mobject_path_query(&line).is_err());
        {
            let error = context.mobject_line_endpoints(&line).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        {
            let error = context.mobject_color(&line).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        {
            let error = context.mobject_fill_opacity(&line).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        {
            let error = context.mobject_stroke_opacity(&line).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        context.return_execution_player(player).unwrap();
        assert_eq!(context.mobject_line_endpoints(&line).unwrap(), observed);
        assert_eq!(context.mobject_color(&line).unwrap(), color);
        assert!((context.mobject_stroke_opacity(&line).unwrap() - 0.6).abs() < 1e-6);

        // A stale returned player must not shadow later direct authored edits.
        line.shift(1.0, 2.0).unwrap();
        line.set_stroke_color(1.0, 1.0, 0.0, 1.0).unwrap();
        line.set_stroke_opacity(0.7).unwrap();
        assert_eq!(
            context.mobject_line_endpoints(&line).unwrap().start,
            (0.0, 2.0)
        );
        assert_eq!(
            context.mobject_color(&line).unwrap(),
            Color::rgb(0.0, 1.0, 0.0)
        );
        assert_eq!(
            context.mobject_stroke_color(&line).unwrap(),
            Some(Color::rgb(1.0, 1.0, 0.0))
        );
        assert_eq!(context.mobject_stroke_opacity(&line).unwrap(), 0.7);
    }

    #[test]
    fn underline_construction_observes_live_and_fresh_detached_targets() {
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
        let player = context.live_player(2.0).unwrap();
        player.live_play_animation(&animation).unwrap();
        player.live_advance_segment_to(2.0).unwrap();
        player.live_complete_segment().unwrap();
        let options = context.begin_underline(&circle, 0.15).unwrap();
        let underline = context.live_create_manim_geometry(options).unwrap();
        assert_eq!(underline.center().unwrap(), (4.0, -3.15));
        assert_eq!(underline.width().unwrap(), 2.0);

        let mut options = noon::ManimGeometryOptions::circle(0.5).unwrap();
        options.set_translation(8.0, 7.0).unwrap();
        let detached = context.live_create_manim_geometry(options).unwrap();
        let options = context.begin_underline(&detached, 0.15).unwrap();
        let detached_underline = context.live_create_manim_geometry(options).unwrap();
        assert_eq!(detached_underline.center().unwrap(), (8.0, 6.35));
        let foreign = noon::Scene::new().circle(1.0).unwrap();
        let error = context.begin_underline(&foreign, 0.15).unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("foreign_handle", "authoring.foreign_store")
        );
        let _player = context.take_execution_player(2.0, 17).unwrap();
        {
            let error = context.begin_underline(&circle, 0.15).unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
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
        assert!(context.player_ownership.local().is_some());

        // The next registration boundary lowers precisely one fresh runtime.
        context.prepare_execution_run().unwrap();
        assert!(context.player_ownership.is_unstarted());

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
    fn empty_wait_then_live_text_fade_reuses_one_returned_player() {
        let mut context = CanonicalAuthoringScene::default();
        assert_eq!(context.begin_ordinary_wait(0.5).unwrap(), 0.5);
        let mut player = context.take_execution_player(1.0, 17).unwrap();
        assert!(player.live_advance_segment_to(0.5).unwrap());
        player.live_complete_segment().unwrap();
        context.return_execution_player(player).unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");

        let label = context
            .live_create_text(
                noon::Text::new("Late")
                    .with_font_size(36.0)
                    .color(Color::rgba(0.2, 0.4, 0.8, 1.0)),
            )
            .unwrap();
        assert_eq!(context.live_execution_ownership(), "returned");
        assert!(!context
            .active_live_player()
            .unwrap()
            .live_contains(&label)
            .unwrap());
        let typst = context
            .live_create_typst(noon::Typst::new("#circle(radius: 1em)"))
            .unwrap();
        let math = context
            .live_create_math_typst(noon::MathTypst::new("x^2"))
            .unwrap();
        assert_ne!(typst.node_id(), math.node_id());
        assert!(!context
            .active_live_player()
            .unwrap()
            .live_contains(&typst)
            .unwrap());
        assert!(!context
            .active_live_player()
            .unwrap()
            .live_contains(&math)
            .unwrap());

        let end = begin_request(
            &mut context,
            fade_request(
                Some(ObjectId::new(0)),
                &label,
                SemanticFadeDirection::In,
                noon::FadeEndpoint::default(),
                AnimationOptions::new()
                    .run_time(0.75)
                    .rate_func(RateFunction::Linear),
            ),
        )
        .unwrap();
        assert_eq!(end, 1.25);
        assert!(context.live_contains_mobject(&label).unwrap());
        assert_eq!(
            context.bindings.get(&ObjectId::new(0)),
            Some(&label.node_id())
        );

        let leased = context.take_execution_player(end, 18).unwrap();
        {
            let error = context
                .live_create_text(noon::Text::new("Rejected"))
                .unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        {
            let error = context
                .live_create_typst(noon::Typst::new("Rejected"))
                .unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        {
            let error = context
                .live_create_math_typst(noon::MathTypst::new("Rejected"))
                .unwrap_err();
            assert_eq!(
                (error.category, error.code),
                ("unclassified", "unclassified")
            );
            assert_eq!(
                error.message,
                "live execution session is running in the semantic engine"
            );
        }
        context.return_execution_player(leased).unwrap();
    }

    #[test]
    fn callback_continuation_can_resume_but_terminal_failure_cannot_reenter_source() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(circle.node_id(), HostCallbackId::new(7), 0.0, None);
        callbacks
            .apply(&mut context.scene.integration_store().borrow_mut())
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

        let end_time = begin_request(
            &mut context,
            bound_transform_child(
                &circle,
                target.clone(),
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            ),
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
        let next_endpoint = begin_request(
            &mut context,
            bound_transform_child(
                &circle,
                next_target.clone(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
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
        assert_eq!(query_error.category, "stale_publication");
        assert_eq!(
            query_error.cause.as_ref().unwrap().code,
            "publication.stale_scene_revision"
        );
        let run_error = context.prepare_execution_run().unwrap_err();
        assert!(run_error.contains("authored scene changed while live execution is active"));
        assert!(context.player_ownership.local().is_some());
        assert!(!context.player_ownership.is_returned());
    }

    #[test]
    fn callback_occurrences_publish_through_the_same_live_session() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();

        context
            .add_updater(&circle, HostCallbackId::new(12), 0.0, None)
            .unwrap();
        let registrations = context
            .scene
            .integration_store()
            .borrow()
            .semantic_updater_registrations(circle.node_id())
            .unwrap()
            .to_vec();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].callback(), HostCallbackId::new(12));
        assert_eq!(registrations[0].active_from(), 0.0);

        context.live_player(2.0).unwrap();
        context
            .add_updater(&circle, HostCallbackId::new(13), 1.0, None)
            .unwrap();
        context
            .remove_updater(&circle, HostCallbackId::new(12), 0.0)
            .unwrap();
        context.clear_updaters(&circle, 1.0).unwrap();
        let registrations = context
            .scene
            .integration_store()
            .borrow()
            .semantic_updater_registrations(circle.node_id())
            .unwrap()
            .to_vec();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].inactive_from(), Some(0.0));
        assert_eq!(registrations[1].inactive_from(), Some(1.0));
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
                .scene
                .play_value(&tracker, 4.0)
                .rate_func(RateFunction::Linear)
                .run_time(2.0)
                .map(|()| context.scene.time())
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
            .integration_store()
            .borrow()
            .is_semantic_signal_scoped(context.scene.root(), tracker.node_id()));

        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.create_value_tracker(f64::MAX).is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
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
        let tracker = noon::ValueTracker::detached(
            std::rc::Rc::clone(context.scene.integration_store()),
            1.25,
        )
        .unwrap();
        context.associate_value_tracker(&tracker).unwrap();
        assert_eq!(context.tracker_value(&tracker).unwrap(), 1.25);

        context.live_player(1.0).unwrap();
        let live_tracker = noon::ValueTracker::detached(
            std::rc::Rc::clone(context.scene.integration_store()),
            2.5,
        )
        .unwrap();
        context.associate_value_tracker(&live_tracker).unwrap();
        assert_eq!(context.tracker_value(&live_tracker).unwrap(), 2.5);

        let invalid = noon::ValueTracker::detached(
            std::rc::Rc::clone(context.scene.integration_store()),
            f64::MAX,
        )
        .unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context.associate_value_tracker(&invalid).is_err());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
        assert_eq!(invalid.detached_value().unwrap(), f64::MAX);

        let foreign = CanonicalAuthoringScene::default();
        let foreign_tracker = noon::ValueTracker::detached(
            std::rc::Rc::clone(foreign.scene.integration_store()),
            3.0,
        )
        .unwrap();
        assert!(context.associate_value_tracker(&foreign_tracker).is_err());
        assert_eq!(foreign_tracker.detached_value().unwrap(), 3.0);
    }

    #[test]
    fn scalar_tracker_wait_keeps_the_canonical_authoring_cursor() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker = context.create_value_tracker(0.0).unwrap();
        context
            .scene
            .play_value(&tracker, 4.0)
            .rate_func(RateFunction::Linear)
            .run_time(2.0)
            .unwrap();
        assert_eq!(context.authored_wait(1.0).unwrap(), 3.0);
        assert_eq!(
            context
                .scene
                .play_value(&tracker, 6.0)
                .rate_func(RateFunction::Linear)
                .run_time(1.0)
                .map(|()| context.scene.time())
                .unwrap(),
            4.0
        );
        let timeline = context
            .scene
            .integration_store()
            .borrow()
            .semantic_signal_state(tracker.node_id())
            .unwrap()
            .scalar_timeline()
            .to_vec();
        let noon_core::SemanticScalarSignalTimelineEntry::Track(second) = &timeline[1] else {
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
        let revision = context.scene.integration_store().borrow().scene_revision();

        assert!(begin_request(
            &mut context,
            OrdinaryCompositionChild::ValueTracker {
                tracker: tracker.clone(),
                target: f64::MAX,
                options: AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
            }
        )
        .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let end = begin_request(
            &mut context,
            OrdinaryCompositionChild::ValueTracker {
                tracker: tracker.clone(),
                target: 2.0,
                options: AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            },
        )
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
                .integration_store()
                .borrow()
                .semantic_input_scalar_value_at(tracker.node_id(), 1.0)
                .unwrap(),
            1.0
        );
    }

    #[test]
    fn ordinary_composition_play_rejects_a_pre_execution_scalar_cursor_without_bootstrapping() {
        let mut context = CanonicalAuthoringScene::default();
        let tracker = context.create_value_tracker(0.0).unwrap();
        let circle = context.scene.circle(0.4).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(2.0, -1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &circle).unwrap();
        context
            .scene
            .play_value(&tracker, 4.0)
            .rate_func(RateFunction::Linear)
            .run_time(2.0)
            .unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();

        let error = play_request(
            &mut context,
            bound_transform_child(
                &circle,
                target.clone(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
        )
        .unwrap_err();
        assert!(error.contains("cannot follow pre-execution canonical timing"));
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(context.authored_duration(), 2.0);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn ordinary_affine_lifecycle_preserves_authored_state_and_removes_detached_leaf() {
        let mut context = CanonicalAuthoringScene::default();
        let square = context.scene.square(1.0).unwrap();
        let before = square.state().unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let end = context
            .ordinary_play_affine_lifecycle(
                ObjectId::new(0),
                &square,
                noon::AffineLifecycleDirection::RemoveTo,
                noon::AffineLifecycleEndpoint::EffectiveCenter,
                options,
            )
            .unwrap();
        assert_eq!(end, 1.0);
        assert!(!context.live_contains_mobject(&square).unwrap());
        assert_eq!(square.state().unwrap(), before);
        assert_eq!(
            context.bindings.get(&ObjectId::new(0)),
            Some(&square.node_id())
        );
    }

    #[test]
    fn ordinary_fade_reuses_one_live_session_and_preserves_readd_identity() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);

        let fade_in_end = begin_request(
            &mut context,
            fade_request(
                Some(ObjectId::new(0)),
                &circle,
                SemanticFadeDirection::In,
                noon::FadeEndpoint::default(),
                options,
            ),
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

        let fade_out_end = begin_request(
            &mut context,
            fade_request(
                None,
                &circle,
                SemanticFadeDirection::Out,
                noon::FadeEndpoint::default(),
                options,
            ),
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
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(begin_request(
            &mut context,
            create_request(
                Some(ObjectId::new(0)),
                &circle,
                AnimationOptions::new().run_time(f64::NAN)
            )
        )
        .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let foreign = CanonicalAuthoringScene::default()
            .scene
            .circle(0.4)
            .unwrap();
        assert!(begin_request(
            &mut context,
            create_request(
                Some(ObjectId::new(0)),
                &foreign,
                AnimationOptions::new().run_time(1.0)
            )
        )
        .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );

        let end = begin_request(
            &mut context,
            create_request(
                Some(ObjectId::new(0)),
                &circle,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
        )
        .unwrap();
        assert_eq!(end, 1.0);
        assert!(context.live_contains_mobject(&circle).unwrap());
        assert!(begin_request(
            &mut context,
            create_request(
                Some(ObjectId::new(1)),
                &circle,
                AnimationOptions::new().run_time(1.0)
            )
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
        let end = begin_request(
            &mut context,
            uncreate_request(Some(id), &square, AnimationOptions::new()),
        )
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
    fn ordinary_uncreate_releases_a_prebound_leaf_without_rebinding_it() {
        let mut context = CanonicalAuthoringScene::default();
        let square = context.scene.square(2.0).unwrap();
        let id = ObjectId::new(0);
        context.bind_mobject(id, &square).unwrap();

        let end = begin_request(
            &mut context,
            uncreate_request(
                None,
                &square,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
        )
        .unwrap();
        assert_eq!(context.bindings.get(&id), Some(&square.node_id()));
        assert!(context.live_contains_mobject(&square).unwrap());
        let player = context.active_live_player().unwrap();
        player.live_advance_segment_to(end).unwrap();
        player.live_complete_segment().unwrap();
        assert!(!context.live_contains_mobject(&square).unwrap());
        assert_eq!(context.bindings.get(&id), Some(&square.node_id()));
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
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &create_requests(&[
                    (ObjectId::new(0), circle.clone(), options),
                    (ObjectId::new(1), square.clone(), options),
                ]),
                AnimationOptions::new(),
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
        let revision = context.scene.integration_store().borrow().scene_revision();

        assert!(context
            .begin_ordinary_mixed_composition(
                noon_core::SemanticAnimationCompositionKind::Parallel,
                &create_requests(&[
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
                ]),
                AnimationOptions::new(),
                AnimationOptions::new().run_time(1.0)
            )
            .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(context.bindings.is_empty());
        assert!(context.identities.is_empty());
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

    #[test]
    fn failed_first_fade_does_not_install_a_player_or_derived_binding() {
        let mut context = CanonicalAuthoringScene::default();
        let circle = context.scene.circle(0.4).unwrap();
        let before = context.scene.integration_store().borrow().scene_revision();
        let id = ObjectId::new(0);

        assert!(begin_request(
            &mut context,
            fade_request(
                Some(id),
                &circle,
                SemanticFadeDirection::In,
                noon::FadeEndpoint::default(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.5)
            )
        )
        .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(!context.player_ownership.is_returned());
        assert!(!context.player_ownership.is_transferred());
        assert!(!context.bindings.contains_key(&id));
        assert!(!context.identities.contains_key(&circle.node_id()));
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            before
        );

        // The failed provisional player did not poison the ordinary path.
        begin_request(
            &mut context,
            fade_request(
                Some(id),
                &circle,
                SemanticFadeDirection::In,
                noon::FadeEndpoint::default(),
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
        )
        .unwrap();
        assert!(context.player_ownership.local().is_some());
        assert_eq!(context.bindings.get(&id), Some(&circle.node_id()));
    }

    #[test]
    fn invalid_first_fade_entry_keeps_context_unbootstrapped() {
        let mut context = CanonicalAuthoringScene::default();
        let text = context
            .scene
            .text(noon::Text::new("resource entry"))
            .unwrap();
        let before = context.scene.integration_store().borrow().scene_revision();
        let id = ObjectId::new(0);

        assert!(begin_request(
            &mut context,
            fade_request(
                Some(id),
                &text,
                SemanticFadeDirection::In,
                noon::FadeEndpoint::default(),
                AnimationOptions::new()
                    .run_time(-1.0)
                    .rate_func(RateFunction::Linear)
            )
        )
        .is_err());
        assert!(context.player_ownership.is_unstarted());
        assert!(!context.bindings.contains_key(&id));
        assert!(!context.identities.contains_key(&text.node_id()));
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            before
        );
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

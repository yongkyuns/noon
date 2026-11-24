//! A consumed static Scene and its one execution component.
//!
//! This owner adds no schedule or clock. It keeps the canonical semantic arena
//! available to mutation-bearing input and retirement on native and WASM hosts.

use super::Scene;
use crate::execution_session::ExecutionSession;
use crate::{
    integration::{NativePointerInputPublication, NativePointerInputToken},
    ExecutionSessionCallbackError, ExecutionSessionInputError, ExecutionSessionPublicationError,
    LiveSession, TranslationDragError, TranslationDragReceipt,
};
use noon_core::{NativePointerInput, SemanticNodeId, SemanticStore};
use std::{cell::RefCell, rc::Rc};

/// One consumed Scene paired with the execution lowered from its exact root.
#[derive(Debug)]
pub struct SceneExecution {
    scene: Scene,
}

/// Failure to evaluate a source with an installed runtime.
#[derive(Debug)]
pub enum SceneEvaluationError {
    /// The authoring Scene has no installed execution.
    NoExecution,
    /// Forward evaluation failed without seeking.
    Advance(noon_runtime::EvaluationError),
    /// Backward evaluation failed to seek the owned scene.
    Seek(ExecutionSessionInputError),
}

impl std::fmt::Display for SceneEvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExecution => formatter.write_str("no execution session"),
            Self::Advance(error) => error.fmt(formatter),
            Self::Seek(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SceneEvaluationError {}

/// Failure to transfer a Scene and an existing execution into one source owner.
#[derive(Debug)]
pub enum SceneExecutionError {
    AlreadyExecuting,
    Lowering(noon_compile::SemanticExecutionLoweringError),
    Publication(ExecutionSessionPublicationError),
}

impl std::fmt::Display for SceneExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExecuting => f.write_str("scene already owns an execution component"),
            Self::Lowering(error) => error.fmt(f),
            Self::Publication(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SceneExecutionError {}

impl Scene {
    fn seek_execution(
        &mut self,
        time: f64,
    ) -> Result<&noon_runtime::FrameState, ExecutionSessionInputError> {
        self.execution
            .as_mut()
            .expect("scene execution is installed")
            .seek_with_store(&mut self.store.borrow_mut(), time)
    }

    /// Consume this scene into the static source used by platform viewers.
    pub fn into_execution(mut self) -> Result<SceneExecution, SceneExecutionError> {
        if self.execution.is_none() {
            let session = self
                .execution_session()
                .map_err(SceneExecutionError::Lowering)?;
            self.install_execution(session);
        }
        self.owned_execution()
            .require_scene_owner(&self.store.borrow(), self.root)
            .map_err(SceneExecutionError::Publication)?;
        Ok(SceneExecution { scene: self })
    }

    /// Consume this scene with one explicitly selected authored animation graph.
    /// Exact tracks keep their authored timing, and the chosen graph begins at zero.
    pub fn into_execution_with_animation_root(
        self,
        animation_root: &crate::DeclaredAnimation,
    ) -> Result<SceneExecution, String> {
        self.into_execution_with_animation_root_at(animation_root, 0.0)
    }

    /// Start the chosen authored animation graph at an explicit origin.
    pub fn into_execution_with_animation_root_at(
        self,
        animation_root: &crate::DeclaredAnimation,
        origin: f64,
    ) -> Result<SceneExecution, String> {
        let session = self.execution_session_with_animation_root_at(animation_root, origin)?;
        host_execution::consume_with_session(self, session).map_err(|error| error.to_string())
    }
}

/// Execution services for hosts that retain an authoring Scene across bootstrap.
pub mod host_execution {
    pub use super::SceneEvaluationError;
    use super::*;
    pub use crate::execution_session::ExecutionSession;

    /// Admission failures for handing a completed source to a native viewer.
    #[derive(Debug)]
    pub enum NativeViewerError {
        NoExecution,
        InteractiveCallbacks,
        HistoryRequired,
        Replay(noon_runtime::ReplayError),
        Publication(ExecutionSessionPublicationError),
        Seek(ExecutionSessionInputError),
    }

    impl std::fmt::Display for NativeViewerError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::NoExecution => formatter.write_str("no execution session"),
                Self::InteractiveCallbacks => formatter.write_str(
                    "native translation drag requires completed source without required callbacks",
                ),
                Self::HistoryRequired => formatter
                    .write_str("native preview requires finite playback or retained history"),
                Self::Replay(error) => error.fmt(formatter),
                Self::Publication(error) => error.fmt(formatter),
                Self::Seek(error) => error.fmt(formatter),
            }
        }
    }

    impl std::error::Error for NativeViewerError {}

    /// Failure to interrupt a pending callback or retire input while closing a source.
    #[derive(Debug)]
    pub enum SceneRetirementError {
        Callback(ExecutionSessionCallbackError),
        Input(ExecutionSessionInputError),
    }

    impl std::fmt::Display for SceneRetirementError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Callback(error) => error.fmt(formatter),
                Self::Input(error) => error.fmt(formatter),
            }
        }
    }

    impl std::error::Error for SceneRetirementError {}

    /// Adopt an existing semantic family as an integration scene's canonical root.
    ///
    /// The same arena and family are retained. This performs no copy, membership
    /// mutation, or execution bootstrap; detached declarations remain detached.
    pub fn adopt_root(
        store: Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
    ) -> Result<Scene, noon_core::SemanticSceneOperationError> {
        store.borrow().semantic_family_checked(root)?;
        Ok(Scene {
            store,
            root,
            cursor: 0.0,
            sections: Vec::new(),
            execution: None,
            source_generation: None,
        })
    }

    /// Lower a standalone runtime for a host-managed bootstrap or handoff.
    /// Use [`Scene::into_execution`] for the ordinary owned execution path.
    pub fn lower(
        scene: &Scene,
    ) -> Result<ExecutionSession, noon_compile::SemanticExecutionLoweringError> {
        scene.execution_session()
    }

    /// Borrow execution controls before the host installs a prepared runtime.
    pub fn borrow_uninstalled<'a>(
        scene: &'a Scene,
        session: &'a mut ExecutionSession,
    ) -> LiveSession<'a> {
        scene.live(session)
    }

    /// Transfer a host-prepared runtime and its exact Scene into one owner.
    ///
    /// The handoff validates arena identity, publication revision, and scene root;
    /// it never relowers or discards the supplied runtime's current state.
    pub fn consume_with_session(
        mut scene: Scene,
        session: ExecutionSession,
    ) -> Result<SceneExecution, SceneExecutionError> {
        attach(&mut scene, session)?;
        Ok(SceneExecution { scene })
    }

    /// Install a prepared session only if it belongs to this exact Scene.
    pub fn attach(scene: &mut Scene, session: ExecutionSession) -> Result<(), SceneExecutionError> {
        if scene.execution.is_some() {
            return Err(SceneExecutionError::AlreadyExecuting);
        }
        session
            .require_scene_owner(&scene.store.borrow(), scene.root)
            .map_err(SceneExecutionError::Publication)?;
        scene.install_execution(session);
        Ok(())
    }

    pub fn session(scene: &Scene) -> Option<&ExecutionSession> {
        scene.running_execution()
    }

    /// Borrow execution controls from the Scene's installed runtime.
    pub fn live(scene: &mut Scene) -> Option<LiveSession<'_>> {
        scene.running_execution()?;
        Some(scene.owned_live())
    }

    /// Register a host callable at its authored time and occurrence position.
    pub fn add_updater(
        scene: &mut Scene,
        target: &crate::Mobject,
        callback: noon_core::HostCallbackId,
        active_from: f64,
        position: Option<usize>,
    ) -> Result<(), crate::AuthoringError> {
        edit_updaters(scene, target, |transaction| {
            transaction.add_updater(target.node_id(), callback, active_from, position);
        })
    }

    /// Close the first open occurrence of a host callable at an exclusive time.
    pub fn remove_updater(
        scene: &mut Scene,
        target: &crate::Mobject,
        callback: noon_core::HostCallbackId,
        inactive_from: f64,
    ) -> Result<(), crate::AuthoringError> {
        edit_updaters(scene, target, |transaction| {
            transaction.remove_updater(target.node_id(), callback, inactive_from);
        })
    }

    /// Close all open occurrences on this object at an exclusive time.
    pub fn clear_updaters(
        scene: &mut Scene,
        target: &crate::Mobject,
        inactive_from: f64,
    ) -> Result<(), crate::AuthoringError> {
        edit_updaters(scene, target, |transaction| {
            transaction.clear_updaters(target.node_id(), inactive_from);
        })
    }

    fn edit_updaters(
        scene: &mut Scene,
        target: &crate::Mobject,
        declare: impl FnOnce(&mut noon_core::SemanticMutationTransaction),
    ) -> Result<(), crate::AuthoringError> {
        if !Rc::ptr_eq(&scene.store, target.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore);
        }
        let mut transaction = noon_core::SemanticMutationTransaction::new();
        declare(&mut transaction);
        scene.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Publish a typed host semantic transaction through its owning Scene.
    pub fn publish(
        scene: &mut Scene,
        transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, crate::AuthoringError> {
        scene.apply_semantic_transaction(transaction)
    }

    /// Publish for a host that explicitly retains a raw store, root, and runtime.
    pub fn publish_at_root(
        store: &Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
        execution: &mut ExecutionSession,
        transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, ExecutionSessionPublicationError>
    {
        Scene::publish_running_transaction(store, root, execution, transaction)
    }

    /// Borrow the installed runtime for host callback and playback services.
    pub fn session_mut(scene: &mut Scene) -> Option<&mut ExecutionSession> {
        scene.running_execution_mut()
    }

    /// Evaluate a source at an absolute authored time. Moving backward uses
    /// the canonical Scene for input cleanup after deterministic admission.
    pub fn evaluate(scene: &mut Scene, time: f64) -> Result<(), SceneEvaluationError> {
        let execution = scene
            .execution
            .as_mut()
            .ok_or(SceneEvaluationError::NoExecution)?;
        if time < execution.effective_time() {
            scene
                .seek_execution(time)
                .map(|_| ())
                .map_err(SceneEvaluationError::Seek)
        } else {
            execution
                .advance_to(time)
                .map(|_| ())
                .map_err(SceneEvaluationError::Advance)
        }
    }

    /// Transfer the installed execution to the native viewer at its initial
    /// replay frame or its interactive endpoint. Admission failures retain the
    /// source with its caller.
    pub fn take_for_native_viewer(scene: &mut Scene) -> Result<SceneExecution, NativeViewerError> {
        let session = scene
            .execution
            .as_mut()
            .ok_or(NativeViewerError::NoExecution)?;
        session
            .require_scene_owner(&scene.store.borrow(), scene.root)
            .map_err(NativeViewerError::Publication)?;
        if session.has_translation_drag_targets() {
            if session.has_required_callbacks() || session.pending_callback_token().is_some() {
                return Err(NativeViewerError::InteractiveCallbacks);
            }
            session.discard_replay_retention();
        } else {
            if session.replay_scope_active() {
                session
                    .seal_replay_for_viewer()
                    .map_err(NativeViewerError::Replay)?;
            } else if !session.supports_immutable_playback() {
                return Err(NativeViewerError::HistoryRequired);
            }
            scene.seek_execution(0.0).map_err(NativeViewerError::Seek)?;
        }
        Ok(SceneExecution {
            scene: std::mem::take(scene),
        })
    }

    /// Interrupt pending callback work and retire input before discarding
    /// execution. Existing authored handles retain their semantic identities.
    pub fn retire(scene: &mut Scene) -> Result<(), SceneRetirementError> {
        if let Some(session) = scene.execution.as_mut() {
            if let Some(token) = session.pending_callback_token() {
                session
                    .interrupt_required_callback_phase(token)
                    .map_err(SceneRetirementError::Callback)?;
            }
            session
                .retire_interactions_for_source_replacement(&mut scene.store.borrow_mut())
                .map_err(SceneRetirementError::Input)?;
        }
        scene.execution = None;
        Ok(())
    }
}

impl SceneExecution {
    /// Borrow the canonical Scene for authored observations and identity checks.
    pub const fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Publish ordinary authoring operations through the Scene that owns execution.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    pub const fn session(&self) -> &ExecutionSession {
        self.scene.owned_execution()
    }

    /// Borrow typed execution services that do not require semantic ownership.
    /// Mutation-bearing interaction and seek use this owner's dedicated methods.
    pub fn session_mut(&mut self) -> &mut ExecutionSession {
        self.scene.owned_execution_mut()
    }

    pub fn live(&mut self) -> LiveSession<'_> {
        self.scene.owned_live()
    }

    pub(crate) fn with_store_and_session<R>(
        &mut self,
        apply: impl FnOnce(&mut SemanticStore, &mut ExecutionSession) -> R,
    ) -> R {
        let scene = &mut self.scene;
        let session = scene
            .execution
            .as_mut()
            .expect("scene execution is installed");
        apply(&mut scene.store.borrow_mut(), session)
    }

    pub fn submit_native_pointer_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<NativePointerInputPublication, ExecutionSessionInputError> {
        self.with_store_and_session(|store, session| {
            session.submit_native_pointer_input(store, token, input)
        })
    }

    /// Publish normalized pointer input through the configured translation-drag policy.
    /// A committed drag can return a receipt for undo on the same Scene.
    pub fn submit_translation_drag_input(
        &mut self,
        token: &NativePointerInputToken,
        input: NativePointerInput,
    ) -> Result<TranslationDragReceipt, TranslationDragError> {
        self.with_store_and_session(|store, session| {
            session.submit_translation_drag_input(store, token, input)
        })
    }

    pub fn advance_interactions(
        &mut self,
        wall_time_seconds: f64,
    ) -> Result<(), ExecutionSessionInputError> {
        self.with_store_and_session(|store, session| {
            session.advance_interactions(store, wall_time_seconds)
        })
    }

    /// Retire input-owned state before releasing this source's canonical owner.
    /// Renderer recovery that retains the source must not call this method.
    pub fn retire_execution_source(&mut self) -> Result<(), ExecutionSessionInputError> {
        self.with_store_and_session(|store, session| {
            session.retire_interactions_for_source_replacement(store)
        })
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<&noon_runtime::FrameState, ExecutionSessionInputError> {
        self.scene.seek_execution(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        NativeInputModifiers, NativePointerId, NativePointerInputKind, NativePointerPosition, Vec2,
    };

    #[test]
    fn native_viewer_rejection_preserves_source_and_success_moves_exact_runtime() {
        let mut scene = Scene::new();
        let mut object = scene.circle(0.5).unwrap();
        scene.add(&object).unwrap();
        let store = Rc::clone(scene.integration_store());

        assert!(matches!(
            host_execution::take_for_native_viewer(&mut scene),
            Err(host_execution::NativeViewerError::NoExecution)
        ));
        let mut session = scene.execution_session().unwrap();
        session.begin_replay_retention(Default::default()).unwrap();
        let runtime = session.runtime_identity();
        scene.install_execution(session);
        object.set_fill(0.0, 0.0, 1.0, 1.0).unwrap();
        assert!(matches!(
            host_execution::take_for_native_viewer(&mut scene),
            Err(host_execution::NativeViewerError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            ))
        ));
        assert!(Rc::ptr_eq(scene.integration_store(), &store));
        assert_eq!(scene.owned_execution().runtime_identity(), runtime);
        assert!(!scene.owned_execution().replay_is_sealed());

        host_execution::retire(&mut scene).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.begin_replay_retention(Default::default()).unwrap();
        session.advance_to(0.5).unwrap();
        let runtime = session.runtime_identity();
        scene.install_execution(session);
        let viewer = host_execution::take_for_native_viewer(&mut scene).unwrap();
        assert!(scene.running_execution().is_none());
        assert!(Rc::ptr_eq(viewer.scene().integration_store(), &store));
        assert_eq!(viewer.session().runtime_identity(), runtime);
        assert_eq!(viewer.session().replay_end_time(), Some(0.5));
        assert_eq!(viewer.session().effective_time(), 0.0);
    }

    #[test]
    fn source_retirement_interrupts_pending_callback_without_publishing_its_sample() {
        let mut scene = Scene::new();
        let object = scene.circle(0.5).unwrap();
        scene.add(&object).unwrap();
        let mut session = scene.execution_session().unwrap();
        session
            .begin_required_callback_phase(0.25, [object.node_id()])
            .unwrap();
        scene.install_execution(session);
        let revision = scene.revision();
        host_execution::retire(&mut scene).unwrap();
        assert!(scene.running_execution().is_none());
        assert_eq!(scene.revision(), revision);
        scene.authored(&object).unwrap();
        host_execution::retire(&mut scene).unwrap();
    }

    #[test]
    fn retiring_consumed_scene_cleans_temporary_glow_retained_by_external_handles() {
        let mut scene = Scene::new();
        let mut circle = scene.circle(0.5).unwrap();
        circle.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        circle.disable_stroke().unwrap();
        scene.add(&circle).unwrap();
        let pulse = scene
            .declare_glow_pulse(&circle, 2.0, crate::AnimationOptions::new().run_time(1.0))
            .unwrap();
        scene.on_click(&circle, Some(&pulse)).unwrap();
        assert!(circle.get_effect("glow").is_err());
        let mut execution = scene.into_execution().unwrap();
        execution
            .session_mut()
            .configure_native_pointer_input(
                NativePointerId {
                    source: 60,
                    pointer: 1,
                },
                1,
            )
            .unwrap();
        let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0, 100.0)).unwrap();
        for (sequence, kind) in [
            (
                1,
                NativePointerInputKind::Press {
                    position,
                    button: 0,
                },
            ),
            (
                2,
                NativePointerInputKind::Release {
                    position,
                    button: 0,
                },
            ),
        ] {
            let token = execution.session().native_pointer_input_token().unwrap();
            let input = NativePointerInput::new(
                sequence,
                token.pointer(),
                token.context(),
                NativeInputModifiers::default(),
                kind,
            );
            execution
                .submit_native_pointer_input(&token, input)
                .unwrap();
        }
        execution.advance_interactions(10.0).unwrap();
        execution.advance_interactions(10.5).unwrap();
        let attachment = circle.get_effect("glow").unwrap();
        let object = execution
            .session()
            .execution_object_id(circle.node_id())
            .unwrap();
        assert!(execution.session().interactions_active());
        assert_eq!(
            execution
                .session()
                .frame()
                .objects
                .iter()
                .find(|row| row.id == object)
                .unwrap()
                .effects
                .as_ref()
                .unwrap()
                .get(attachment.node_id())
                .unwrap()
                .definition
                .as_glow()
                .unwrap()
                .intensity(),
            2.0,
        );
        let time = execution.session().frame().time;

        execution.retire_execution_source().unwrap();

        assert!(!execution.session().interactions_active());
        assert_eq!(execution.session().frame().time, time);
        assert!(execution
            .session()
            .frame()
            .objects
            .iter()
            .find(|row| row.id == object)
            .unwrap()
            .effects
            .is_none());
        execution.with_store_and_session(|store, session| {
            assert!(store.node(attachment.node_id()).is_none());
            assert_eq!(
                store.scene_revision(),
                session.publication_context().scene_revision()
            );
        });
        assert!(circle.get_effect("glow").is_err());
        assert!(attachment.authored_definition().is_err());

        // These handles keep the canonical arena alive after the source drops.
        // The completed retirement must remain visible through that same arena.
        drop(execution);
        circle.validate().unwrap();
        assert!(circle.get_effect("glow").is_err());
        assert!(attachment.authored_definition().is_err());
    }

    #[test]
    fn consumed_scene_retains_the_same_arena_and_activated_runtime() {
        let mut scene = Scene::new();
        let circle = scene.circle(0.5).unwrap();
        scene.add(&circle).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.shift(2.0, 0.0).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                crate::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(crate::RateFunction::Linear),
            )
            .unwrap();
        let arena = Rc::clone(scene.integration_store());
        let mut session = scene.execution_session().unwrap();
        scene.live(&mut session).play_animation(&animation).unwrap();
        session.advance_to(0.5).unwrap();
        let before = session.frame().clone();
        let publication = session.publication_context();
        let mut execution = host_execution::consume_with_session(scene, session).unwrap();
        assert!(Rc::ptr_eq(&arena, execution.scene.integration_store()));
        assert_eq!(execution.session().frame(), &before);
        assert_eq!(execution.session().publication_context(), publication);
        execution.with_store_and_session(|store, session| {
            assert_eq!(
                store.scene_revision(),
                session.publication_context().scene_revision()
            );
        });
    }

    #[test]
    fn consumed_scene_rejects_a_foreign_arena_or_other_root() {
        let left = Scene::new();
        let right = Scene::new();
        let session = left.execution_session().unwrap();
        assert!(matches!(
            host_execution::consume_with_session(right, session),
            Err(SceneExecutionError::Publication(
                ExecutionSessionPublicationError::ForeignSemanticStore
            )),
        ));

        let left = Scene::new();
        let right = Scene::with_integration_store(Rc::clone(left.integration_store()));
        let session = left.execution_session().unwrap();
        let root = right.root();
        assert!(matches!(
            host_execution::consume_with_session(right, session),
            Err(SceneExecutionError::Publication(ExecutionSessionPublicationError::UnknownObject(found))) if found == root,
        ));
    }

    #[test]
    fn consumed_scene_rejects_a_stale_publication() {
        let mut scene = Scene::new();
        let session = scene.execution_session().unwrap();
        scene.circle(0.5).unwrap();
        assert!(matches!(
            host_execution::consume_with_session(scene, session),
            Err(SceneExecutionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            )),
        ));
    }

    #[test]
    fn consumed_installed_scene_rejects_a_stale_publication() {
        let mut scene = Scene::new();
        let session = scene.execution_session().unwrap();
        scene.install_execution(session);
        // Integration edits can invalidate an already-installed component too.
        // Consuming that component must retain the same checked-owner contract.
        let mut external = noon_core::SemanticMutationTransaction::new();
        external.add_node(noon_core::SemanticNodeCreation::family());
        external
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        assert_ne!(
            scene.revision(),
            scene
                .owned_execution()
                .publication_context()
                .scene_revision(),
            "the semantic transaction must leave the installed execution stale",
        );
        assert!(matches!(
            scene.into_execution(),
            Err(SceneExecutionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            )),
        ));
    }
}

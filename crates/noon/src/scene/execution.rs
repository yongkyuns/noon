//! A consumed static Scene and its one execution component.
//!
//! This owner adds no schedule or clock. It keeps the canonical semantic arena
//! available to mutation-bearing input and retirement on native and WASM hosts.

use super::Scene;
use crate::{
    integration::{NativePointerInputPublication, NativePointerInputToken},
    ExecutionSession, ExecutionSessionInputError, ExecutionSessionPublicationError, LiveSession,
};
use noon_core::{NativePointerInput, SemanticNodeId, SemanticStore};
use std::{cell::RefCell, rc::Rc};

/// One consumed Scene paired with the execution lowered from its exact root.
#[derive(Debug)]
pub struct SceneExecution {
    scene: Scene,
}

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

    /// Consume this scene with its already-activated matching execution.
    ///
    /// The handoff validates arena identity, publication revision, and scene root;
    /// it never relowers or discards the supplied runtime's current state.
    pub fn into_execution_with_session(
        mut self,
        session: ExecutionSession,
    ) -> Result<SceneExecution, SceneExecutionError> {
        if self.execution.is_some() {
            return Err(SceneExecutionError::AlreadyExecuting);
        }
        session
            .require_scene_owner(&self.store.borrow(), self.root)
            .map_err(SceneExecutionError::Publication)?;
        self.install_execution(session);
        Ok(SceneExecution { scene: self })
    }

    /// Consume this scene with one explicitly selected authored animation graph.
    pub fn into_execution_with_animation_root(
        self,
        animation_root: &crate::DeclaredAnimation,
    ) -> Result<SceneExecution, String> {
        self.into_execution_with_animation_root_at(animation_root, 0.0)
    }

    pub fn into_execution_with_animation_root_at(
        self,
        animation_root: &crate::DeclaredAnimation,
        origin: f64,
    ) -> Result<SceneExecution, String> {
        let session = self.execution_session_with_animation_root_at(animation_root, origin)?;
        self.into_execution_with_session(session)
            .map_err(|error| error.to_string())
    }

    /// Adopt an existing semantic family as an integration scene's canonical root.
    ///
    /// The same arena and family are retained. This performs no copy, membership
    /// mutation, or execution bootstrap; detached declarations remain detached.
    pub fn with_integration_root(
        store: Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
    ) -> Result<Self, noon_core::SemanticSceneOperationError> {
        store.borrow().semantic_family_checked(root)?;
        Ok(Self {
            store,
            root,
            cursor: 0.0,
            sections: Vec::new(),
            execution: None,
            source_generation: None,
        })
    }
}

impl SceneExecution {
    /// Transfer this checked pair into an existing language/context adapter.
    /// No arena or runtime is copied, replaced, or lowered during the handoff.
    pub fn into_integration_parts(
        mut self,
    ) -> (Rc<RefCell<SemanticStore>>, SemanticNodeId, ExecutionSession) {
        let session = self
            .scene
            .execution
            .take()
            .expect("consumed Scene owns execution");
        (self.scene.store, self.scene.root, session)
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

    /// Borrow the canonical semantic and execution pair for one integration call.
    /// The semantic borrow cannot escape the closure or cross an async boundary.
    pub fn with_store_and_session<R>(
        &mut self,
        apply: impl FnOnce(&mut SemanticStore, &mut ExecutionSession) -> R,
    ) -> R {
        let store = Rc::clone(self.scene.integration_store());
        let mut store = store.borrow_mut();
        apply(&mut store, self.scene.owned_execution_mut())
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
        self.with_store_and_session(|store, session| {
            session.seek_with_store(store, time).map(|_| ())
        })?;
        Ok(self.session().frame())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{
        NativeInputModifiers, NativePointerId, NativePointerInputKind, NativePointerPosition, Vec2,
    };

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
                .runtime
                .effective_object(object)
                .unwrap()
                .glow
                .as_ref()
                .unwrap()
                .definition
                .intensity(),
            2.0,
        );
        let time = execution.session().frame().time;

        execution.retire_execution_source().unwrap();

        assert!(!execution.session().interactions_active());
        assert_eq!(execution.session().frame().time, time);
        assert!(execution
            .session()
            .runtime
            .effective_object(object)
            .unwrap()
            .glow
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
        let mut execution = scene.into_execution_with_session(session).unwrap();
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
            right.into_execution_with_session(session),
            Err(SceneExecutionError::Publication(
                ExecutionSessionPublicationError::ForeignSemanticStore
            )),
        ));

        let left = Scene::new();
        let right = Scene::with_integration_store(Rc::clone(left.integration_store()));
        let session = left.execution_session().unwrap();
        let root = right.root();
        assert!(matches!(
            right.into_execution_with_session(session),
            Err(SceneExecutionError::Publication(ExecutionSessionPublicationError::UnknownObject(found))) if found == root,
        ));
    }

    #[test]
    fn consumed_scene_rejects_a_stale_publication() {
        let mut scene = Scene::new();
        let session = scene.execution_session().unwrap();
        scene.circle(0.5).unwrap();
        assert!(matches!(
            scene.into_execution_with_session(session),
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
        scene.integration_store().borrow_mut().insert_family();
        assert!(matches!(
            scene.into_execution(),
            Err(SceneExecutionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            )),
        ));
    }
}

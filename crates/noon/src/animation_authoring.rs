//! Replayable semantic animation declarations.
//!
//! A declaration is authored into the same semantic store as its objects before
//! an execution session is lowered.  It is deliberately not a runtime command:
//! [`crate::LiveSession`] activates the existing declaration against one already
//! published session and returns that session's continuation segment.

use crate::Mobject;
use noon_core::{
    AnimationOptions, SemanticAnimationIntent, SemanticAnimationState, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticNodeId, SemanticNodeKind, SemanticStore,
};
use std::{cell::RefCell, rc::Rc};

/// Store-scoped handle for one authored semantic animation declaration.
///
/// The declaration stays ordinary replayable scene meaning. It neither owns a
/// session nor removes itself after a live run; lifecycle/reconciliation of
/// completed declarations belongs to the future continuation contract.
#[derive(Clone, Debug)]
pub struct DeclaredAnimation {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

impl DeclaredAnimation {
    pub(crate) fn new(store: Rc<RefCell<SemanticStore>>, node: SemanticNodeId) -> Self {
        Self { store, node }
    }

    /// The semantic identity of this replayable declaration.
    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    /// Read the current authored options from the authoritative semantic store.
    pub fn options(&self) -> Result<AnimationOptions, String> {
        self.store
            .borrow()
            .semantic_animation_state(self.node)
            .map(|state| state.options())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, store) {
            return Err("animation belongs to another semantic store".into());
        }
        match self.store.borrow().node(self.node).map(|node| node.kind()) {
            Some(SemanticNodeKind::Animation(_)) => Ok(()),
            Some(_) => Err("semantic handle is not an animation declaration".into()),
            None => Err("animation declaration is stale or has been removed".into()),
        }
    }
}

impl crate::Scene {
    /// Declare an animation before lowering an execution session.
    ///
    /// This is an authored transaction, so invalid references/options fail
    /// before the declaration receives an identity. Calling this after a
    /// session is lowered intentionally requires a future atomic
    /// declaration-and-publication operation.
    pub fn declare_animation(
        &self,
        intent: SemanticAnimationIntent,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, String> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_animation(SemanticAnimationState::new(intent, options));
        let result = transaction
            .apply(&mut self.store().borrow_mut())
            .map_err(|error| error.to_string())?;
        let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
            return Err("animation declaration did not produce one animation identity".into());
        };
        Ok(DeclaredAnimation::new(Rc::clone(self.store()), *animation))
    }

    /// Declare a shared affine transform between two store-scoped mobjects.
    pub fn declare_transform_to(
        &self,
        source: &Mobject,
        target: &Mobject,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, String> {
        self.require_object(source)?;
        self.require_object(target)?;
        self.declare_animation(
            SemanticAnimationIntent::TransformTo {
                target: source.node_id(),
                target_state: target.node_id(),
            },
            options,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{RateFunction, SemanticVec3, Vec2};

    #[test]
    fn declared_transform_is_replayable_and_live_queries_observe_its_effective_state() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, -2.0).unwrap();
        scene.add(&circle).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();

        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation,
            Vec2::new(2.0, -1.0)
        );
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::new(0.0, 0.0, 0.0)
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert!(live.segment_state(segment).is_complete());
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation,
            Vec2::new(4.0, -2.0)
        );

        let wait = live.wait_segment(0.5).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        assert!(live.segment_state(wait).is_complete());
    }

    #[test]
    fn declaration_rejects_foreign_target_before_allocating_an_animation() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let foreign = crate::Scene::new().circle(1.0).unwrap();
        let revision = scene.store().borrow().scene_revision();

        assert!(scene
            .declare_transform_to(&circle, &foreign, AnimationOptions::new())
            .is_err());
        assert_eq!(scene.store().borrow().scene_revision(), revision);
    }

    #[test]
    fn foreign_declaration_leaves_the_existing_live_runtime_unchanged() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();

        let mut foreign_scene = crate::Scene::new();
        let foreign_source = foreign_scene.circle(1.0).unwrap();
        let mut foreign_target = foreign_source.target_editor().unwrap();
        foreign_target.set_translation(3.0, 0.0).unwrap();
        foreign_scene.add(&foreign_source).unwrap();
        let foreign = foreign_scene
            .declare_transform_to(
                &foreign_source,
                &foreign_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        {
            let mut live = scene.live(&mut session);
            assert!(live.play_animation(&foreign).is_err());
            assert_eq!(
                live.effective(&circle).unwrap().transform.translation,
                Vec2::new(0.0, 0.0)
            );
        }
        assert_eq!(session.frame().time, 0.0);
    }
}

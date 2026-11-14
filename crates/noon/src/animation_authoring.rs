//! Replayable semantic animation declarations.
//!
//! Scene publishes declarations into the same semantic store as their targets.
//! [`crate::LiveSession`] activates a declaration and returns the continuation segment.

use crate::{AuthoringError, Mobject};
use noon_core::{
    AnimationOptions, SemanticAnimationIntent, SemanticAnimationState, SemanticMutationTransaction,
    SemanticNodeId, SemanticNodeKind, SemanticStore,
};
use std::{cell::RefCell, rc::Rc};

/// Store-scoped handle for one authored semantic animation declaration.
///
/// A declaration stays available for future activation after a segment completes.
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

    pub(crate) fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
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
    /// Publish an animation declaration during setup or at an authoring
    /// boundary. Playback starts separately when the declaration is activated.
    pub fn declare_animation(
        &mut self,
        intent: SemanticAnimationIntent,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        let token = transaction.create_animation(SemanticAnimationState::new(intent, options));
        let animation = self
            .apply_semantic_transaction(transaction)?
            .resolve(token)
            .ok_or(AuthoringError::UnresolvedCreatedNode(token))?;
        Ok(DeclaredAnimation::new(
            Rc::clone(self.integration_store()),
            animation,
        ))
    }

    /// Declare a shared affine transform between two store-scoped mobjects.
    pub fn declare_transform_to(
        &mut self,
        source: &Mobject,
        target: &Mobject,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        self.require_object(source)?;
        self.require_object(target)?;
        self.declare_animation(
            SemanticAnimationIntent::TransformTo {
                target: source.node_id(),
                target_state: target.node_id(),
                interpolation: noon_core::SemanticTransformInterpolation::Affine,

                complete_priority: false,
            },
            options,
        )
    }

    /// Declare exact bounds-center motion over a retained static path.
    pub fn declare_move_along_path(
        &mut self,
        target: &Mobject,
        path: &Mobject,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        self.require_object(target)?;
        self.require_object(path)?;
        self.declare_animation(
            SemanticAnimationIntent::MoveAlongPath {
                target: target.node_id(),
                path: path.node_id(),
            },
            options,
        )
    }

    /// Declare a replayable intensity pulse. Activation captures the effective glow;
    /// an absent canonical attachment is enrolled neutrally by the live session.
    pub fn declare_glow_pulse(
        &mut self,
        target: &Mobject,
        intensity: f64,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        self.require_object(target)?;
        self.declare_animation(
            SemanticAnimationIntent::GlowPulse {
                target: target.node_id(),
                intensity,
            },
            options,
        )
    }

    /// Declare a replayable visibility gesture without enrolling a mask yet.
    pub fn declare_dissolve<'a>(
        &mut self,
        target: impl Into<crate::MobjectTarget<'a>>,
        direction: noon_core::DissolveDirection,
        style: noon_core::DissolveStyle,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        let target = target.into();
        target.require_store(self.integration_store())?;
        self.declare_animation(
            SemanticAnimationIntent::Dissolve {
                target: target.node_id(),
                direction,
                style,
            },
            options,
        )
    }

    /// Declare one transient flash over an exact analytic Line.
    pub fn declare_passing_flash(
        &mut self,
        target: &Mobject,
        time_width: f64,
        options: AnimationOptions,
    ) -> Result<DeclaredAnimation, AuthoringError> {
        self.require_object(target)?;
        let options = normalized_passing_flash_options(options)
            .map_err(AuthoringError::InvalidAnimationDeclaration)?;
        self.declare_animation(
            SemanticAnimationIntent::PassingFlash {
                target: target.node_id(),
                time_width,
            },
            options,
        )
    }
}

pub(crate) fn normalized_passing_flash_options(
    options: AnimationOptions,
) -> Result<AnimationOptions, &'static str> {
    if options.introducer == Some(false)
        || options.remover == Some(false)
        || options.reverse_rate_function == Some(true)
        || options.lag_ratio.is_some_and(|value| value != 0.0)
        || options.path_arc.is_some_and(|value| value != 0.0)
    {
        return Err("PassingFlash has fixed transient membership and does not support lag, path arcs, or rate reversal");
    }
    Ok(options.introducer(true).remover(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_core::{RateFunction, SemanticVec3, Vec2};

    #[test]
    fn declaration_publishes_without_activating_and_rejects_a_pending_segment_atomically() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let target = scene.circle(1.0).unwrap();
        scene.set_translation(&target, 4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let mut execution = scene.into_execution().unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let animation = execution
            .scene_mut()
            .declare_transform_to(&circle, &target, options)
            .unwrap();
        assert_eq!(execution.session().frame().time, 0.0);
        assert_eq!(
            execution
                .scene()
                .effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::ZERO,
        );
        let segment = execution.live().play_animation(&animation).unwrap();
        let publication = execution.session().publication_context();
        let revision = execution.scene().revision();
        let count = execution.scene().integration_store().borrow().len();
        assert!(matches!(
            execution
                .scene_mut()
                .declare_transform_to(&circle, &target, options),
            Err(AuthoringError::ExecutionPublication(
                crate::ExecutionSessionPublicationError::SegmentCompletionPending
            )),
        ));
        assert_eq!(execution.scene().revision(), revision);
        assert_eq!(execution.scene().integration_store().borrow().len(), count);
        assert_eq!(execution.session().publication_context(), publication);
        execution.live().advance_segment_to(segment, 2.0).unwrap();
        execution.live().complete_segment(segment).unwrap();
        assert_eq!(
            execution
                .scene()
                .effective(&circle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(4.0, 0.0),
        );
    }

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
            circle.state().unwrap().transform.translation,
            SemanticVec3::new(0.0, 0.0, 0.0)
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert!(!live.segment_state(segment).is_complete());
        live.complete_segment(segment).unwrap();
        assert!(live.segment_state(segment).is_complete());
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation,
            Vec2::new(4.0, -2.0)
        );

        let wait = live.wait_segment(0.5).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        assert!(live.segment_state(wait).is_complete());
        live.complete_segment(wait).unwrap();
    }

    #[test]
    fn declaration_rejects_foreign_target_before_allocating_an_animation() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let foreign = crate::Scene::new().circle(1.0).unwrap();
        let revision = scene.integration_store().borrow().scene_revision();

        assert!(scene
            .declare_transform_to(&circle, &foreign, AnimationOptions::new())
            .is_err());
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
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

    #[test]
    fn declared_passing_flash_has_fixed_transient_lifecycle() {
        let mut scene = crate::Scene::new();
        let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        let animation = scene
            .declare_passing_flash(&line, 0.25, AnimationOptions::new().run_time(2.0))
            .unwrap();
        let options = animation.options().unwrap();
        assert_eq!(options.introducer, Some(true));
        assert_eq!(options.remover, Some(true));
        assert!(scene
            .declare_passing_flash(&line, 0.25, AnimationOptions::new().remover(false),)
            .is_err());
    }
}

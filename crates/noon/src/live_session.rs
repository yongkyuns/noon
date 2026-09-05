//! Borrowed live access to one already-published execution session.
//!
//! This facade owns neither semantic nor runtime state.  It only coordinates a
//! transaction with the session that already lowered the same semantic store.
//! Membership and property publication use the same prepared semantic transaction;
//! animation continuation remains a separate lifecycle contract.

use crate::{EffectiveSemanticObject, ExecutionSession, ExecutionSessionPublicationError, Mobject};
use noon_core::{
    PublicationContext, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeId, SemanticObjectProperty, SemanticObjectState, SemanticSignalValue,
    SemanticStore, SemanticStyle, Style, Transform2D,
};
use std::{cell::RefCell, rc::Rc};

/// An owned observation of one effective runtime object at one publication.
///
/// The clone makes the observation safe to retain after the next live mutation;
/// it is an observation, not another runtime authority.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveMobjectState {
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub publication: PublicationContext,
}

/// Errors while a semantic handle is used through a live execution session.
#[derive(Debug)]
pub enum LiveSessionError {
    ForeignMobjectStore,
    Mobject(String),
    Publication(ExecutionSessionPublicationError),
}

impl std::fmt::Display for LiveSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignMobjectStore => {
                formatter.write_str("mobject belongs to another semantic store")
            }
            Self::Mobject(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LiveSessionError {}

impl From<ExecutionSessionPublicationError> for LiveSessionError {
    fn from(value: ExecutionSessionPublicationError) -> Self {
        Self::Publication(value)
    }
}

/// A temporary, typed view over one semantic store and its published runtime.
///
/// `LiveSession` has no scheduler, scene copy, or runtime mirror.  Persistent
/// property edits use the shared semantic transaction vocabulary and publish
/// through [`ExecutionSession`] atomically.  The supported transaction subset is
/// exactly the session publication subset.
pub struct LiveSession<'a> {
    store: &'a Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    session: &'a mut ExecutionSession,
}

impl<'a> LiveSession<'a> {
    /// Bind a facade to the supplied store and existing execution session.
    /// Provenance and revision are checked by every publish/query operation.
    pub fn new(
        store: &'a Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
        session: &'a mut ExecutionSession,
    ) -> Self {
        Self {
            store,
            root,
            session,
        }
    }

    /// Apply one supported semantic transaction and publish it into the same
    /// runtime. Unsupported content, ordering, and structural work fails before
    /// either layer commits.
    pub fn apply(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .apply_semantic_transaction(&mut store, transaction)
            .map_err(Into::into)
    }

    /// Add an existing detached object to this live scene root.
    pub fn add(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(self.root, mobject.node_id());
        self.apply(transaction)
    }

    /// Remove an existing object from this live scene root without deleting identity.
    pub fn remove(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_member(self.root, mobject.node_id());
        self.apply(transaction)
    }

    /// Inspect authored/base state explicitly, separate from [`Self::effective`].
    pub fn authored(&self, mobject: &Mobject) -> Result<SemanticObjectState, LiveSessionError> {
        self.require_mobject(mobject)?;
        mobject.state().map_err(LiveSessionError::Mobject)
    }

    /// Read the current effective runtime value at the session's publication.
    pub fn effective(&self, mobject: &Mobject) -> Result<EffectiveMobjectState, LiveSessionError> {
        self.require_mobject(mobject)?;
        let store = self.store.borrow();
        let EffectiveSemanticObject {
            object,
            publication,
        } = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        Ok(EffectiveMobjectState {
            transform: object.transform,
            style: object.style,
            appearance: object.appearance,
            publication,
        })
    }

    /// Set any already-supported semantic property through one atomic publish.
    pub fn set_property(
        &mut self,
        mobject: &Mobject,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(mobject.node_id(), property, value);
        self.apply(transaction)
    }

    pub fn set_translation(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut translation = self.authored(mobject)?.transform.translation;
        translation.x = x;
        translation.y = y;
        self.set_property(mobject, SemanticObjectProperty::Translation, translation)
    }

    pub fn shift(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut translation = self.authored(mobject)?.transform.translation;
        translation.x += x;
        translation.y += y;
        self.set_property(mobject, SemanticObjectProperty::Translation, translation)
    }

    pub fn set_scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut scale = self.authored(mobject)?.transform.scale;
        scale.x = x;
        scale.y = y;
        self.set_property(mobject, SemanticObjectProperty::Scale, scale)
    }

    pub fn set_rotation(
        &mut self,
        mobject: &Mobject,
        angle: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.set_property(mobject, SemanticObjectProperty::RotationZ, angle)
    }

    pub fn replace_style(
        &mut self,
        mobject: &Mobject,
        style: SemanticStyle,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(mobject.node_id(), style);
        self.apply(transaction)
    }

    fn require_mobject(&self, mobject: &Mobject) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, mobject.store()) {
            return Err(LiveSessionError::ForeignMobjectStore);
        }
        mobject.validate().map_err(LiveSessionError::Mobject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;
    use noon_core::{
        AnimationOptions, RateFunction, SemanticAnimationIntent, SemanticAnimationState,
        SemanticMutationImpact, SemanticObjectContent, SemanticVec3, StoredGeometry,
    };

    #[test]
    fn live_property_edits_publish_once_and_queries_are_effective_not_authored_aliases() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();

        let observed = {
            let mut live = scene.live(&mut session);
            live.set_translation(&circle, 2.0, -1.0).unwrap();
            live.set_scale(&circle, 1.5, 0.5).unwrap();
            live.set_rotation(&circle, 0.25).unwrap();
            let authored = live.authored(&circle).unwrap();
            let effective = live.effective(&circle).unwrap();
            assert_eq!(
                authored.transform.translation,
                SemanticVec3::new(2.0, -1.0, 0.0)
            );
            assert_eq!(effective.transform.translation.x, 2.0);
            assert_eq!(effective.transform.translation.y, -1.0);
            assert_eq!(effective.transform.scale.x, 1.5);
            assert_eq!(effective.transform.rotation, 0.25);
            effective
        };
        assert_eq!(observed.transform.translation.x, 2.0);
        assert_eq!(session.frame().objects.len(), 1);
    }

    #[test]
    fn live_facade_rejects_foreign_handles_and_unsupported_publication_without_fallback() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let foreign = Scene::new().circle(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = scene.store().borrow().scene_revision();

        let mut live = scene.live(&mut session);
        assert!(matches!(
            live.effective(&foreign),
            Err(LiveSessionError::ForeignMobjectStore)
        ));
        let mut unsupported = SemanticMutationTransaction::new();
        unsupported.replace_content(
            circle.node_id(),
            SemanticObjectContent::Geometry(StoredGeometry::Circle { radius: 2.0 }),
        );
        assert!(matches!(
            live.apply(unsupported),
            Err(LiveSessionError::Publication(_))
        ));
        assert_eq!(scene.store().borrow().scene_revision(), before);
    }

    #[test]
    fn live_query_observes_the_active_driver_while_authored_edits_remain_explicit() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let options = AnimationOptions::new()
            .run_time(2.0)
            .rate_func(RateFunction::Linear);
        let mut declaration = SemanticMutationTransaction::new();
        declaration.add_animation(SemanticAnimationState::new(
            SemanticAnimationIntent::TransformTo {
                target: circle.node_id(),
                target_state: target.node_id(),
            },
            options,
        ));
        let result = session
            .apply_semantic_transaction(&mut scene.store().borrow_mut(), declaration)
            .unwrap();
        let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
            panic!("one animation declaration must allocate one identity")
        };
        session
            .activate_animation(&scene.store().borrow(), *animation, options)
            .unwrap();
        session.seek(1.0).unwrap();

        let mut live = scene.live(&mut session);
        live.set_translation(&circle, 100.0, 0.0).unwrap();
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::new(100.0, 0.0, 0.0)
        );
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            2.0
        );
    }

    #[test]
    fn live_membership_detaches_readds_and_appends_without_changing_unrelated_slots() {
        let mut scene = Scene::new();
        let anchor = scene.circle(1.0).unwrap();
        let toggled = scene.circle(2.0).unwrap();
        let detached = scene.circle(3.0).unwrap();
        scene.add(&anchor).unwrap();
        scene.add(&toggled).unwrap();
        let mut session = scene.execution_session().unwrap();
        let anchor_slot = session.execution_slot_for_frame_index(0).unwrap();

        {
            let mut live = scene.live(&mut session);
            live.remove(&toggled).unwrap();
            assert!(live.effective(&toggled).is_err());
            live.add(&toggled).unwrap();
            assert!(live.effective(&toggled).is_ok());
            live.add(&detached).unwrap();
            assert_eq!(
                live.session
                    .last_structural_publication_stats()
                    .entered_objects,
                1
            );
            live.set_translation(&detached, 4.0, -2.0).unwrap();
            assert_eq!(
                live.effective(&detached).unwrap().transform.translation,
                noon_core::Vec2::new(4.0, -2.0)
            );
        }

        assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));
        assert_eq!(session.frame().objects.len(), 4);
    }
}

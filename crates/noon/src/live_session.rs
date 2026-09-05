//! Borrowed live access to one already-published execution session.
//!
//! This facade owns neither semantic nor runtime state.  It only coordinates a
//! transaction with the session that already lowered the same semantic store.
//! Structural/content publication and animation continuation deliberately stay
//! outside this bounded property-edit slice until their incremental lowering
//! contracts are available.

use crate::{EffectiveSemanticObject, ExecutionSession, ExecutionSessionPublicationError, Mobject};
use noon_core::{
    PublicationContext, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticObjectProperty, SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticStyle,
    SemanticVec3, Style, Transform2D,
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
/// exactly the session publication subset (currently transform/style values).
pub struct LiveSession<'a> {
    store: &'a Rc<RefCell<SemanticStore>>,
    session: &'a mut ExecutionSession,
}

impl<'a> LiveSession<'a> {
    /// Bind a facade to the supplied store and existing execution session.
    /// Provenance and revision are checked by every publish/query operation.
    pub fn new(store: &'a Rc<RefCell<SemanticStore>>, session: &'a mut ExecutionSession) -> Self {
        Self { store, session }
    }

    /// Apply one supported semantic transaction and publish it into the same
    /// runtime. Unsupported structural/content work fails before either layer
    /// commits; callers must use the future incremental-publication contract.
    pub fn apply(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .apply_semantic_transaction(&mut store, transaction)
            .map_err(Into::into)
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
            style: object.style.clone(),
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
    use noon_core::{SemanticObjectContent, StoredGeometry};

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
}

//! Thin Rust appearance authoring over the canonical semantic transaction path.
//! Finite glow playback uses the same Scene/LiveSession publication owners.
use std::{cell::RefCell, rc::Rc};

use noon_core::{
    EffectDefinition, EffectUpdate, Glow, SemanticMutationTransaction, SemanticNodeId,
    SemanticStore,
};

use crate::{
    AuthoringError, LiveSession, LiveSessionError, Mobject, MobjectFamily, MobjectTarget, Scene,
};

/// A store-scoped generational attachment reference. Dropping it never detaches
/// an effect. `authored_definition` is explicitly not an effective-runtime query.
#[derive(Clone, Debug)]
pub struct EffectHandle {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

impl EffectHandle {
    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    pub fn authored_definition(&self) -> Result<EffectDefinition, AuthoringError> {
        Ok(self
            .store
            .borrow()
            .semantic_effect_state(self.node)?
            .definition())
    }
}

/// Named lookup or an exact handle; a stale handle never resolves by name.
#[derive(Clone, Copy)]
pub enum EffectSelector<'a> {
    Name(&'a str),
    Handle(&'a EffectHandle),
}
impl<'a> From<&'a str> for EffectSelector<'a> {
    fn from(name: &'a str) -> Self {
        Self::Name(name)
    }
}
impl<'a> From<&'a EffectHandle> for EffectSelector<'a> {
    fn from(handle: &'a EffectHandle) -> Self {
        Self::Handle(handle)
    }
}

fn lookup(
    object: MobjectTarget<'_>,
    selector: EffectSelector<'_>,
) -> Result<EffectHandle, AuthoringError> {
    object.validate()?;
    let store = object.integration_store();
    let node = match selector {
        EffectSelector::Name(name) => store
            .borrow()
            .effect_by_name(object.node_id(), name)?
            .ok_or_else(|| AuthoringError::EffectNotFound {
                owner: object.node_id(),
                name: name.to_owned(),
            })?,
        EffectSelector::Handle(handle) => {
            if !Rc::ptr_eq(store, &handle.store) {
                return Err(AuthoringError::ForeignStore);
            }
            let owner = store.borrow().semantic_effect_state(handle.node)?.owner();
            if owner != object.node_id() {
                return Err(AuthoringError::EffectOwnerMismatch {
                    effect: handle.node,
                    owner: object.node_id(),
                });
            }
            handle.node
        }
    };
    Ok(EffectHandle {
        store: Rc::clone(store),
        node,
    })
}

fn glow_transaction(
    object: MobjectTarget<'_>,
    update: EffectUpdate,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let existing = object
        .integration_store()
        .borrow()
        .effect_by_name(object.node_id(), "glow")?;
    let mut tx = SemanticMutationTransaction::new();
    if let Some(effect) = existing {
        if object
            .integration_store()
            .borrow()
            .semantic_effect_state(effect)?
            .definition()
            .as_glow()
            .is_none()
        {
            return Err(noon_core::EffectParameterError::IncompatibleDefinition.into());
        }
        tx.update_effect(effect, update);
    } else {
        tx.create_effect(object.node_id(), "glow", Glow::new(update)?);
    }
    Ok(tx)
}

fn add_transaction(
    object: MobjectTarget<'_>,
    definition: EffectDefinition,
    name: &str,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(object.node_id(), name, definition);
    Ok(tx)
}

fn update_transaction(
    object: MobjectTarget<'_>,
    selector: EffectSelector<'_>,
    update: EffectUpdate,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let handle = lookup(object, selector)?;
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(handle.node, update);
    Ok(tx)
}

fn remove_transaction(
    object: MobjectTarget<'_>,
    selector: EffectSelector<'_>,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let handle = lookup(object, selector)?;
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(handle.node);
    Ok(tx)
}

fn remove_glow_transaction(
    object: MobjectTarget<'_>,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let mut tx = SemanticMutationTransaction::new();
    if let Some(effect) = object
        .integration_store()
        .borrow()
        .effect_by_name(object.node_id(), "glow")?
    {
        if object
            .integration_store()
            .borrow()
            .semantic_effect_state(effect)?
            .definition()
            .as_glow()
            .is_none()
        {
            return Err(noon_core::EffectParameterError::IncompatibleDefinition.into());
        }
        tx.remove_node(effect);
    }
    Ok(tx)
}

impl Mobject {
    /// Author the canonical glow. Like other raw Mobject mutators, this is for
    /// pre-execution/target editing; running scenes must use Scene or LiveSession.
    pub fn set_glow(&mut self, update: EffectUpdate) -> Result<(), AuthoringError> {
        glow_transaction((&*self).into(), update)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn add_effect(
        &mut self,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), AuthoringError> {
        add_transaction((&*self).into(), definition.into(), name)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn get_effect(&self, name: &str) -> Result<EffectHandle, AuthoringError> {
        lookup(self.into(), name.into())
    }

    pub fn set_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringError> {
        update_transaction((&*self).into(), selector.into(), update)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn remove_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), AuthoringError> {
        remove_transaction((&*self).into(), selector.into())?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    /// Idempotent when absent; an invalid object handle still fails.
    pub fn remove_glow(&mut self) -> Result<(), AuthoringError> {
        remove_glow_transaction((&*self).into())?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
}

impl MobjectFamily {
    /// Author an isolated family treatment before execution. Descendants never
    /// receive copied per-leaf attachments. Live edits use Scene or LiveSession.
    pub fn add_effect(
        &mut self,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), AuthoringError> {
        add_transaction((&*self).into(), definition.into(), name)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn get_effect(&self, name: &str) -> Result<EffectHandle, AuthoringError> {
        lookup(self.into(), name.into())
    }

    pub fn set_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringError> {
        update_transaction((&*self).into(), selector.into(), update)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn remove_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), AuthoringError> {
        remove_transaction((&*self).into(), selector.into())?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn set_glow(&mut self, update: EffectUpdate) -> Result<(), AuthoringError> {
        glow_transaction((&*self).into(), update)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn remove_glow(&mut self) -> Result<(), AuthoringError> {
        remove_glow_transaction((&*self).into())?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
}

impl Scene {
    /// Declare a glow on an object or isolated family through the normal scene transaction.
    pub fn set_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply_semantic_transaction(glow_transaction(object, update)?)?;
        Ok(())
    }

    pub fn add_effect<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply_semantic_transaction(add_transaction(object, definition.into(), name)?)?;
        Ok(())
    }

    pub fn get_effect<'owner>(
        &self,
        object: impl Into<MobjectTarget<'owner>>,
        name: &str,
    ) -> Result<EffectHandle, AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        lookup(object, name.into())
    }

    pub fn set_effect<'owner, 'a>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: impl Into<EffectSelector<'a>>,
        update: EffectUpdate,
    ) -> Result<(), AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply_semantic_transaction(update_transaction(object, selector.into(), update)?)?;
        Ok(())
    }

    pub fn remove_effect<'owner, 'a>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply_semantic_transaction(remove_transaction(object, selector.into())?)?;
        Ok(())
    }

    pub fn remove_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
    ) -> Result<(), AuthoringError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply_semantic_transaction(remove_glow_transaction(object)?)?;
        Ok(())
    }
}

impl LiveSession<'_> {
    /// Publish through the ordinary live transaction, including attachment
    /// enrollment/removal. Unsupported profiles fail before either state commits.
    pub fn set_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        update: EffectUpdate,
    ) -> Result<(), LiveSessionError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply(glow_transaction(object, update)?)?;
        Ok(())
    }

    /// Stage one atomic batch of named attachments for the existing program's
    /// publication-preparation barrier. No attachment/revision is visible yet.
    pub fn prepare_effect_attachments<'owner, 'name, T>(
        &mut self,
        attachments: impl IntoIterator<Item = (T, EffectDefinition, &'name str)>,
    ) -> Result<crate::StagedEffectPublication, LiveSessionError>
    where
        T: Into<MobjectTarget<'owner>>,
    {
        let mut transaction = SemanticMutationTransaction::new();
        for (object, definition, name) in attachments {
            let object = object.into();
            object.require_store(self.integration_store())?;
            object.validate()?;
            transaction.create_effect(object.node_id(), name, definition);
        }
        self.stage_effect_publication(transaction)
    }

    pub fn add_effect<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), LiveSessionError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply(add_transaction(object, definition.into(), name)?)?;
        Ok(())
    }

    pub fn set_effect<'owner, 'a>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: impl Into<EffectSelector<'a>>,
        update: EffectUpdate,
    ) -> Result<(), LiveSessionError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply(update_transaction(object, selector.into(), update)?)?;
        Ok(())
    }

    pub fn remove_effect<'owner, 'a>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), LiveSessionError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply(remove_transaction(object, selector.into())?)?;
        Ok(())
    }

    pub fn remove_glow<'owner>(
        &mut self,
        object: impl Into<MobjectTarget<'owner>>,
    ) -> Result<(), LiveSessionError> {
        let object = object.into();
        object.require_store(self.integration_store())?;
        self.apply(remove_glow_transaction(object)?)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn only_effect(
    stack: &noon_compile::CompiledEffectStack,
) -> noon_compile::CompiledEffect {
    assert_eq!(stack.len(), 1, "single-effect authoring fixture");
    stack.iter().next().unwrap()
}

#[cfg(test)]
mod activation_tests;
#[cfg(test)]
mod blur_tests;
#[cfg(test)]
mod bridge_tests;
#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod shadow_tests;
#[cfg(test)]
mod target_intent_tests;
#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "wgsl-effects"))]
mod shader_tests;

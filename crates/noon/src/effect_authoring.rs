//! Thin Rust appearance authoring over the canonical semantic transaction path.
//! This is the M0 declaration surface, not enabled effect playback/rendering.
use std::{cell::RefCell, rc::Rc};

use noon_core::{
    EffectDefinition, Glow, GlowUpdate, SemanticMutationTransaction, SemanticNodeId, SemanticStore,
};

use crate::{AuthoringError, LiveSession, LiveSessionError, Mobject, Scene};

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

fn require_store(
    store: &Rc<RefCell<SemanticStore>>,
    object: &Mobject,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, object.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    object.validate()
}

fn lookup(object: &Mobject, selector: EffectSelector<'_>) -> Result<EffectHandle, AuthoringError> {
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
    object: &Mobject,
    update: GlowUpdate,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let existing = object
        .integration_store()
        .borrow()
        .effect_by_name(object.node_id(), "glow")?;
    let mut tx = SemanticMutationTransaction::new();
    if let Some(effect) = existing {
        tx.update_effect(effect, update);
    } else {
        tx.create_effect(object.node_id(), "glow", Glow::new(update)?);
    }
    Ok(tx)
}

fn add_transaction(
    object: &Mobject,
    definition: EffectDefinition,
    name: &str,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let mut tx = SemanticMutationTransaction::new();
    tx.create_effect(object.node_id(), name, definition);
    Ok(tx)
}

fn update_transaction(
    object: &Mobject,
    selector: EffectSelector<'_>,
    update: GlowUpdate,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let handle = lookup(object, selector)?;
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(handle.node, update);
    Ok(tx)
}

fn remove_transaction(
    object: &Mobject,
    selector: EffectSelector<'_>,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    let handle = lookup(object, selector)?;
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_node(handle.node);
    Ok(tx)
}

fn remove_glow_transaction(
    object: &Mobject,
) -> Result<SemanticMutationTransaction, AuthoringError> {
    object.validate()?;
    let mut tx = SemanticMutationTransaction::new();
    if let Some(effect) = object
        .integration_store()
        .borrow()
        .effect_by_name(object.node_id(), "glow")?
    {
        tx.remove_node(effect);
    }
    Ok(tx)
}

impl Mobject {
    /// Author the canonical glow. Like other raw Mobject mutators, this is for
    /// pre-execution/target editing; running scenes must use Scene or LiveSession.
    pub fn set_glow(&mut self, update: GlowUpdate) -> Result<(), AuthoringError> {
        glow_transaction(self, update)?.apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn add_effect(
        &mut self,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), AuthoringError> {
        add_transaction(self, definition.into(), name)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn get_effect(&self, name: &str) -> Result<EffectHandle, AuthoringError> {
        lookup(self, name.into())
    }

    pub fn set_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
        update: GlowUpdate,
    ) -> Result<(), AuthoringError> {
        update_transaction(self, selector.into(), update)?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    pub fn remove_effect<'a>(
        &mut self,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), AuthoringError> {
        remove_transaction(self, selector.into())?
            .apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }

    /// Idempotent when absent; an invalid object handle still fails.
    pub fn remove_glow(&mut self) -> Result<(), AuthoringError> {
        remove_glow_transaction(self)?.apply(&mut self.integration_store().borrow_mut())?;
        Ok(())
    }
}

impl Scene {
    /// Declare a glow through the normal scene transaction. Execution remains
    /// explicitly unavailable for effect-bearing stores in this M0 slice.
    pub fn set_glow(&mut self, object: &Mobject, update: GlowUpdate) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        self.apply_semantic_transaction(glow_transaction(object, update)?)?;
        Ok(())
    }

    pub fn add_effect(
        &mut self,
        object: &Mobject,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        self.apply_semantic_transaction(add_transaction(object, definition.into(), name)?)?;
        Ok(())
    }

    pub fn get_effect(&self, object: &Mobject, name: &str) -> Result<EffectHandle, AuthoringError> {
        self.require_object(object)?;
        lookup(object, name.into())
    }

    pub fn set_effect<'a>(
        &mut self,
        object: &Mobject,
        selector: impl Into<EffectSelector<'a>>,
        update: GlowUpdate,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        self.apply_semantic_transaction(update_transaction(object, selector.into(), update)?)?;
        Ok(())
    }

    pub fn remove_effect<'a>(
        &mut self,
        object: &Mobject,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        self.apply_semantic_transaction(remove_transaction(object, selector.into())?)?;
        Ok(())
    }

    pub fn remove_glow(&mut self, object: &Mobject) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        self.apply_semantic_transaction(remove_glow_transaction(object)?)?;
        Ok(())
    }
}

impl LiveSession<'_> {
    /// The signature shares the ordinary live publication owner. Until M1 adds
    /// effect execution, a nonempty request is rejected before either state commits.
    pub fn set_glow(
        &mut self,
        object: &Mobject,
        update: GlowUpdate,
    ) -> Result<(), LiveSessionError> {
        require_store(self.integration_store(), object)?;
        self.apply(glow_transaction(object, update)?)?;
        Ok(())
    }

    pub fn add_effect(
        &mut self,
        object: &Mobject,
        definition: impl Into<EffectDefinition>,
        name: &str,
    ) -> Result<(), LiveSessionError> {
        require_store(self.integration_store(), object)?;
        self.apply(add_transaction(object, definition.into(), name)?)?;
        Ok(())
    }

    pub fn set_effect<'a>(
        &mut self,
        object: &Mobject,
        selector: impl Into<EffectSelector<'a>>,
        update: GlowUpdate,
    ) -> Result<(), LiveSessionError> {
        require_store(self.integration_store(), object)?;
        self.apply(update_transaction(object, selector.into(), update)?)?;
        Ok(())
    }

    pub fn remove_effect<'a>(
        &mut self,
        object: &Mobject,
        selector: impl Into<EffectSelector<'a>>,
    ) -> Result<(), LiveSessionError> {
        require_store(self.integration_store(), object)?;
        self.apply(remove_transaction(object, selector.into())?)?;
        Ok(())
    }

    pub fn remove_glow(&mut self, object: &Mobject) -> Result<(), LiveSessionError> {
        require_store(self.integration_store(), object)?;
        self.apply(remove_glow_transaction(object)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod activation_tests;
#[cfg(test)]
mod bridge_tests;
#[cfg(test)]
mod tests;

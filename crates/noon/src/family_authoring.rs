//! Typed handle for authoritative semantic family membership.

use noon_core::{SemanticNodeId, SemanticStore};
use std::{cell::RefCell, rc::Rc};

/// A semantic family identity in one shared scene store.
///
/// The handle retains no copied membership, schedule, or runtime state. Ordered
/// traversal always reads the authoritative semantic family at use time.
#[derive(Clone, Debug)]
pub struct MobjectFamily {
    store: Rc<RefCell<SemanticStore>>,
    node: SemanticNodeId,
}

/// One borrowed direct member of a family published through a live session.
#[derive(Clone, Copy)]
pub enum MobjectFamilyMember<'a> {
    Mobject(&'a crate::Mobject),
    Family(&'a MobjectFamily),
}

impl MobjectFamilyMember<'_> {
    pub(crate) fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        match self {
            Self::Mobject(member) => member.store(),
            Self::Family(member) => member.store(),
        }
    }

    pub(crate) fn node_id(&self) -> SemanticNodeId {
        match self {
            Self::Mobject(member) => member.node_id(),
            Self::Family(member) => member.node_id(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Mobject(member) => member.validate(),
            Self::Family(member) => member.validate(),
        }
    }
}

impl MobjectFamily {
    pub fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        node: SemanticNodeId,
    ) -> Result<Self, String> {
        store
            .borrow()
            .semantic_family_members_checked(node)
            .map_err(|error| error.to_string())?;
        Ok(Self { store, node })
    }

    pub fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }

    pub const fn node_id(&self) -> SemanticNodeId {
        self.node
    }

    pub fn validate(&self) -> Result<(), String> {
        self.store
            .borrow()
            .semantic_family_members_checked(self.node)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

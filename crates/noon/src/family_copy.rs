//! Atomic copies of the authoritative semantic family graph.
use crate::{Mobject, MobjectFamily, MobjectFamilyMember};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeCreation, SemanticNodeId, SemanticNodeKind, SemanticObjectState, SemanticStore,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

/// A detached copied family and a derived mapping for host wrapper reconstruction.
/// The semantic store owns all copied nodes; dropping this lookup does not change them.
#[derive(Clone, Debug)]
pub struct FamilyCopy {
    root: MobjectFamily,
    copied: BTreeMap<SemanticNodeId, SemanticNodeId>,
}

impl FamilyCopy {
    pub fn root(&self) -> &MobjectFamily {
        &self.root
    }

    pub fn mobject(&self, source: &Mobject) -> Result<Mobject, String> {
        self.require_store(source.integration_store())?;
        source.validate().map_err(|error| error.to_string())?;
        Mobject::from_node(
            Rc::clone(self.root.integration_store()),
            self.copied_id(source.node_id())?,
        )
    }

    pub fn family(&self, source: &MobjectFamily) -> Result<MobjectFamily, String> {
        self.require_store(source.integration_store())?;
        source.validate().map_err(|error| error.to_string())?;
        MobjectFamily::from_node(
            Rc::clone(self.root.integration_store()),
            self.copied_id(source.node_id())?,
        )
    }

    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), String> {
        if Rc::ptr_eq(self.root.integration_store(), store) {
            Ok(())
        } else {
            Err("family copy source belongs to another semantic store".into())
        }
    }

    fn copied_id(&self, source: SemanticNodeId) -> Result<SemanticNodeId, String> {
        self.copied
            .get(&source)
            .copied()
            .ok_or_else(|| "source is not part of this family copy".into())
    }
}

pub(crate) struct PendingFamilyCopy {
    store: Rc<RefCell<SemanticStore>>,
    source_root: SemanticNodeId,
    copied: BTreeMap<SemanticNodeId, SemanticLocalNodeToken>,
}

impl PendingFamilyCopy {
    pub(crate) fn resolve(
        self,
        result: &SemanticMutationTransactionResult,
    ) -> Result<FamilyCopy, String> {
        let copied: BTreeMap<_, _> = self
            .copied
            .into_iter()
            .map(|(source, token)| {
                (
                    source,
                    result
                        .resolve(token)
                        .expect("committed copied node resolves"),
                )
            })
            .collect();
        let root = MobjectFamily::from_node(self.store, copied[&self.source_root])?;
        Ok(FamilyCopy { root, copied })
    }
}

/// Prepare every copied node and edge before the caller publishes anything.
/// Shared aliases allocate once; traversal and temporary work stay family-local.
pub(crate) fn prepare_family_copy(
    source: &MobjectFamily,
    references: &[MobjectFamilyMember<'_>],
    mut capture: impl FnMut(&Mobject) -> Result<SemanticObjectState, String>,
) -> Result<(SemanticMutationTransaction, PendingFamilyCopy), String> {
    source.validate().map_err(|error| error.to_string())?;
    let store = source.integration_store();
    let mut transaction = SemanticMutationTransaction::new();
    let mut copied = BTreeMap::new();
    let mut edges = Vec::new();
    let mut queue = Vec::with_capacity(references.len() + 1);
    for member in references {
        if !Rc::ptr_eq(store, member.integration_store()) {
            return Err("family copy reference belongs to another semantic store".into());
        }
        member.validate().map_err(|error| error.to_string())?;
        queue.push(member.node_id());
    }
    queue.push(source.node_id());
    while let Some(id) = queue.pop() {
        if copied.contains_key(&id) {
            continue;
        }
        let members = {
            let store = store.borrow();
            let node = store
                .node(id)
                .ok_or("family copy contains an unknown semantic node")?;
            match node.kind() {
                SemanticNodeKind::Family => Some(node.members_iter().collect::<Vec<_>>()),
                SemanticNodeKind::AuthoringObject => None,
                _ => return Err("family copy contains a non-mobject member".into()),
            }
        };
        let creation = if let Some(members) = members {
            queue.extend(members.iter().rev().copied());
            edges.push((id, members));
            SemanticNodeCreation::family()
        } else {
            let mobject = Mobject::from_node(Rc::clone(store), id)?;
            SemanticNodeCreation::object(capture(&mobject)?)
        };
        copied.insert(id, transaction.create_node(creation));
    }
    for (parent, members) in edges {
        for member in members {
            transaction.add_member(copied[&parent], copied[&member]);
        }
    }
    Ok((
        transaction,
        PendingFamilyCopy {
            store: Rc::clone(store),
            source_root: source.node_id(),
            copied,
        },
    ))
}

impl MobjectFamily {
    /// Copy the complete authored family, preserving order and internal aliases.
    pub fn copy_family(&self) -> Result<FamilyCopy, String> {
        self.copy_with_references(&[])
    }

    /// Copy a family and additional detached references in one transaction.
    /// References may represent saved states or other host metadata. They are
    /// copied once, including aliases into the family, but never added as members.
    pub fn copy_with_references(
        &self,
        references: &[MobjectFamilyMember<'_>],
    ) -> Result<FamilyCopy, String> {
        let (transaction, pending) = prepare_family_copy(self, references, Mobject::state)?;
        let result = transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(|e| e.to_string())?;
        pending.resolve(&result)
    }
}

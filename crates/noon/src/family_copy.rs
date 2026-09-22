//! Atomic copies of the authoritative semantic family graph.
use crate::AuthoringError;
use crate::{ManimArrow, ManimArrowVectorField, Mobject, MobjectFamily, MobjectTarget};
use noon_core::{
    SemanticLocalNodeToken, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticGraphEdgeDependency, SemanticNodeCreation, SemanticNodeId, SemanticNodeKind,
    SemanticObjectState, SemanticStore, SemanticTransactionGraphDeclaration,
    SemanticTransactionGraphEdgeBinding,
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

    pub fn mobject(&self, source: &Mobject) -> Result<Mobject, AuthoringError> {
        self.require_store(source.integration_store())?;
        source.validate()?;
        Mobject::from_node(
            Rc::clone(self.root.integration_store()),
            self.copied_id(source.node_id())?,
        )
    }

    pub fn family(&self, source: &MobjectFamily) -> Result<MobjectFamily, AuthoringError> {
        self.require_store(source.integration_store())?;
        source.validate()?;
        MobjectFamily::from_node(
            Rc::clone(self.root.integration_store()),
            self.copied_id(source.node_id())?,
        )
    }

    /// Rebind an Arrow aggregate to the semantic components created by this
    /// copy. This never recreates geometry: every component must already be in
    /// the authoritative copied-family map.
    pub fn rebind_manim_arrow(&self, source: &ManimArrow) -> Result<ManimArrow, AuthoringError> {
        Ok(ManimArrow::from_copied_components(
            self.family(source.family())?,
            self.mobject(source.shaft())?,
            self.mobject(source.end_tip())?,
            source
                .start_tip()
                .map(|tip| self.mobject(tip))
                .transpose()?,
        ))
    }

    /// Rebind a static ArrowVectorField aggregate using only its copied outer
    /// family and copied Arrow component identities.
    pub fn rebind_manim_arrow_vector_field(
        &self,
        source: &ManimArrowVectorField,
    ) -> Result<ManimArrowVectorField, AuthoringError> {
        let vectors = source
            .vectors()
            .iter()
            .map(|arrow| self.rebind_manim_arrow(arrow))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ManimArrowVectorField::from_copied_components(
            self.family(source.family())?,
            vectors,
        ))
    }

    /// Rebind one ArrowVectorField member as an independent Arrow aggregate.
    /// The selected vector family must be present in this copy; callers use
    /// this when copying one field member without its outer field family.
    pub fn rebind_manim_arrow_vector(
        &self,
        source: &ManimArrowVectorField,
        index: usize,
    ) -> Result<ManimArrow, AuthoringError> {
        let arrow = source
            .vectors()
            .get(index)
            .ok_or(AuthoringError::InvalidSubmobjectIndex {
                family: source.family().node_id(),
                index: index as isize,
            })?;
        self.rebind_manim_arrow(arrow)
    }

    fn require_store(&self, store: &Rc<RefCell<SemanticStore>>) -> Result<(), AuthoringError> {
        if Rc::ptr_eq(self.root.integration_store(), store) {
            Ok(())
        } else {
            Err(AuthoringError::ForeignStore)
        }
    }

    fn copied_id(&self, source: SemanticNodeId) -> Result<SemanticNodeId, AuthoringError> {
        self.copied
            .get(&source)
            .copied()
            .ok_or(AuthoringError::MissingCopySource(source))
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
    ) -> Result<FamilyCopy, AuthoringError> {
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
pub(crate) fn prepare_family_copy<E: From<AuthoringError>>(
    source: &MobjectFamily,
    references: &[MobjectTarget<'_>],
    mut capture: impl FnMut(&Mobject) -> Result<SemanticObjectState, E>,
) -> Result<(SemanticMutationTransaction, PendingFamilyCopy), E> {
    source.validate()?;
    let store = source.integration_store();
    let mut transaction = SemanticMutationTransaction::new();
    let mut copied = BTreeMap::new();
    let mut edges = Vec::new();
    let mut graph_declarations = Vec::new();
    let mut queue = Vec::with_capacity(references.len() + 1);
    for target in references {
        queue.push(target.require_store(store)?);
    }
    queue.push(source.node_id());
    while let Some(id) = queue.pop() {
        if copied.contains_key(&id) {
            continue;
        }
        let (members, family_z, graph_declaration) = {
            let store = store.borrow();
            let node = store.node(id).ok_or_else(|| {
                AuthoringError::from(noon_core::SemanticSceneOperationError::UnknownNode(id))
            })?;
            match node.kind() {
                SemanticNodeKind::Family(presentation) => (
                    Some(node.members_iter().collect::<Vec<_>>()),
                    Some(presentation.z_index),
                    node.graph_declaration().cloned(),
                ),
                SemanticNodeKind::AuthoringObject => (None, None, None),
                _ => {
                    return Err(AuthoringError::from(
                        noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(id),
                    )
                    .into())
                }
            }
        };
        if let Some(graph) = graph_declaration {
            graph_declarations.push((id, graph));
        }
        let creation = if let Some(members) = members {
            queue.extend(members.iter().rev().copied());
            edges.push((id, members));
            SemanticNodeCreation::family()
        } else {
            let mobject = Mobject::from_node(Rc::clone(store), id)?;
            SemanticNodeCreation::object(capture(&mobject)?)
        };
        let pending = transaction.create_node(creation);
        if let Some(z) = family_z {
            transaction.set_z_index(pending, z);
        }
        copied.insert(id, pending);
    }
    for (parent, members) in edges {
        for member in members {
            transaction.add_member(copied[&parent], copied[&member]);
        }
    }
    for (source_family, graph) in graph_declarations {
        let vertices = graph.vertices().map(|(id, source)| (id, copied[&source]));
        let edges = graph.edges().map(|(edge, binding)| match binding.dependency() {
            SemanticGraphEdgeDependency::Line => SemanticTransactionGraphEdgeBinding::new(
                edge.id,
                copied[&binding.family()].into(),
                copied[&binding.line()].into(),
            ),
            SemanticGraphEdgeDependency::Arrow {
                end_tip,
                start_tip,
                policy,
            } => SemanticTransactionGraphEdgeBinding::new_arrow(
                edge.id,
                copied[&binding.family()].into(),
                copied[&binding.line()].into(),
                copied[&end_tip].into(),
                start_tip.map(|tip| copied[&tip].into()),
                policy,
            ),
        });
        transaction.set_graph_declaration(
            copied[&source_family],
            SemanticTransactionGraphDeclaration::new(graph.topology().clone(), vertices, edges),
        );
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
    pub fn copy_family(&self) -> Result<FamilyCopy, AuthoringError> {
        self.copy_with_references(&[])
    }

    /// Copy a family and additional detached references in one transaction.
    /// References may represent saved states or other host metadata. They are
    /// copied once, including aliases into the family, but never added as members.
    pub fn copy_with_references(
        &self,
        references: &[MobjectTarget<'_>],
    ) -> Result<FamilyCopy, AuthoringError> {
        let (transaction, pending) = prepare_family_copy(self, references, Mobject::state)?;
        let result = transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(AuthoringError::from)?;
        pending.resolve(&result)
    }
}

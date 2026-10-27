//! Local composed-source relowering through the ordinary prepared publication.
//! Plans borrow final semantic membership; Runtime validates their final physical
//! painter intervals together with all object/order changes before commit.
use super::*;
use noon_core::{ComposedSource, ObjectContentRef, Style, Transform2D};
use std::sync::Arc;

mod view;
use view::{old_scope, View};

pub(super) use view::unrendered_family;

pub(super) fn entry(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    reachability: &SemanticExecutionReachability,
    object: SemanticTransactionNodeRef,
) -> Result<PreparedEntry, SemanticPublicationLoweringError> {
    let mut view = View::new(prepared, reachability);
    let id = view.id(object)?;
    let style = prepared.family_style(object)?;
    let mut leaves = view
        .leaves(object)?
        .into_iter()
        .map(|member| {
            let state = prepared.object_state(member)?;
            if !prepared.draw_enabled(member)? {
                return Err(view.error(object, "source member is not drawable"));
            }
            super::super::projection::validate_effect_source_profile(
                view.id(member)?,
                state,
                prepared.store(),
            )?;
            Ok((member, prepared.z_index(member)?))
        })
        .collect::<Result<Vec<_>, SemanticPublicationLoweringError>>()?;
    // Stable ties keep authoritative family traversal, just like initial lowering.
    leaves.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("prepared finite z index"));
    let members = leaves
        .into_iter()
        .map(|(member, _)| view.id(member).map(semantic_execution_object_id))
        .collect::<Result<Vec<_>, _>>()?;
    let source = ComposedSource::new(members, view.parent_scope(object)?)
        .map_err(|_| view.error(object, "duplicate source member"))?;
    let mut compiled = CompiledObject::new(
        semantic_execution_object_id(id),
        ObjectContentRef::Composed(Arc::new(source)),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            opacity: super::super::projection::lower_scalar_f32(
                super::super::SemanticExecutionField::ObjectOpacity,
                style.object_opacity,
            )
            .map_err(|error| error.with_node(id))?,
            ..Style::default()
        },
    );
    compiled.draw_enabled = false;
    compiled.effects = effect::lower_prepared_attachment(prepared, object)?;
    Ok(PreparedEntry {
        object,
        compiled,
        inset_view: None,
        numeric_text: None,
    })
}

// Old and final ancestors are both needed for reparent/removal. Only their local
// adjacency is read; adding one member never clones an unrelated scene root.
fn ancestors(
    view: &mut View<'_, '_>,
    seed: SemanticTransactionNodeRef,
    candidates: &mut HashSet<SemanticTransactionNodeRef>,
) -> Result<(), SemanticPublicationLoweringError> {
    if let Some(seed) = seed.existing() {
        let mut pending = vec![seed];
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            if old_scope(view.prepared.store(), node) {
                candidates.insert(node.into());
            }
            if let Some(value) = view.prepared.store().node(node) {
                pending.extend(value.parents().iter().copied());
            }
        }
    }
    let mut pending = vec![seed];
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) || view.prepared.node_is_removed(node) {
            continue;
        }
        if view.scope(node)? {
            candidates.insert(node);
        }
        pending.extend(view.prepared.family_parents(node)?);
    }
    Ok(())
}

fn subtree(
    view: &mut View<'_, '_>,
    seed: SemanticTransactionNodeRef,
    candidates: &mut HashSet<SemanticTransactionNodeRef>,
) -> Result<(), SemanticPublicationLoweringError> {
    if let Some(seed) = seed.existing() {
        let mut pending = vec![seed];
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop() {
            if !seen.insert(node) {
                continue;
            }
            if old_scope(view.prepared.store(), node) {
                candidates.insert(node.into());
            }
            if let Some(value) = view.prepared.store().node(node) {
                pending.extend(value.members_iter());
            }
        }
    }
    let mut pending = vec![seed];
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) || view.prepared.node_is_removed(node) {
            continue;
        }
        if view.scope(node)? {
            candidates.insert(node);
        }
        pending.extend(view.prepared.family_members(node)?);
    }
    Ok(())
}

pub(super) fn prepare(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    reachability: &SemanticExecutionReachability,
    values: &mut ExecutionMutationTransaction,
    entry_refs: &mut Vec<SemanticTransactionNodeRef>,
    exits: &mut Vec<SemanticNodeId>,
) -> Result<(), SemanticPublicationLoweringError> {
    let mut view = View::new(prepared, reachability);
    let mut candidates = HashSet::new();
    for owner in prepared.family_appearance_scope_changes() {
        ancestors(&mut view, owner, &mut candidates)?;
        subtree(&mut view, owner, &mut candidates)?;
    }
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::AddMember { family, member }
            | SemanticMutation::RemoveMember { family, member } => {
                ancestors(&mut view, *family, &mut candidates)?;
                ancestors(&mut view, *member, &mut candidates)?;
                subtree(&mut view, *member, &mut candidates)?;
            }
            SemanticMutation::ReorderMember { family, .. }
            | SemanticMutation::ReplaceMemberContainer { family, .. } => {
                ancestors(&mut view, *family, &mut candidates)?;
            }
            SemanticMutation::SetZIndex { node, .. }
            | SemanticMutation::SetDrawEnabled { node, .. } => {
                ancestors(&mut view, *node, &mut candidates)?;
            }
            SemanticMutation::SetFamilyStyle { node, .. } => {
                let old = node
                    .existing()
                    .is_some_and(|id| old_scope(prepared.store(), id));
                let new = view.scope(*node)?;
                if old != new {
                    ancestors(&mut view, *node, &mut candidates)?;
                    subtree(&mut view, *node, &mut candidates)?;
                } else if new && view.reachable(*node)? {
                    let id = view.id(*node)?;
                    let opacity = super::super::projection::lower_scalar_f32(
                        super::super::SemanticExecutionField::ObjectOpacity,
                        prepared.family_style(*node)?.object_opacity,
                    )
                    .map_err(|error| error.with_node(id))?;
                    values.push(ExecutionPatch::SetStyle {
                        object: semantic_execution_object_id(id),
                        style: Style {
                            fill: None,
                            stroke: None,
                            opacity,
                            ..Style::default()
                        },
                        fill_paint: None,
                    });
                }
            }
            SemanticMutation::AddNode {
                creation: SemanticNodeCreation::Effect { owner, .. },
                ..
            } => {
                if !owner
                    .existing()
                    .is_some_and(|id| old_scope(prepared.store(), id))
                    && view.scope(*owner)?
                {
                    ancestors(&mut view, *owner, &mut candidates)?;
                    subtree(&mut view, *owner, &mut candidates)?;
                }
            }
            SemanticMutation::RemoveNode { node } => {
                if let Some(effect) = node
                    .existing()
                    .and_then(|id| prepared.store().node(id))
                    .and_then(|node| node.semantic_effect_state())
                {
                    let owner = effect.owner().into();
                    if old_scope(prepared.store(), effect.owner()) && !view.scope(owner)? {
                        ancestors(&mut view, owner, &mut candidates)?;
                        subtree(&mut view, owner, &mut candidates)?;
                    }
                } else {
                    ancestors(&mut view, *node, &mut candidates)?;
                    subtree(&mut view, *node, &mut candidates)?;
                }
            }
            _ => {}
        }
    }
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    // A changed physical member must remain capturable even when it has no
    // attachment of its own. Validate only that changed value and its ancestor
    // edges; ordinary transform/style edits do not rebuild any source plan.
    if prepared.store().has_composed_source_declarations() {
        for (node, state) in prepared.object_updates() {
            if prepared.node_is_removed(node) || !view.reachable(node.into())? {
                continue;
            }
            let mut scopes = HashSet::new();
            ancestors(&mut view, node.into(), &mut scopes)?;
            let mut captured = false;
            for scope in scopes {
                captured |= view.reachable(scope)? && view.scope(scope)?;
            }
            if captured {
                super::super::projection::validate_effect_source_profile(
                    node,
                    &state,
                    prepared.store(),
                )?;
            }
        }
    }
    candidates.sort_by_key(|node| view.id(*node).ok());
    for node in candidates {
        let live = view.reachable(node)? && view.scope(node)?;
        let before = node
            .existing()
            .is_some_and(|id| reachability.is_object_reachable(id));
        if !live {
            if before {
                exits.push(view.id(node)?);
            }
        } else if !before {
            entry_refs.push(node);
        } else {
            values.push(ExecutionPatch::SetContent {
                object: semantic_execution_object_id(view.id(node)?),
                content: entry(prepared, reachability, node)?.compiled.content,
                text_bounds: None,
            });
        }
    }
    // Use exact final reachability for conservative preflight too. A temporary
    // remove/add or a surviving ordinary alias must not retire a source input.
    let mut seen = HashSet::new();
    let mut exact_entries = Vec::new();
    for node in entry_refs.drain(..) {
        if seen.insert(node) && view.reachable(node)? {
            exact_entries.push(node);
        }
    }
    *entry_refs = exact_entries;
    let mut seen = HashSet::new();
    let mut exact_exits = Vec::new();
    for node in exits.drain(..) {
        if seen.insert(node)
            && (!view.reachable(node.into())?
                || !view.scope(node.into())? && prepared.object_state(node).is_err())
        {
            exact_exits.push(node);
        }
    }
    *exits = exact_exits;
    Ok(())
}

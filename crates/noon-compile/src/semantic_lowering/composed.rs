//! Derive capture inputs from rooted semantic families and actual painter order.
//! This plan has no authored membership, allocation domain, clock or value mirror.
use super::{SemanticExecutionContent, SemanticExecutionObject, SemanticLoweringError};
use crate::{CompiledEffect, CompiledEffectStack};
use noon_core::{
    ComposedSource, ObjectId, SemanticNodeId, SemanticNodeKind, SemanticPresentation,
    SemanticStore, Style, Transform2D,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(super) struct PreparedComposedSource {
    pub(super) owner: SemanticNodeId,
    source: Arc<ComposedSource>,
    effects: Option<Arc<CompiledEffectStack>>,
    opacity: f32,
    presentation: SemanticPresentation,
}

impl PreparedComposedSource {
    pub(super) fn into_execution_object(self, execution_id: ObjectId) -> SemanticExecutionObject {
        SemanticExecutionObject {
            semantic_id: self.owner,
            execution_id,
            content: SemanticExecutionContent::Composed(self.source),
            fill_paint: None,
            effects: self.effects,
            base_transform: Transform2D::IDENTITY,
            base_style: Style {
                fill: None,
                stroke: None,
                opacity: self.opacity,
                ..Style::default()
            },
            spatial: None,
            presentation: SemanticPresentation {
                draw_enabled: false,
                ..self.presentation
            },
            signal_bindings: Vec::new(),
            decimal_number: None,
        }
    }
}

fn error(owner: SemanticNodeId, reason: &'static str) -> SemanticLoweringError {
    SemanticLoweringError::InvalidEffectIsolation { owner, reason }
}

pub(super) fn prepare_initial(
    store: &SemanticStore,
    roots: &[SemanticNodeId],
    leaf_values: impl IntoIterator<Item = (SemanticNodeId, f64)>,
) -> Result<Vec<PreparedComposedSource>, SemanticLoweringError> {
    if !store.has_composed_source_declarations() {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for &root in roots {
        for owner in store.ordered_authoring_nodes(root)? {
            if !seen.insert(owner) {
                continue;
            }
            let node = store.node(owner).expect("validated traversal");
            if matches!(node.kind(), SemanticNodeKind::Family(_))
                && node.semantic_object_state().is_none()
                && (!node.effect_ids().is_empty()
                    || node.family_style().expect("family style").object_opacity != 1.0
                    || store.has_family_appearance_target(owner))
            {
                candidates.push(owner);
            }
        }
    }
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut painter = leaf_values.into_iter().collect::<Vec<_>>();
    painter.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("validated finite priority"));
    let ranks = painter
        .iter()
        .enumerate()
        .map(|(rank, (id, _))| (*id, rank))
        .collect::<HashMap<_, _>>();
    let mut prepared = Vec::new();
    let mut intervals = Vec::new();
    let candidate_set = candidates.iter().copied().collect::<HashSet<_>>();
    let root_set = roots.iter().copied().collect::<HashSet<_>>();
    for owner in candidates {
        let node = store.node(owner).expect("validated family");
        for descendant in store.ordered_authoring_nodes(owner)? {
            let parents = store
                .node(descendant)
                .expect("validated descendant")
                .parents()
                .iter()
                .filter(|parent| seen.contains(parent))
                .count();
            if parents > 1 || (parents > 0 && root_set.contains(&descendant)) {
                return Err(error(owner, "aliased family membership"));
            }
        }
        let mut members = store.ordered_leaf_nodes(owner)?;
        if members.is_empty() {
            return Err(error(owner, "empty source"));
        }
        for &member in &members {
            if !ranks.contains_key(&member) {
                return Err(error(owner, "member is absent from rooted execution"));
            }
            let state = store.semantic_object_state_checked(member)?;
            super::projection::validate_effect_source_profile(member, state, store)?;
        }
        // Sort by the already-authoritative painter ranks, never by a new group
        // order. A gap proves external interleaving and is rejected below.
        members.sort_unstable_by_key(|member| ranks[member]);
        let start = ranks[&members[0]];
        let end = ranks[members.last().unwrap()] + 1;
        if end - start != members.len() {
            return Err(error(
                owner,
                "external painter content interleaves the source",
            ));
        }
        let mut ancestor = node
            .parents()
            .iter()
            .copied()
            .find(|parent| seen.contains(parent));
        while let Some(parent) = ancestor.filter(|parent| !candidate_set.contains(parent)) {
            ancestor = store
                .node(parent)
                .expect("validated ancestor")
                .parents()
                .iter()
                .copied()
                .find(|parent| seen.contains(parent));
        }
        let source = ComposedSource::new(
            members.into_iter().map(super::semantic_execution_object_id),
            ancestor.map(super::semantic_execution_object_id),
        )
        .map_err(|_| error(owner, "duplicate source member"))?;
        let effects = store.animation_effect_snapshot(owner)?;
        let effects = if effects.is_empty() {
            None
        } else {
            Some(Arc::new(
                CompiledEffectStack::new_with_visibility(
                    effects.iter().map(|effect| CompiledEffect {
                        attachment: effect.attachment,
                        definition: effect.definition,
                    }),
                    effects
                        .iter()
                        .find(|effect| effect.name.is_visibility())
                        .map(|effect| effect.attachment),
                )
                .map_err(|_| SemanticLoweringError::UnsupportedEffectProfile { owner })?,
            ))
        };
        let opacity = super::projection::lower_scalar_f32(
            super::SemanticExecutionField::ObjectOpacity,
            node.family_style().expect("family style").object_opacity,
        )
        .map_err(|error| error.with_node(owner))?;
        intervals.push((start, end, owner));
        prepared.push(PreparedComposedSource {
            owner,
            source: Arc::new(source),
            effects,
            opacity,
            presentation: node.presentation().expect("family presentation"),
        });
    }
    // A stack of actual painter intervals proves laminar containment. This is
    // initial lowering work; later parameter edits do not rebuild these inputs.
    intervals.sort_unstable_by_key(|&(start, end, owner)| (start, std::cmp::Reverse(end), owner));
    let mut parents = Vec::<(usize, usize)>::new();
    for (start, end, owner) in intervals {
        while parents
            .last()
            .is_some_and(|&(_, parent_end)| start >= parent_end)
        {
            parents.pop();
        }
        if let Some(&(_, parent_end)) = parents.last() {
            if end > parent_end {
                return Err(error(owner, "effect scopes cross in painter order"));
            }
        }
        parents.push((start, end));
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests;

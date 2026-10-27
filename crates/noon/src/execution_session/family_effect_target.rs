//! Family targets stage ordinary effect-only semantic leaves alongside their
//! existing geometric animation, sharing one schedule and completion barrier.
use super::*;

fn error(message: impl std::fmt::Display) -> ExecutionSessionAnimationError {
    ExecutionSessionAnimationError::InvalidComposition(message.to_string())
}

fn has_effects(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, ExecutionSessionAnimationError> {
    if !store.has_effect_attachments() {
        return Ok(false);
    }
    for root in [source, target] {
        if store
            .ordered_authoring_nodes(root)
            .map_err(error)?
            .into_iter()
            .any(|node| {
                !store
                    .node(node)
                    .expect("validated family traversal")
                    .effect_ids()
                    .is_empty()
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn reject_matching(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<(), ExecutionSessionAnimationError> {
    if has_effects(store, source, target)? {
        return Err(error("matching effect-bearing family targets require composed-scope animation lowering qualification (#1897)"));
    }
    Ok(())
}

pub(super) fn validate(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, ExecutionSessionAnimationError> {
    let effects = has_effects(store, source, target)?;
    if effects {
        store
            .ordered_family_root_pairs(source, target)
            .map_err(error)?;
        store
            .ordered_family_leaf_pairs(source, target)
            .map_err(error)?;
    }
    Ok(effects)
}

pub(super) fn has_scope_effects(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, ExecutionSessionAnimationError> {
    if !has_effects(store, source, target)? {
        return Ok(false);
    }
    Ok(store
        .ordered_family_root_pairs(source, target)
        .map_err(error)?
        .into_iter()
        .any(|(source, target)| {
            [source, target].into_iter().any(|owner| {
                let node = store.node(owner).expect("validated root");
                node.semantic_object_state().is_none() && !node.effect_ids().is_empty()
            })
        }))
}

/// Resolve public play overrides before inserting an internal parallel fanout.
/// A scope's parameters share the family's root clock; member lag remains owned
/// by the geometric expansion and never staggers the parameter columns.
pub(super) fn root_options(
    store: &SemanticStore,
    request: &SemanticCompositionRequest,
    play: AnimationOptions,
) -> Result<Option<SemanticCompositionRequest>, ExecutionSessionAnimationError> {
    let (SemanticCompositionRequest::FamilyTransformTo {
        source,
        target_state,
        ..
    }
    | SemanticCompositionRequest::FamilyMethodTargetTo {
        source,
        target_state,
        ..
    }) = request
    else {
        return Ok(None);
    };
    if !has_scope_effects(store, *source, *target_state)? {
        return Ok(None);
    }
    let mut adjusted = request.clone();
    match &mut adjusted {
        SemanticCompositionRequest::FamilyTransformTo { options, .. }
        | SemanticCompositionRequest::FamilyMethodTargetTo { options, .. } => {
            *options = options.with_overrides(play)
        }
        _ => unreachable!("checked family target"),
    }
    Ok(Some(adjusted))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compose(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
    method_target: bool,
    geometry: noon_core::SemanticLocalNodeToken,
    options: AnimationOptions,
    declaration: &mut SemanticMutationTransaction,
) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
    if !has_effects(store, source, target)? {
        return Ok(geometry);
    }
    let pairs = store
        .ordered_family_root_pairs(source, target)
        .map_err(error)?;
    let mut children = vec![geometry];
    let effect_options = AnimationOptions::new().rate_func(RateFunction::Linear);
    let mut seen = std::collections::HashSet::new();
    for (owner, target) in pairs {
        if !seen.insert(owner) {
            continue;
        }
        if store
            .node(owner)
            .expect("validated source")
            .effect_ids()
            .is_empty()
            && store
                .node(target)
                .expect("validated target")
                .effect_ids()
                .is_empty()
        {
            continue;
        }
        // Rendered roots already own effects in the nested per-leaf expansion.
        if store
            .node(owner)
            .expect("validated source")
            .semantic_object_state()
            .is_some()
        {
            continue;
        }
        children.push(declaration.create_effect_target_animation(
            owner,
            target,
            method_target,
            effect_options,
        ));
    }
    if children.len() == 1 {
        return Ok(geometry);
    }
    let mut root_options = options;
    root_options.rate_func = Some(options.rate_func.unwrap_or(RateFunction::Smooth));
    root_options.path_arc = None;
    root_options.lag_ratio = None;
    Ok(declaration.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        children,
        root_options,
    ))
}

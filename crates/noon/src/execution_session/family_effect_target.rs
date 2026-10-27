//! #1897: remove this admission gap once family-owned effect targets lower to
//! the ordinary generation-checked effect channels. Never silently ignore them.
use super::*;

pub(super) fn validate(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
    method_target: bool,
) -> Result<(), ExecutionSessionAnimationError> {
    if !store.has_effect_attachments() {
        return Ok(());
    }
    let scope_has_effects = |id| {
        store.node(id).is_some_and(|node| {
            matches!(node.kind(), noon_core::SemanticNodeKind::Family(_))
                && node.semantic_object_state().is_none()
                && !node.effect_ids().is_empty()
        })
    };
    let mut has_scopes = false;
    for root in [source, target] {
        let nodes = store.ordered_authoring_nodes(root).map_err(|error| {
            ExecutionSessionAnimationError::InvalidComposition(error.to_string())
        })?;
        has_scopes |= nodes.into_iter().any(scope_has_effects);
    }
    if !has_scopes {
        return Ok(());
    }
    let unsupported = || {
        ExecutionSessionAnimationError::InvalidComposition(
        "family effect parameter targets require composed-scope animation lowering (#1897); use live setters between segments until qualified".into()
    )
    };
    let pairs = store
        .ordered_family_root_pairs(source, target)
        .map_err(|_| unsupported())?;
    for (source, target) in pairs {
        if !scope_has_effects(source) && !scope_has_effects(target) {
            continue;
        }
        let source = store
            .animation_effect_snapshot(source)
            .map_err(|_| unsupported())?;
        let target = store
            .animation_effect_snapshot(target)
            .map_err(|_| unsupported())?;
        if !method_target || source.len() != target.len() {
            return Err(unsupported());
        }
        for (source, target) in source.iter().zip(&target) {
            let unchanged_copy = target.copy_provenance.is_some_and(|copy| {
                copy.source_attachment() == source.attachment
                    && target.name == source.name
                    && target.definition == copy.copied_definition()
                    && [
                        noon_core::EffectParameter::Color,
                        noon_core::EffectParameter::Radius,
                        noon_core::EffectParameter::Intensity,
                        noon_core::EffectParameter::Offset,
                    ]
                    .into_iter()
                    .all(|parameter| !copy.writes_parameter(parameter))
            });
            if !unchanged_copy {
                return Err(unsupported());
            }
        }
    }
    Ok(())
}

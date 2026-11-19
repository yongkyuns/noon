//! Family targets stage ordinary appearance semantic leaves alongside their
//! existing geometric animation, sharing one schedule and completion barrier.
use super::*;

fn error(message: impl std::fmt::Display) -> ExecutionSessionAnimationError {
    ExecutionSessionAnimationError::InvalidComposition(message.to_string())
}

/// Stage only the family's appearance endpoint, preserving its original source plan.
pub(super) fn stage_owned_opacity(
    execution: &ExecutionSession,
    store: &SemanticStore,
    owner: SemanticNodeId,
    opacity: f64,
    options: AnimationOptions,
    declaration: &mut SemanticMutationTransaction,
) -> Result<noon_core::SemanticLocalNodeToken, ExecutionSessionAnimationError> {
    let source = store
        .node(owner)
        .filter(|node| node.semantic_object_state().is_none())
        .and_then(|node| node.family_style())
        .ok_or_else(|| error("composite opacity animation requires an unrendered group"))?;
    let mut style = source.clone();
    crate::semantic_mobject::edit_object_opacity(&mut style, opacity).map_err(error)?;
    let target = declaration.create_node(SemanticNodeCreation::family());
    declaration.set_copied_family_opacity_target(target, owner, style);
    crate::effective_capture::stage_effect_copy(store, execution, owner, target, declaration)
        .map_err(error)?;
    Ok(declaration.create_effect_target_animation(owner, target, true, options))
}

pub(super) fn is_opacity_copy(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> bool {
    store
        .node(target)
        .and_then(noon_core::SemanticNode::family_opacity_copy_provenance)
        .is_some_and(|copy| copy.source() == source && copy.writes_object_opacity())
}

/// Python's method builder already has a copied endpoint. Admit a root fade
/// through the same appearance track as Rust, rejecting edits that track cannot carry.
pub(super) fn validate_entering_opacity(
    store: &SemanticStore,
    owner: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<(), ExecutionSessionAnimationError> {
    let unsupported =
        || error("detached family animate uses a separate request for composite opacity");
    if !crate::animation_request::is_method_family(
        store,
        target,
        MethodFamilyAppearance::RootOpacity,
    ) {
        return Err(unsupported());
    }
    for (source, target) in store
        .ordered_family_root_pairs(owner, target)
        .map_err(error)?
    {
        require_preserved_effects(store, source, target)?;
        let original = store.node(source).expect("validated family pair");
        let copied = store.node(target).expect("validated family pair");
        let mut style = original
            .family_style()
            .expect("validated family style")
            .clone();
        let target_style = copied.family_style().expect("validated family style");
        if source == owner {
            style.object_opacity = target_style.object_opacity;
        }
        let original_paint = original
            .presentation()
            .expect("validated family presentation");
        let copied_paint = copied
            .presentation()
            .expect("validated family presentation");
        if style != *target_style
            || original_paint.draw_enabled != copied_paint.draw_enabled
            || original_paint.z_index != copied_paint.z_index
            || original.family_fallback_color() != copied.family_fallback_color()
            || original.family_background_stroke_color() != copied.family_background_stroke_color()
            || original.family_background_stroke_width() != copied.family_background_stroke_width()
            || original.family_background_stroke_opacity()
                != copied.family_background_stroke_opacity()
            || original.sheen_direction() != copied.sheen_direction()
        {
            return Err(unsupported());
        }
    }
    for (source, target) in store
        .ordered_family_leaf_pairs(owner, target)
        .map_err(error)?
    {
        let original = store.semantic_object_state_checked(source).map_err(error)?;
        let copied = store.semantic_object_state_checked(target).map_err(error)?;
        if !copied.matches_authored_state_of(original) {
            return Err(unsupported());
        }
    }
    Ok(())
}

pub(super) fn has_effects(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<bool, ExecutionSessionAnimationError> {
    if !store.has_composed_source_declarations() {
        return Ok(false);
    }
    let source_nodes = store.ordered_authoring_nodes(source).map_err(error)?;
    let source_ids = source_nodes
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    for (nodes, destination) in [
        (source_nodes, false),
        (store.ordered_authoring_nodes(target).map_err(error)?, true),
    ] {
        if nodes.into_iter().any(|node| {
            let node = store.node(node).expect("validated family traversal");
            !node.effect_ids().is_empty()
                || node.semantic_object_state().is_none()
                    && (node
                        .family_style()
                        .is_some_and(|style| style.object_opacity != 1.0)
                        || !destination && store.has_family_appearance_target(node.id())
                        || destination
                            && node.family_opacity_copy_provenance().is_some_and(|copy| {
                                source_ids.contains(&copy.source()) && copy.writes_object_opacity()
                            }))
        }) {
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
        return Err(error("matching appearance-bearing family targets require composed-scope animation lowering qualification (#1897)"));
    }
    Ok(())
}

pub(super) fn validate(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
    entering: bool,
) -> Result<(), ExecutionSessionAnimationError> {
    let pairs = store
        .ordered_family_root_pairs(source, target)
        .map_err(error)?;
    if !entering {
        store
            .ordered_family_leaf_pairs(source, target)
            .map_err(error)?;
        return Ok(());
    }
    // The entering geometry uses the original scopes without appearance tracks.
    for (source, target) in pairs {
        require_preserved_effects(store, source, target)?;
    }
    Ok(())
}

fn require_preserved_effects(
    store: &SemanticStore,
    source: SemanticNodeId,
    target: SemanticNodeId,
) -> Result<(), ExecutionSessionAnimationError> {
    let source = store.animation_effect_snapshot(source).map_err(error)?;
    let target = store.animation_effect_snapshot(target).map_err(error)?;
    if source
        .iter()
        .map(|item| (&item.name, &item.definition))
        .ne(target.iter().map(|item| (&item.name, &item.definition)))
    {
        return Err(error(
            "detached family animate must preserve its composed effects",
        ));
    }
    Ok(())
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
            [(source, false), (target, true)]
                .into_iter()
                .any(|(owner, destination)| {
                    let node = store.node(owner).expect("validated root");
                    node.semantic_object_state().is_none()
                        && (!node.effect_ids().is_empty()
                            || node
                                .family_style()
                                .is_some_and(|style| style.object_opacity != 1.0)
                            || !destination && store.has_family_appearance_target(owner)
                            || destination
                                && node.family_opacity_copy_provenance().is_some_and(|copy| {
                                    copy.source() == source && copy.writes_object_opacity()
                                }))
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
    if !matches!(
        request,
        SemanticCompositionRequest::FamilyMethodTargetTo { .. }
    ) && !has_scope_effects(store, *source, *target_state)?
    {
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
    // This is a derived fanout, not an authored AnimationGroup. Keep the
    // family's easing on its actual parameter leaf so returning rates retain
    // the ordinary terminal sample and completion policy.
    let effect_options = AnimationOptions::new()
        .run_time(1.0)
        .rate_func(options.rate_func.unwrap_or(RateFunction::Smooth));
    let mut seen = std::collections::HashSet::new();
    for (owner, target) in pairs {
        if !seen.insert(owner) {
            continue;
        }
        if [(owner, false), (target, true)]
            .into_iter()
            .all(|(id, destination)| {
                let node = store.node(id).expect("validated family pair");
                node.effect_ids().is_empty()
                    && node
                        .family_style()
                        .is_none_or(|style| style.object_opacity == 1.0)
                    && (destination || !store.has_family_appearance_target(id))
                    && (!destination
                        || node.family_opacity_copy_provenance().is_none_or(|copy| {
                            copy.source() != owner || !copy.writes_object_opacity()
                        }))
            })
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
    root_options.rate_func = Some(RateFunction::Linear);
    root_options.path_arc = None;
    root_options.lag_ratio = None;
    Ok(declaration.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        children,
        root_options,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_identity_scope_full_target_captures_current_effective_opacity() {
        let mut scene = crate::Scene::new();
        let leaf = scene.circle(0.4).unwrap();
        let family = scene.family(&[(&leaf).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let snapshot = family.copy_family().unwrap();
        // A real replayable appearance declaration requires the identity scope,
        // independently from its authored opacity or attachment column.
        let _declaration = scene
            .declare_animation(
                noon_core::SemanticAnimationIntent::EffectTargetTo {
                    target: family.node_id(),
                    target_state: snapshot.root().node_id(),
                    method_target: false,
                },
                AnimationOptions::new().run_time(1.0),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let object = session.execution_object_id(family.node_id()).unwrap();
        let id = session.next_activation_track_id.unwrap();
        // Install an ordinary typed runtime timeline, as other activation tests
        // do. The authored root remains one while the effective endpoint is .5.
        session
            .runtime
            .apply_execution_transaction(
                &noon_compile::ExecutionMutationTransaction::from_mutations(vec![
                    noon_compile::ExecutionPatch::AddTrack(noon_core::TrackDefinition {
                        id: noon_core::TrackId::new(id),
                        object,
                        property: noon_core::Property::Opacity,
                        values: noon_core::TrackValues::Scalar { from: 1.0, to: 0.5 },
                        timing: noon_core::TrackTiming::new(0.0, 1.0, RateFunction::Linear),
                        time_map: noon_core::CompositionTimeMap::identity(),
                    }),
                ]),
            )
            .unwrap();
        session.next_activation_track_id = id.checked_add(1);
        session.seek(1.0).unwrap();
        assert_eq!(
            session
                .runtime
                .effective_object(object)
                .unwrap()
                .style
                .opacity,
            0.5
        );
        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(
                &crate::AnimationCompositionRequest::FamilyTransformTo {
                    source: &family,
                    target_state: snapshot.root(),
                    options: AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                },
                AnimationOptions::new(),
            )
            .unwrap();
        session.advance_segment_to(segment, 1.5).unwrap();
        assert_eq!(
            session
                .runtime
                .effective_object(object)
                .unwrap()
                .style
                .opacity,
            0.75
        );
        assert_eq!(family.own_style().unwrap().object_opacity, 1.0);
    }
}

use noon_compile::{
    PreparedDerivedFamilyTransformOccurrence, PreparedFamilyTransformChannelProjection,
    PreparedMatchingFamilyTransformPayload, PreparedMatchingShapeSourceMember,
    PreparedMatchingShapeTargetLeftoverFade, PreparedTransientPainterPlacement,
};
use noon_core::{SemanticNodeId, SemanticStore};
use noon_runtime::{
    DerivedDisplayAnimationOccurrence, DerivedDisplayAnimationPlan, DerivedDisplayAnimationTrack,
    DerivedDisplayObjectState, SceneInstance, TransientPresentationPainterPlacement,
};

/// Convert compiler-owned family-Transform padding channels into one runtime-only
/// display plan while the activation frame is still authoritative.
///
/// Stable execution identity is used only to locate/copy the real source row. The
/// resulting plan carries painter anchors and occurrence ordinals, never a synthetic
/// semantic or execution object identity.
pub(super) fn build_derived_family_transform_plan(
    runtime: &SceneInstance,
    projection: &PreparedFamilyTransformChannelProjection,
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    let occurrences =
        materialize_derived_family_occurrences(runtime, projection.derived_occurrences())?;
    build_transient_plan(occurrences)
}

fn materialize_derived_family_occurrences(
    runtime: &SceneInstance,
    prepared_occurrences: &[PreparedDerivedFamilyTransformOccurrence],
) -> Result<Vec<DerivedDisplayAnimationOccurrence>, String> {
    let frame = runtime.frame();
    let mut occurrences = Vec::with_capacity(prepared_occurrences.len());
    for occurrence in prepared_occurrences {
        let anchor_object_index = runtime
            .frame_index_for_object(occurrence.anchor_execution_object_id)
            .ok_or_else(|| {
                format!(
                    "derived family Transform occurrence {} has no live source execution row {}",
                    occurrence.occurrence_index,
                    occurrence.anchor_execution_object_id.get()
                )
            })?;
        let row = frame.objects.get(anchor_object_index).ok_or_else(|| {
            format!(
                "derived family Transform occurrence {} source frame row {} is missing",
                occurrence.occurrence_index, anchor_object_index
            )
        })?;
        let base = DerivedDisplayObjectState {
            z_index: row.z_index,
            content: row.content.clone(),
            text_bounds: row.text_bounds,
            transform: row.transform,
            style: row.style,
            appearance: row.appearance,
            presence: *frame.presences.get(anchor_object_index).ok_or_else(|| {
                format!(
                    "derived family Transform occurrence {} source presence row is missing",
                    occurrence.occurrence_index
                )
            })?,
            reveal: *frame.reveals.get(anchor_object_index).ok_or_else(|| {
                format!(
                    "derived family Transform occurrence {} source reveal row is missing",
                    occurrence.occurrence_index
                )
            })?,
            morph: *frame.morphs.get(anchor_object_index).ok_or_else(|| {
                format!(
                    "derived family Transform occurrence {} source morph row is missing",
                    occurrence.occurrence_index
                )
            })?,
            render_geometry: frame
                .render_geometries
                .get(anchor_object_index)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "derived family Transform occurrence {} source render-geometry row is missing",
                        occurrence.occurrence_index
                    )
                })?,
            render_transform: *frame
                .render_transforms
                .get(anchor_object_index)
                .ok_or_else(|| {
                    format!(
                        "derived family Transform occurrence {} source render-transform row is missing",
                        occurrence.occurrence_index
                    )
                })?,
        };
        let tracks = occurrence
            .tracks
            .iter()
            .map(|track| DerivedDisplayAnimationTrack {
                property: track.property,
                values: track.values.clone(),
                timing: track.timing,
                time_map: track.time_map.clone(),
                transform_geometry_plan: track.transform_geometry_plan.clone(),
            })
            .collect();
        occurrences.push(DerivedDisplayAnimationOccurrence {
            painter_placement: TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index: u32::try_from(anchor_object_index).map_err(|_| {
                    format!(
                        "derived family Transform occurrence {} source row exceeds u32 painter indexing",
                        occurrence.occurrence_index
                    )
                })?,
            },
            occurrence_index: occurrence.occurrence_index,
            base,
            tracks,
        });
    }
    Ok(occurrences)
}

/// Materialize detached matching-shape target leftovers into the existing
/// identity-free transient renderer path.
///
/// Compiler `LayerEnd` intent is resolved against the activation frame to the last
/// *real* stable row in the same z layer. That row is painter placement provenance
/// only; it does not become semantic or execution identity for the detached target.
/// A layer with no stable row fails closed rather than fabricating an anchor or
/// silently changing z ordering.
#[cfg(test)]
pub(super) fn build_matching_shape_target_leftover_plan(
    runtime: &SceneInstance,
    target_fades: &[PreparedMatchingShapeTargetLeftoverFade],
    occurrence_index_start: u32,
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    let occurrences =
        materialize_matching_shape_target_leftovers(target_fades, occurrence_index_start, |z| {
            stable_layer_tail(runtime, z)
        })?;
    build_transient_plan(occurrences)
}

/// Materialize every identity-free occurrence required by one complete matching-family
/// payload into the session's single existing transient animation plan.
///
/// Matched duplicate padding copies keep their real source painter anchors. Detached
/// target leftovers resolve `LayerEnd` independently. The compiler reserves the target
/// occurrence range after all matched derived occurrences, so concatenation cannot
/// invent or arbitrate another occurrence identity space.
pub(super) fn build_matching_family_transform_plan(
    runtime: &SceneInstance,
    payload: &PreparedMatchingFamilyTransformPayload,
    moved_sources: &[PreparedMatchingShapeSourceMember],
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    // The declaration moves the source family behind surviving roots. Resolve
    // LayerEnd against that proposed order before committing, not the old frame's
    // tail. Only affected source members are visited; stable row IDs do not change.
    let mut moved_layer_tails = std::collections::HashMap::new();
    for source in moved_sources {
        let index = runtime
            .frame_index_for_object(source.execution_object_id)
            .ok_or_else(|| "matching source has no stable painter row".to_owned())?;
        if runtime.frame().is_present(index) {
            moved_layer_tails.insert(
                layer_key(source.effective.z_index),
                u32::try_from(index)
                    .map_err(|_| "matching source row exceeds u32 painter indexing".to_owned())?,
            );
        }
    }
    build_matching_family_transform_plan_from_parts(
        runtime,
        payload.derived_occurrences(),
        payload.target_leftovers(),
        payload.target_occurrence_index_start(),
        |z| {
            moved_layer_tails
                .get(&layer_key(z))
                .copied()
                .or_else(|| stable_layer_tail(runtime, z))
        },
    )
}

fn layer_key(z: f64) -> u64 {
    // Painter layers compare signed zero as equal.
    if z == 0.0 {
        0
    } else {
        z.to_bits()
    }
}

fn build_matching_family_transform_plan_from_parts(
    runtime: &SceneInstance,
    derived: &[PreparedDerivedFamilyTransformOccurrence],
    target_fades: &[PreparedMatchingShapeTargetLeftoverFade],
    target_occurrence_index_start: u32,
    layer_tail: impl FnMut(f64) -> Option<u32>,
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    let mut occurrences = materialize_derived_family_occurrences(runtime, derived)?;
    occurrences.extend(materialize_matching_shape_target_leftovers(
        target_fades,
        target_occurrence_index_start,
        layer_tail,
    )?);
    build_transient_plan(occurrences)
}

fn materialize_matching_shape_target_leftovers(
    target_fades: &[PreparedMatchingShapeTargetLeftoverFade],
    occurrence_index_start: u32,
    mut layer_tail: impl FnMut(f64) -> Option<u32>,
) -> Result<Vec<DerivedDisplayAnimationOccurrence>, String> {
    let mut occurrences = Vec::with_capacity(target_fades.len());
    for (ordinal, fade) in target_fades.iter().enumerate() {
        if fade.base.painter_placement() != PreparedTransientPainterPlacement::LayerEnd {
            return Err(format!(
                "matching-shape target leftover {} unexpectedly requests anchored painter placement",
                fade.target_index
            ));
        }
        let anchor_object_index = layer_tail(fade.base.z_index).ok_or_else(|| {
            format!(
                "matching-shape target leftover {} has no stable painter row in z layer {}",
                fade.target_index, fade.base.z_index
            )
        })?;
        let ordinal = u32::try_from(ordinal).map_err(|_| {
            format!(
                "matching-shape target leftover count exceeds u32 occurrence indexing at {}",
                fade.target_index
            )
        })?;
        let occurrence_index = occurrence_index_start.checked_add(ordinal).ok_or_else(|| {
            format!(
                "matching-shape target leftover occurrence index overflows at {}",
                fade.target_index
            )
        })?;
        let tracks = fade
            .tracks
            .iter()
            .map(|track| DerivedDisplayAnimationTrack {
                property: track.property,
                values: track.values.clone(),
                timing: track.timing,
                time_map: track.time_map.clone(),
                transform_geometry_plan: None,
            })
            .collect();
        occurrences.push(DerivedDisplayAnimationOccurrence {
            painter_placement: TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index,
            },
            occurrence_index,
            base: DerivedDisplayObjectState {
                z_index: fade.base.z_index,
                content: fade.base.content.clone(),
                text_bounds: None,
                transform: fade.base.transform,
                style: fade.base.style,
                appearance: fade.base.appearance,
                presence: true,
                reveal: fade.base.reveal,
                morph: fade.base.morph,
                render_geometry: None,
                render_transform: None,
            },
            tracks,
        });
    }
    Ok(occurrences)
}

fn build_transient_plan(
    occurrences: Vec<DerivedDisplayAnimationOccurrence>,
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    if occurrences.is_empty() {
        return Ok(None);
    }
    DerivedDisplayAnimationPlan::new(occurrences)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn stable_layer_tail(runtime: &SceneInstance, z_index: f64) -> Option<u32> {
    let frame = runtime.frame();
    runtime
        .painter_order()
        .iter()
        .copied()
        .rev()
        .find(|&index| {
            let index = index as usize;
            frame.is_present(index)
                && frame
                    .objects
                    .get(index)
                    .is_some_and(|object| object.z_index == z_index)
        })
}

/// Unsupported topology for exact-end matching-family replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MatchingFamilyCompletionSwapError {
    InvalidExecutionRoot(SemanticNodeId),
    InvalidSourceFamily(SemanticNodeId),
    InvalidTargetFamily(SemanticNodeId),
    SourceEqualsTarget(SemanticNodeId),
    SourceNotDirectMember {
        execution_root: SemanticNodeId,
        source: SemanticNodeId,
    },
    TargetNotDetached(SemanticNodeId),
    AliasedSourceDescendant(SemanticNodeId),
    AliasedTargetDescendant(SemanticNodeId),
}

impl std::fmt::Display for MatchingFamilyCompletionSwapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "unsupported matching-family completion swap topology: {self:?}"
        )
    }
}

impl std::error::Error for MatchingFamilyCompletionSwapError {}

/// Validate the exact-end source-family -> target-family replacement topology.
///
/// The source must be one direct member of `execution_root` with no shared
/// descendants; the authored target must be detached. Shared descendants require
/// membership restructuring before their active presentation can move to the tail,
/// so this bounded path rejects them before staging any edit. Both families retain
/// their internal authored topology and no execution identity is manufactured. The
/// membership and foreground-aware ordering are staged separately with all other
/// removals at the same completion boundary.
pub(super) fn validate_matching_family_completion_swap(
    store: &SemanticStore,
    execution_root: SemanticNodeId,
    source_root: SemanticNodeId,
    target_root: SemanticNodeId,
) -> Result<(), MatchingFamilyCompletionSwapError> {
    store
        .node(execution_root)
        .filter(|node| matches!(node.kind(), noon_core::SemanticNodeKind::Family(_)))
        .ok_or(MatchingFamilyCompletionSwapError::InvalidExecutionRoot(
            execution_root,
        ))?;
    let source = store
        .node(source_root)
        .filter(|node| matches!(node.kind(), noon_core::SemanticNodeKind::Family(_)))
        .ok_or(MatchingFamilyCompletionSwapError::InvalidSourceFamily(
            source_root,
        ))?;
    let target = store
        .node(target_root)
        .filter(|node| matches!(node.kind(), noon_core::SemanticNodeKind::Family(_)))
        .ok_or(MatchingFamilyCompletionSwapError::InvalidTargetFamily(
            target_root,
        ))?;

    if source_root == target_root {
        return Err(MatchingFamilyCompletionSwapError::SourceEqualsTarget(
            source_root,
        ));
    }
    if source_root == execution_root || source.parents() != [execution_root] {
        return Err(MatchingFamilyCompletionSwapError::SourceNotDirectMember {
            execution_root,
            source: source_root,
        });
    }
    if target.is_scene_owned() || !target.parents().is_empty() {
        return Err(MatchingFamilyCompletionSwapError::TargetNotDetached(
            target_root,
        ));
    }

    // Root-only admission is insufficient: an earlier surviving family may own
    // the first occurrence of a source leaf or nested family. Moving just the
    // source root would leave that row behind and invalidate LayerEnd anchors.
    // Inspect only this source subtree and its maintained parent links.
    for descendant in store
        .ordered_authoring_nodes(source_root)
        .map_err(|_| MatchingFamilyCompletionSwapError::InvalidSourceFamily(source_root))?
    {
        if descendant != source_root
            && store
                .node(descendant)
                .is_none_or(|node| node.parents().len() != 1)
        {
            return Err(MatchingFamilyCompletionSwapError::AliasedSourceDescendant(
                descendant,
            ));
        }
    }

    // A detached target root can still alias a live leaf through another family.
    // Such a target is not a detached replacement and would change existing
    // presentation ownership when admitted.
    for descendant in store
        .ordered_authoring_nodes(target_root)
        .map_err(|_| MatchingFamilyCompletionSwapError::InvalidTargetFamily(target_root))?
    {
        if descendant != target_root
            && store
                .node(descendant)
                .is_none_or(|node| node.parents().len() != 1)
        {
            return Err(MatchingFamilyCompletionSwapError::AliasedTargetDescendant(
                descendant,
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod matching_completion_tests {
    use noon_core::{SemanticMutationTransaction, SemanticObjectState, StoredGeometry};

    use super::*;

    fn stage_matching_family_completion_swap(
        store: &SemanticStore,
        root: SemanticNodeId,
        source: SemanticNodeId,
        target: SemanticNodeId,
        transaction: &mut SemanticMutationTransaction,
    ) -> Result<(), MatchingFamilyCompletionSwapError> {
        validate_matching_family_completion_swap(store, root, source, target)?;
        noon_core::stage_semantic_scene_lifecycle_membership(
            store,
            root,
            &[source],
            &[target],
            transaction,
        )
        .unwrap();
        Ok(())
    }

    fn object(store: &mut SemanticStore) -> SemanticNodeId {
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 1.0,
        }))
    }

    fn family(store: &mut SemanticStore, members: &[SemanticNodeId]) -> SemanticNodeId {
        let family = store.insert_family();
        for &member in members {
            store.add_member(family, member).unwrap();
        }
        family
    }

    #[test]
    fn completion_swap_appends_authored_target_and_preserves_family_internals() {
        let mut store = SemanticStore::new();
        let before = object(&mut store);
        let after = object(&mut store);
        let source_leaf = object(&mut store);
        let target_leaf = object(&mut store);
        let source = family(&mut store, &[source_leaf]);
        let target = family(&mut store, &[target_leaf]);
        let execution_root = family(&mut store, &[before, source, after]);
        let source_members = store
            .semantic_family_members_checked(source)
            .unwrap()
            .to_vec();
        let target_members = store
            .semantic_family_members_checked(target)
            .unwrap()
            .to_vec();

        let mut semantic = SemanticMutationTransaction::new();
        stage_matching_family_completion_swap(
            &store,
            execution_root,
            source,
            target,
            &mut semantic,
        )
        .unwrap();
        let prepared = semantic.prepare(&mut store).unwrap();
        let (_result, committed) = prepared.commit_with_store();

        assert_eq!(
            committed
                .semantic_family_members_checked(execution_root)
                .unwrap(),
            &[before, after, target]
        );
        assert_eq!(
            committed.semantic_family_members_checked(source).unwrap(),
            source_members
        );
        assert_eq!(
            committed.semantic_family_members_checked(target).unwrap(),
            target_members
        );
    }

    #[test]
    fn completion_swap_rejects_aliased_source() {
        let mut store = SemanticStore::new();
        let source = family(&mut store, &[]);
        let target = family(&mut store, &[]);
        let execution_root = family(&mut store, &[source]);
        let alias_root = store.insert_family();
        store.add_member(alias_root, source).unwrap();

        let mut semantic = SemanticMutationTransaction::new();
        assert_eq!(
            stage_matching_family_completion_swap(
                &store,
                execution_root,
                source,
                target,
                &mut semantic,
            ),
            Err(MatchingFamilyCompletionSwapError::SourceNotDirectMember {
                execution_root,
                source,
            })
        );
    }

    #[test]
    fn completion_swap_rejects_mounted_target() {
        let mut store = SemanticStore::new();
        let source = family(&mut store, &[]);
        let target = family(&mut store, &[]);
        let execution_root = family(&mut store, &[source]);
        let target_root = family(&mut store, &[target]);
        assert_ne!(execution_root, target_root);

        let mut semantic = SemanticMutationTransaction::new();
        assert_eq!(
            stage_matching_family_completion_swap(
                &store,
                execution_root,
                source,
                target,
                &mut semantic,
            ),
            Err(MatchingFamilyCompletionSwapError::TargetNotDetached(target))
        );
    }
}

#[cfg(test)]
mod matching_target_tests {
    use noon_compile::{
        CompiledObject, CompiledScene, EffectiveAnimationProperties,
        PreparedDerivedFamilyTransformTrack, PreparedMatchingShapeLeftoverFadeTrack,
        PreparedMatchingShapeTargetTransientBase,
    };
    use noon_core::{
        CompositionTimeMap, GeometryRef, ObjectContentRef, ObjectId, Property, RateFunction,
        SemanticNodeId, Style, TrackTiming, TrackValues, Transform2D,
    };

    use super::*;

    fn runtime() -> SceneInstance {
        SceneInstance::new(
            CompiledScene::compile_objects(
                vec![
                    CompiledObject::new(
                        ObjectId::new(1),
                        GeometryRef::circle(1.0),
                        Transform2D::IDENTITY,
                        Style::default(),
                    ),
                    CompiledObject::new(
                        ObjectId::new(2),
                        GeometryRef::rectangle(1.0, 1.0),
                        Transform2D::IDENTITY,
                        Style::default(),
                    ),
                ],
                &[],
            )
            .unwrap(),
        )
    }

    fn target_fade(z_index: f64) -> PreparedMatchingShapeTargetLeftoverFade {
        PreparedMatchingShapeTargetLeftoverFade {
            target_index: 4,
            base: PreparedMatchingShapeTargetTransientBase {
                node: SemanticNodeId::new(9, 0),
                z_index,
                content: ObjectContentRef::Geometry(GeometryRef::circle(0.5)),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
                reveal: 1.0,
                morph: 0.0,
            },
            tracks: vec![PreparedMatchingShapeLeftoverFadeTrack {
                property: Property::Appearance,
                values: TrackValues::Scalar { from: 0.0, to: 1.0 },
                timing: TrackTiming::new(2.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        }
    }

    fn derived_occurrence() -> PreparedDerivedFamilyTransformOccurrence {
        PreparedDerivedFamilyTransformOccurrence {
            occurrence_index: 0,
            anchor_execution_object_id: ObjectId::new(1),
            source: SemanticNodeId::new(1, 0),
            target_state: SemanticNodeId::new(2, 0),
            effective_source: EffectiveAnimationProperties {
                z_index: 0.0,
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
                reveal: 1.0,
            },
            tracks: vec![PreparedDerivedFamilyTransformTrack {
                occurrence_index: 0,
                anchor_execution_object_id: ObjectId::new(1),
                property: Property::Appearance,
                values: TrackValues::Scalar { from: 0.0, to: 1.0 },
                transform_geometry_plan: None,
                timing: TrackTiming::new(2.0, 1.0, RateFunction::Linear),
                time_map: CompositionTimeMap::identity(),
            }],
        }
    }

    #[test]
    fn target_leftover_resolves_layer_end_to_last_real_stable_row() {
        let runtime = runtime();
        let plan = build_matching_shape_target_leftover_plan(&runtime, &[target_fade(0.0)], 12)
            .unwrap()
            .unwrap();

        assert_eq!(plan.occurrences().len(), 1);
        assert_eq!(plan.occurrences()[0].occurrence_index, 12);
        assert_eq!(
            plan.occurrences()[0].painter_placement,
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index: 1,
            }
        );
        assert_eq!(plan.evaluate(2.0).unwrap()[0].state().appearance, 0.0);
        assert_eq!(plan.evaluate(3.0).unwrap()[0].state().appearance, 1.0);
    }

    #[test]
    fn matching_plan_combines_derived_and_target_occurrences_without_index_collision() {
        let runtime = runtime();
        let plan = build_matching_family_transform_plan_from_parts(
            &runtime,
            &[derived_occurrence()],
            &[target_fade(0.0)],
            1,
            |z| stable_layer_tail(&runtime, z),
        )
        .unwrap()
        .unwrap();

        assert_eq!(plan.occurrences().len(), 2);
        assert_eq!(plan.occurrences()[0].occurrence_index, 0);
        assert_eq!(plan.occurrences()[1].occurrence_index, 1);
        assert_eq!(
            plan.occurrences()[0].painter_placement,
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index: 0,
            }
        );
        assert_eq!(
            plan.occurrences()[1].painter_placement,
            TransientPresentationPainterPlacement::AfterStable {
                anchor_object_index: 1,
            }
        );
        assert_eq!(plan.evaluate(2.5).unwrap().len(), 2);
    }

    #[test]
    fn target_leftover_without_real_stable_layer_fails_closed() {
        let runtime = runtime();
        let error = build_matching_shape_target_leftover_plan(&runtime, &[target_fade(7.0)], 0)
            .unwrap_err();

        assert!(error.contains("no stable painter row in z layer 7"));
    }
}

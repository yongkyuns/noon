use noon_compile::{
    PreparedFamilyTransformChannelProjection, PreparedMatchingShapeTargetLeftoverFade,
    PreparedTransientPainterPlacement,
};
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
    if projection.derived_occurrences().is_empty() {
        return Ok(None);
    }

    let frame = runtime.frame();
    let mut occurrences = Vec::with_capacity(projection.derived_occurrences().len());
    for occurrence in projection.derived_occurrences() {
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

    DerivedDisplayAnimationPlan::new(occurrences)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Materialize detached matching-shape target leftovers into the existing
/// identity-free transient renderer path.
///
/// Compiler `LayerEnd` intent is resolved against the activation frame to the last
/// *real* stable row in the same z layer. That row is painter placement provenance
/// only; it does not become semantic or execution identity for the detached target.
/// A layer with no stable row fails closed rather than fabricating an anchor or
/// silently changing z ordering.
#[allow(dead_code)]
pub(super) fn build_matching_shape_target_leftover_plan(
    runtime: &SceneInstance,
    target_fades: &[PreparedMatchingShapeTargetLeftoverFade],
    occurrence_index_start: u32,
) -> Result<Option<DerivedDisplayAnimationPlan>, String> {
    if target_fades.is_empty() {
        return Ok(None);
    }

    let mut occurrences = Vec::with_capacity(target_fades.len());
    for (ordinal, fade) in target_fades.iter().enumerate() {
        if fade.base.painter_placement() != PreparedTransientPainterPlacement::LayerEnd {
            return Err(format!(
                "matching-shape target leftover {} unexpectedly requests anchored painter placement",
                fade.target_index
            ));
        }
        let anchor_object_index =
            stable_layer_tail(runtime, fade.base.z_index).ok_or_else(|| {
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

    DerivedDisplayAnimationPlan::new(occurrences)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[allow(dead_code)]
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

#[cfg(test)]
mod matching_target_tests {
    use noon_compile::{
        CompiledObject, CompiledScene, PreparedMatchingShapeLeftoverFadeTrack,
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
    fn target_leftover_without_real_stable_layer_fails_closed() {
        let runtime = runtime();
        let error = build_matching_shape_target_leftover_plan(&runtime, &[target_fade(7.0)], 0)
            .unwrap_err();

        assert!(error.contains("no stable painter row in z layer 7"));
    }
}

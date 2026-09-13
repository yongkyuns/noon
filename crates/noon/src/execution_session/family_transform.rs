use noon_compile::PreparedFamilyTransformChannelProjection;
use noon_runtime::{
    DerivedDisplayAnimationOccurrence, DerivedDisplayAnimationPlan, DerivedDisplayAnimationTrack,
    DerivedDisplayObjectState, SceneInstance,
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
            anchor_object_index: u32::try_from(anchor_object_index).map_err(|_| {
                format!(
                    "derived family Transform occurrence {} source row exceeds u32 painter indexing",
                    occurrence.occurrence_index
                )
            })?,
            occurrence_index: occurrence.occurrence_index,
            base,
            tracks,
        });
    }

    DerivedDisplayAnimationPlan::new(occurrences)
        .map(Some)
        .map_err(|error| error.to_string())
}

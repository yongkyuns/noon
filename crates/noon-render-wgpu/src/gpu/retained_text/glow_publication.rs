//! Automatic retained glow preparation for native, direct WASM and capture hosts.
//!
//! This is a disposable sparse index of the existing runtime column, not an
//! effect-value store. No frontend request, object scan on clean publications,
//! or additional clock is involved. The genuine worker reborrows this exact
//! publication from its validated retained mirror. Scene admission belongs to
//! shared lowering; actual cross-worker playback has a separate qualification.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::{EffectCaptureRequest, EffectCaptureStats, EffectPrepareError};

/// Current renderer-owned capture/filter texture budget. In-flight resources and
/// driver bookkeeping are not included; hosts retain their own submission fences.
pub const DEFAULT_EFFECT_TEXTURE_BUDGET: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedEffectStats {
    /// Runtime rows examined while updating the sparse source index.
    pub rows_examined: usize,
    /// Source-to-packed mappings checked during this preparation.
    pub sources_prepared: usize,
    pub source_passes: usize,
    pub blur_passes: usize,
    pub composite_passes: usize,
    pub bytes_uploaded: usize,
    pub texture_allocations: usize,
    pub retained_texture_bytes: u64,
}
impl RetainedEffectStats {
    fn accumulate(&mut self, value: EffectCaptureStats) {
        self.sources_prepared += 1;
        self.source_passes += value.source_passes;
        self.blur_passes += value.filter.blur_passes;
        self.composite_passes += value.filter.composite_passes;
        self.bytes_uploaded += value.source_bytes_uploaded
            + value.placement_bytes_uploaded
            + value.filter.bytes_uploaded;
        self.texture_allocations +=
            value.source_texture_allocations + value.filter.texture_allocations;
    }
}

#[derive(Debug, Default)]
pub(crate) struct RetainedEffectPublication {
    context: Option<PublicationContext>,
    sources: BTreeMap<usize, ObjectId>,
    // Existing source indices -> current private geometry scratch identities.
    packed: BTreeMap<usize, (ObjectId, ObjectId)>,
    dirty: BTreeSet<usize>,
    visible: Vec<usize>,
    painter_ranks: Vec<usize>,
    view: Option<(Camera2D, [u32; 2])>,
    rows_examined: usize,
    invalidated: bool,
}
impl RetainedEffectPublication {
    pub(crate) fn invalidate(&mut self) {
        self.invalidated = true;
    }

    fn sync(&mut self, publication: &RendererPublication<'_>) -> Result<(), EffectPrepareError> {
        let context = publication.context();
        if self
            .context
            .is_some_and(|old| publication_is_stale(context, old))
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        if self.context == Some(context) && !publication.changes().is_all() {
            return Ok(());
        }
        let frame = publication.frame();
        let full = self.context.is_none() || publication.changes().is_all();
        if full {
            self.dirty.extend(self.packed.keys().copied());
            self.sources.clear();
            for (index, row) in frame.objects.iter().enumerate() {
                self.rows_examined += 1;
                if row.effect.is_some() && frame.is_drawable(index) {
                    self.sources.insert(index, row.id);
                    self.dirty.insert(index);
                }
            }
        } else {
            for &index in publication.changes().object_indices() {
                self.rows_examined += 1;
                let next = frame
                    .objects
                    .get(index)
                    .filter(|row| row.effect.is_some() && frame.is_drawable(index))
                    .map(|row| row.id);
                if let Some(object) = next {
                    self.sources.insert(index, object);
                    self.dirty.insert(index);
                } else if self.sources.remove(&index).is_some() || self.packed.contains_key(&index)
                {
                    self.dirty.insert(index);
                }
            }
        }
        // Order belongs to the publication, including intervals with no glow.
        // Initialize this projection at the full-publication barrier and keep
        // its changed ranges current so a later local attachment needs no scan.
        let order = publication.painter_order();
        if full {
            self.painter_ranks.clear();
            self.painter_ranks.resize(frame.objects.len(), usize::MAX);
            for (rank, &index) in order.iter().enumerate() {
                self.painter_ranks[index as usize] = rank;
            }
        } else if let Some(range) = publication.changes().painter_order_range() {
            self.painter_ranks.resize(frame.objects.len(), usize::MAX);
            for rank in range.start..range.end.min(order.len()) {
                self.painter_ranks[order[rank] as usize] = rank;
            }
        }
        self.context = Some(context);
        Ok(())
    }
}

impl GpuRenderer {
    /// A transport renderer can return to its ordinary zero-effect fast path
    /// after the last published source and its scope resources are retired.
    pub fn has_retained_effect_sources(&self) -> bool {
        !self.retained_effects.sources.is_empty()
    }

    /// Drop publication-scoped glow bindings when a new validated worker
    /// session replaces the previous one. Frame epochs order a single session;
    /// they cannot establish authority across unrelated sessions. All GPU
    /// textures remain safely retained by any already-submitted wgpu work.
    pub fn reset_retained_effect_publication(&mut self) {
        self.retained_effects = RetainedEffectPublication::default();
        self.effect_captures = None;
    }

    /// Add only effect-source indices to the execution-owned, painter-ordered
    /// visibility result. The usual retained preparer validates the candidate set.
    /// A source may be outside semantic bounds while its padded halo is visible.
    /// The source itself is still raster-clipped; layout and picking are unchanged.
    /// No-effect calls return the input slice without allocating or copying it.
    ///
    /// Call before the normal retained geometry/text preparation, then call
    /// `prepare_retained_analytic_glows` with the same publication and encoder.
    pub fn effect_source_visibility<'a>(
        &'a mut self,
        publication: &RendererPublication<'_>,
        visible: &'a [usize],
    ) -> Result<&'a [usize], EffectPrepareError> {
        self.retained_effects.sync(publication)?;
        if self.retained_effects.sources.is_empty() {
            return Ok(visible);
        }
        if !self.inset_views.is_empty() {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        // The execution-owned candidate slice is already in painter order.
        // Typical on-screen glow needs no visibility copy, allocation or full
        // candidate scan: prove each source by its existing inverse rank.
        // The ordinary retained preparer still validates the input index set.
        if self.retained_effects.sources.keys().all(|index| {
            let rank = self.retained_effects.painter_ranks[*index];
            visible
                .binary_search_by_key(&rank, |candidate| {
                    self.retained_effects
                        .painter_ranks
                        .get(*candidate)
                        .copied()
                        .unwrap_or(usize::MAX)
                })
                .is_ok()
        }) {
            return Ok(visible);
        }
        validate_visible_object_indices(publication.frame(), visible)
            .map_err(|_| EffectPrepareError::PublicationMismatch)?;
        let state = &mut self.retained_effects;
        // Visibility is in painter order, NOT source-index order. Merge the
        // extra source candidates with the existing runtime-derived ranks.
        state.visible.clear();
        state.visible.extend_from_slice(visible);
        state.visible.extend(state.sources.keys().copied());
        state.visible.sort_unstable_by_key(|index| {
            state
                .painter_ranks
                .get(*index)
                .copied()
                .unwrap_or(usize::MAX)
        });
        state.visible.dedup();
        Ok(&state.visible)
    }

    /// Prepare every affected published glow through the same retained geometry
    /// and painter path used by ordinary geometry, text and images. Values come
    /// exclusively from the borrowed runtime publication. Clean calls do no GPU
    /// work; view changes revisit only glow sources, not unrelated frame rows.
    ///
    /// On any failure the host must abandon this encoder. All affected cache
    /// commands are invalidated for retry; no partial frame may be submitted.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_retained_effects(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &RetainedTextGpuState,
        publication: &RendererPublication<'_>,
        texture_budget_bytes: u64,
    ) -> Result<RetainedEffectStats, EffectPrepareError> {
        let result = self.prepare_retained_effects_inner(
            device,
            queue,
            encoder,
            prepared,
            text_state,
            publication,
            texture_budget_bytes,
        );
        if result.is_err() {
            self.invalidate_effect_captures();
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_retained_effects_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        prepared: &PreparedRetainedGpuFrame<'_>,
        text_state: &RetainedTextGpuState,
        publication: &RendererPublication<'_>,
        texture_budget_bytes: u64,
    ) -> Result<RetainedEffectStats, EffectPrepareError> {
        let context = publication.context();
        if self.retained_effects.context != Some(context)
            || *prepared.applied_publication != Some(context)
            || prepared.time() != publication.frame().time
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        let view = (self.camera, self.viewport_size);
        if self.retained_effects.view != Some(view)
            || self.retained_effects.invalidated
            || prepared.geometry.stats.full_rebuilds > 0
        {
            self.retained_effects
                .dirty
                .extend(self.retained_effects.sources.keys().copied());
        }
        if !self.retained_effects.sources.is_empty() && !self.inset_views.is_empty() {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let frame = publication.frame();
        // Prove all source mappings before recording GPU work. Scratch identities
        // stay private; no assumption that frame index equals packed index.
        let mut sources = Vec::with_capacity(self.retained_effects.dirty.len());
        for &index in &self.retained_effects.dirty {
            let mapped = if let Some(&object) = self.retained_effects.sources.get(&index) {
                let row = frame
                    .objects
                    .get(index)
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                if row.id != object || !frame.is_drawable(index) || row.spatial.is_some() {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                let effect = row
                    .effect
                    .as_ref()
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                let analytic = matches!(
                    row.geometry(),
                    Some(GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. })
                ) && row.style.fill.is_some()
                    && row.fill_gradient.is_none()
                    && row.style.stroke.is_none()
                    && frame.reveal(index) == 1.0
                    && publication.active_family_animation_indices().is_empty();
                if !analytic {
                    if prepared.object_indices.get(&object) != Some(&index)
                        || prepared.capture_items(object).is_none()
                        || !prepared.source_is_visible(object)
                    {
                        return Err(EffectPrepareError::PublicationMismatch);
                    }
                    sources.push((index, Some((object, index, effect.definition, None))));
                    continue;
                }
                let scratch = if prepared.geometry_only {
                    index
                } else {
                    prepared
                        .source_geometry_slots
                        .and_then(|slots| slots.get(index))
                        .copied()
                        .flatten()
                        .ok_or(EffectPrepareError::UnsupportedCapture)?
                };
                let mut observation = prepared
                    .geometry
                    .observe_object(scratch)
                    .map_err(|_| EffectPrepareError::UnsupportedCapture)?;
                let geometry = match observation.primitive {
                    RenderPrimitive::Circle => GeometryRef::circle(
                        prepared.geometry.circles[observation.instance_index].radius,
                    ),
                    RenderPrimitive::Rectangle => {
                        let size = prepared.geometry.rectangles[observation.instance_index].size;
                        GeometryRef::rectangle(size[0], size[1])
                    }
                    _ => return Err(EffectPrepareError::UnsupportedCapture),
                };
                // This retained frame uses the visibility union admitted above.
                // A transparent source's anchor must survive paint suppression.
                let included = if prepared.geometry_only {
                    prepared.geometry.source_is_submitted(scratch)
                } else {
                    // Existing source-to-scratch and painter-rank tables prove
                    // one geometry item in O(log visible items), not a scene scan.
                    let rank = prepared
                        .painter_ranks
                        .get(index)
                        .copied()
                        .ok_or(EffectPrepareError::PublicationMismatch)?;
                    prepared.object_indices.get(&object) == Some(&index)
                        && prepared.render_items.binary_search_by_key(&rank, |item| {
                            prepared.object_indices.get(&item.object_id())
                                .and_then(|index| prepared.painter_ranks.get(*index))
                                .copied().unwrap_or(usize::MAX)
                        }).ok().and_then(|at| prepared.render_items.get(at)).is_some_and(|item| {
                            matches!(item, RetainedRenderItem::Geometry { object_id, batch }
                                if *object_id == object && batch.primitive == observation.primitive
                                    && batch.instance_range.contains(&(observation.instance_index as u32)))
                        })
                };
                if !included
                    || observation.transform != frame.render_transform(index).into()
                    || observation.style != crate::pack_style(row)
                    || frame.render_geometry(index) != Some(&geometry)
                    || frame.reveal(index) != 1.0
                    || (prepared.geometry_only && observation.object != object)
                {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                // Partial visibility has no O(1) public observation, but the
                // source index plus existing item/slot mapping above proves it.
                observation.submission_membership = Some(true);
                Some((object, scratch, effect.definition, Some(observation)))
            } else {
                None
            };
            sources.push((index, mapped));
        }
        // Retire old mappings before installing new ones, so packed-slot swaps
        // cannot remove the other source's newly prepared scope.
        for (index, mapped) in &sources {
            let next = mapped.as_ref().map(|(object, _, _, observation)| {
                (
                    *object,
                    observation
                        .as_ref()
                        .map_or(*object, |observation| observation.object),
                )
            });
            if let Some(old) = self
                .retained_effects
                .packed
                .get(index)
                .copied()
                .filter(|old| Some(*old) != next)
            {
                self.remove_effect_capture(old.1);
                self.retained_effects.packed.remove(index);
            }
        }
        let mut stats = RetainedEffectStats::default();
        if sources.iter().any(|(_, mapped)| {
            mapped
                .as_ref()
                .is_some_and(|(_, _, _, observation)| observation.is_none())
        }) {
            // Newly rasterized glyphs must be resident before an isolated draw,
            // not only before the later main pass. Leave acknowledgement to the
            // successful ordinary encode so abandoned encoders remain retryable.
            prepared.text.atlas.encode_pending_uploads(encoder);
        }
        for (index, mapped) in sources {
            if let Some((object, scratch, definition, observation)) = mapped {
                let packed = observation
                    .as_ref()
                    .map_or(object, |observation| observation.object);
                let request = EffectCaptureRequest {
                    object_index: scratch,
                    definition,
                    texture_budget_bytes,
                };
                let value = if let Some(observation) = observation {
                    self.prepare_observed_effect_capture(
                        device,
                        queue,
                        encoder,
                        &prepared.geometry,
                        request,
                        observation,
                    )?
                } else {
                    self.prepare_draw_source_effect(
                        device,
                        queue,
                        encoder,
                        prepared,
                        text_state,
                        request,
                        object,
                        crate::pack_style(&frame.objects[index]).opacity,
                    )?
                };
                stats.accumulate(value);
                self.retained_effects.packed.insert(index, (object, packed));
            }
        }
        // Even a clean publication must admit a newly reduced host budget.
        // Removal is processed first so retiring a scope can satisfy that budget.
        if self.effect_capture_texture_bytes() > texture_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        stats.rows_examined = std::mem::take(&mut self.retained_effects.rows_examined);
        stats.retained_texture_bytes = self.effect_capture_texture_bytes();
        self.retained_effects.dirty.clear();
        self.retained_effects.view = Some(view);
        self.retained_effects.invalidated = false;
        Ok(stats)
    }
}

#[cfg(all(test, feature = "ci-noop"))]
mod tests;

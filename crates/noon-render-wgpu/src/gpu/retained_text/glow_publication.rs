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

mod backdrop_inputs;
mod composed;
mod prefix_admission;
use backdrop_inputs::BackdropInputs;
use composed::ComposedCaptures;

/// Current renderer-owned capture/filter texture budget. In-flight resources and
/// driver bookkeeping are not included; hosts retain their own submission fences.
pub const DEFAULT_EFFECT_TEXTURE_BUDGET: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedEffectStats {
    /// Runtime rows examined while updating the sparse source index.
    pub rows_examined: usize,
    /// Changed/crossed rows checked for finite backdrop input capability.
    pub prefix_admission_rows: usize,
    pub prefix_queries: usize,
    pub prefix_candidates: usize,
    pub prefix_footprints: usize,
    pub prefix_links: usize,
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
        self.accumulate_work(value);
    }
    fn accumulate_work(&mut self, value: EffectCaptureStats) {
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
    capture_dirty: BTreeSet<usize>,
    composed: ComposedCaptures,
    prefix_owners: BTreeSet<usize>,
    prefix_admission: noon_compile::PrefixAdmission,
    prefix_admission_rows: usize,
    backdrops: BackdropInputs,
    visible: Vec<usize>,
    painter_ranks: Vec<usize>,
    view: Option<(Camera2D, [u32; 2])>,
    rows_examined: usize,
    invalidated: bool,
    sync_failed: bool,
    clear: Option<wgpu::Color>,
}
impl RetainedEffectPublication {
    fn active_source(
        frame: &noon_runtime::FrameState,
        index: usize,
        view: (Camera2D, [u32; 2]),
    ) -> Result<bool, EffectPrepareError> {
        let row = &frame.objects[index];
        let opacity = crate::pack_style(row).opacity;
        if row.content.composed_source().is_some() && opacity != 1.0 {
            return Ok(true);
        }
        let Some(effects) = row.effects.as_ref() else {
            return Ok(false);
        };
        let mut active = false;
        for &definition in effects.definitions() {
            let parameters = crate::EffectCaptureParameters {
                definition,
                output_height: view.1[1],
                world_view_height: f64::from(view.0.world_size.y),
                scope_opacity: opacity,
            };
            active |= !parameters.is_neutral_at_sigma(parameters.sigma_pixels()?);
        }
        Ok(active && opacity > 0.0)
    }

    fn has_original_mask(frame: &noon_runtime::FrameState, index: usize) -> bool {
        frame.objects[index].effects.as_ref().is_some_and(|stack| {
            stack.definitions().iter().any(|definition| {
                definition
                    .as_glow()
                    .is_some_and(|glow| glow.source() == noon_core::GlowSource::Silhouette)
            })
        })
    }

    fn original_by_parent(
        &self,
        frame: &noon_runtime::FrameState,
        index: usize,
        view: (Camera2D, [u32; 2]),
    ) -> Result<bool, EffectPrepareError> {
        if !self.composed.scopes.contains_key(&index)
            && frame.objects[index]
                .effects
                .as_ref()
                .and_then(|stack| stack.visibility())
                .is_none()
        {
            return Ok(false);
        }
        for parent in self
            .composed
            .dependents(index)
            .chain(self.composed.ancestors(index))
        {
            if Self::has_original_mask(frame, parent) && Self::active_source(frame, parent, view)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn required_by_parent(
        &self,
        frame: &noon_runtime::FrameState,
        index: usize,
        view: (Camera2D, [u32; 2]),
    ) -> Result<bool, EffectPrepareError> {
        if self.backdrops.required(index) {
            return Ok(true);
        }
        for parent in self
            .composed
            .dependents(index)
            .chain(self.composed.ancestors(index))
        {
            if Self::active_source(frame, parent, view)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn composed_placement(
        &self,
        object: ObjectId,
        enabled: impl Fn(ObjectId) -> bool,
    ) -> Option<(ObjectId, bool)> {
        self.composed.placement(object, enabled)
    }

    pub(crate) fn packed_capture_owner(&self, index: usize, object: ObjectId) -> Option<ObjectId> {
        self.packed
            .get(&index)
            .filter(|(owner, _)| *owner == object)
            .map(|(_, packed)| *packed)
    }

    pub(crate) fn composed_candidates(
        &self,
        owner: usize,
        member: usize,
    ) -> impl Iterator<Item = (usize, ObjectId, usize, ObjectId, usize)> + '_ {
        self.composed
            .candidates(owner, member)
            .map(|(index, scope)| {
                (
                    index,
                    scope.owner,
                    scope.source.members().len(),
                    scope.source.members()[0],
                    scope.depth.unwrap_or(0),
                )
            })
    }

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
        if self.context == Some(context) && !publication.changes().is_all() && !self.sync_failed {
            return Ok(());
        }
        let frame = publication.frame();
        let full = self.context.is_none() || publication.changes().is_all() || self.sync_failed;
        let order = publication.painter_order();
        let mut order_affected = BTreeSet::new();
        let mut capture_ancestors = BTreeSet::new();
        if full {
            self.painter_ranks.clear();
            self.painter_ranks.resize(frame.objects.len(), usize::MAX);
            for (rank, &index) in order.iter().enumerate() {
                self.painter_ranks[index as usize] = rank;
            }
            self.composed = ComposedCaptures::default();
        } else if let Some(range) = publication.changes().painter_order_range() {
            self.painter_ranks.resize(frame.objects.len(), usize::MAX);
            for (rank, &index) in order.iter().enumerate().take(range.end).skip(range.start) {
                let index = index as usize;
                self.painter_ranks[index] = rank;
                order_affected.extend(self.composed.dependents(index));
            }
        }
        if full {
            self.dirty.extend(self.packed.keys().copied());
            self.sources.clear();
            self.prefix_owners.clear();
            for (index, row) in frame.objects.iter().enumerate() {
                self.rows_examined += 1;
                if row.content.composed_source().is_some() {
                    self.composed
                        .install(publication, &self.painter_ranks, index)?;
                }
                if (row.effects.is_some()
                    || row.content.composed_source().is_some() && row.style.opacity != 1.0)
                    && (frame.is_drawable(index)
                        || (row.content.composed_source().is_some() && frame.is_present(index)))
                {
                    self.sources.insert(index, row.id);
                    if row.effects.as_ref().is_some_and(|stack| {
                        stack.input_role() == noon_core::EffectInputRole::PainterPrefix
                    }) {
                        self.prefix_owners.insert(index);
                    }
                    self.dirty.insert(index);
                    self.capture_dirty.insert(index);
                }
            }
        } else {
            for &index in publication.changes().object_indices() {
                self.rows_examined += 1;
                capture_ancestors.extend(self.composed.dependents(index));
                capture_ancestors.extend(self.composed.ancestors(index));
                let old_source = self
                    .composed
                    .scopes
                    .get(&index)
                    .map(|scope| scope.source.clone());
                if frame
                    .objects
                    .get(index)
                    .is_some_and(|row| row.content.composed_source().is_some())
                {
                    self.composed
                        .install(publication, &self.painter_ranks, index)?;
                    if old_source.as_ref().is_none_or(|old| {
                        self.composed
                            .scopes
                            .get(&index)
                            .is_none_or(|next| !Arc::ptr_eq(old, &next.source))
                    }) {
                        self.capture_dirty.insert(index);
                    }
                } else if old_source.is_some() {
                    self.composed
                        .install(publication, &self.painter_ranks, index)?;
                }
                let next = frame
                    .objects
                    .get(index)
                    .filter(|row| {
                        (row.effects.is_some()
                            || row.content.composed_source().is_some() && row.style.opacity != 1.0)
                            && (frame.is_drawable(index)
                                || (row.content.composed_source().is_some()
                                    && frame.is_present(index)))
                    })
                    .map(|row| row.id);
                if next.is_some()
                    && frame.objects[index].effects.as_ref().is_some_and(|stack| {
                        stack.input_role() == noon_core::EffectInputRole::PainterPrefix
                    })
                {
                    self.prefix_owners.insert(index);
                } else {
                    self.prefix_owners.remove(&index);
                }
                if let Some(object) = next {
                    if self.composed.scopes.contains_key(&index)
                        && !self.sources.contains_key(&index)
                    {
                        // Reattaching a treatment must validate eligibility that
                        // ordinary drawing could change while this plan was inert.
                        capture_ancestors.insert(index);
                        self.capture_dirty.insert(index);
                    }
                    self.sources.insert(index, object);
                    self.dirty.insert(index);
                } else if self.sources.remove(&index).is_some() || self.packed.contains_key(&index)
                {
                    self.dirty.insert(index);
                }
            }
        }
        self.composed.finish_topology()?;
        if !full {
            for &index in publication.changes().object_indices() {
                capture_ancestors.extend(self.composed.dependents(index));
                capture_ancestors.extend(self.composed.ancestors(index));
                order_affected.extend(self.composed.dependents(index));
            }
        }
        for scope in order_affected {
            self.composed.validate_order(&self.painter_ranks, scope)?;
            capture_ancestors.insert(scope);
        }
        for scope in capture_ancestors {
            self.composed.validate_inputs(publication, scope)?;
            if self.sources.contains_key(&scope) {
                self.dirty.insert(scope);
                self.capture_dirty.insert(scope);
            }
        }
        self.sync_prefix_admission(publication, full)?;
        self.context = Some(context);
        self.sync_failed = false;
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
        if self
            .retained_effects
            .context
            .is_some_and(|old| publication_is_stale(publication.context(), old))
        {
            return Err(EffectPrepareError::PublicationMismatch);
        }
        if let Err(error) = self.retained_effects.sync(publication) {
            self.retained_effects.sync_failed = true;
            self.invalidate_effect_captures();
            return Err(error);
        }
        if self.retained_effects.sources.is_empty() {
            self.retained_effects.backdrops = BackdropInputs::default();
            return Ok(visible);
        }
        let mut backdrops = std::mem::take(&mut self.retained_effects.backdrops);
        let result = backdrops.refresh(
            publication,
            &mut self.retained_effects,
            self.camera,
            self.viewport_size,
        );
        self.retained_effects.backdrops = backdrops;
        if let Err(error) = result {
            self.retained_effects.sync_failed = true;
            self.invalidate_effect_captures();
            return Err(error);
        }
        if !self.inset_views.is_empty() {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        // The execution-owned candidate slice is already in painter order.
        // Typical on-screen glow needs no visibility copy, allocation or full
        // candidate scan: prove each source by its existing inverse rank.
        // The ordinary retained preparer still validates the input index set.
        let required = || {
            self.retained_effects
                .sources
                .keys()
                .copied()
                .filter(|index| !self.retained_effects.composed.scopes.contains_key(index))
                .chain(
                    self.retained_effects
                        .composed
                        .visibility(&self.retained_effects.sources),
                )
                .chain(self.retained_effects.backdrops.visible_members())
        };
        if required().all(|index| {
            let rank = self.retained_effects.painter_ranks[index];
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
        let extra = required().collect::<Vec<_>>();
        let state = &mut self.retained_effects;
        // Visibility is in painter order, NOT source-index order. Merge the
        // extra source candidates with the existing runtime-derived ranks.
        state.visible.clear();
        state.visible.extend_from_slice(visible);
        state.visible.extend(extra);
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
        clear_color: wgpu::Color,
        texture_budget_bytes: u64,
    ) -> Result<RetainedEffectStats, EffectPrepareError> {
        let result = self.prepare_retained_effects_inner(
            device,
            queue,
            encoder,
            prepared,
            text_state,
            publication,
            clear_color,
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
        clear_color: wgpu::Color,
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
            self.retained_effects
                .capture_dirty
                .extend(self.retained_effects.sources.keys().copied());
        }
        if !self.retained_effects.sources.is_empty() && !self.inset_views.is_empty() {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        if self.retained_effects.clear != Some(clear_color) {
            self.retained_effects
                .dirty
                .extend(self.retained_effects.prefix_owners.iter().copied());
        }
        let frame = publication.frame();
        // Prove all source mappings before recording GPU work. Scratch identities
        // stay private; no assumption that frame index equals packed index.
        // Activating a parent can demand a formerly offscreen child. Revisit
        // only its declared input closure, and only when this parent needs a
        // capture or has no usable output. Warm active parameter edits skip it.
        let mut demanded = BTreeSet::new();
        for &index in &self.retained_effects.dirty {
            let Some(scope) = self.retained_effects.composed.scopes.get(&index) else {
                continue;
            };
            if !RetainedEffectPublication::active_source(frame, index, view)?
                || (!self.retained_effects.capture_dirty.contains(&index)
                    && self.retained_input_is_ready(index, scope.owner)
                    && (!RetainedEffectPublication::has_original_mask(frame, index)
                        || self.retained_original_is_ready(index, scope.owner)))
            {
                continue;
            }
            for &object in scope.source.members() {
                let member = publication
                    .object_index(object)
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                demanded.insert(member);
                demanded.extend(
                    self.retained_effects
                        .composed
                        .candidates(index, member)
                        .map(|(child, _)| child),
                );
            }
        }
        for &owner in &self.retained_effects.dirty {
            if let Some(input) = self.retained_effects.backdrops.entries.get(&owner) {
                demanded.extend(input.dependencies());
            }
        }
        for index in demanded {
            if let Some(&object) = self.retained_effects.sources.get(&index) {
                let original = self
                    .retained_effects
                    .original_by_parent(frame, index, view)?;
                if (!self.retained_input_is_ready(index, object)
                    || (original && !self.retained_original_is_ready(index, object)))
                    && (original || RetainedEffectPublication::active_source(frame, index, view)?)
                {
                    self.retained_effects.dirty.insert(index);
                }
            }
        }
        let mut sources = Vec::with_capacity(self.retained_effects.dirty.len());
        for &index in &self.retained_effects.dirty {
            let mapped = if let Some(&object) = self.retained_effects.sources.get(&index) {
                let row = frame
                    .objects
                    .get(index)
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                let composed = self.retained_effects.composed.scopes.contains_key(&index);
                if row.id != object
                    || (!composed && !frame.is_drawable(index))
                    || row.spatial.is_some()
                {
                    return Err(EffectPrepareError::PublicationMismatch);
                }
                let definitions = row
                    .effects
                    .as_ref()
                    .map_or(&[][..], |effects| effects.definitions());
                if composed {
                    let anchor = self.retained_effects.composed.scopes[&index]
                        .source
                        .members()[0];
                    if !prepared.source_is_visible(anchor)
                        || prepared
                            .capture_items(anchor)
                            .is_none_or(|items| items.is_empty())
                    {
                        return Err(EffectPrepareError::UnsupportedCapture);
                    }
                    sources.push((index, Some((object, index, definitions, None))));
                    continue;
                }
                let effect = row
                    .effects
                    .as_ref()
                    .ok_or(EffectPrepareError::PublicationMismatch)?;
                let analytic = matches!(
                    row.geometry(),
                    Some(GeometryRef::Circle { .. } | GeometryRef::Rectangle { .. })
                ) && row.style.fill.is_some()
                    && row.fill_paint.is_none()
                    && row.style.stroke.is_none()
                    && effect.backdrop_blur().is_none()
                    && frame.reveal(index) == 1.0
                    && publication.active_family_animation_indices().is_empty();
                if !analytic {
                    if prepared.object_indices.get(&object) != Some(&index)
                        || prepared.capture_items(object).is_none()
                        || !prepared.source_is_visible(object)
                    {
                        return Err(EffectPrepareError::PublicationMismatch);
                    }
                    sources.push((index, Some((object, index, effect.definitions(), None))));
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
                Some((object, scratch, effect.definitions(), Some(observation)))
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
        // Strict painter dependencies precede a backdrop; child intervals
        // precede their containing scope even for coincident terminal ranks.
        sources.sort_by_key(|(index, _)| {
            let last = self
                .retained_effects
                .composed
                .scopes
                .get(index)
                .and_then(|scope| scope.source.members().last())
                .and_then(|object| publication.object_index(*object))
                .unwrap_or(*index);
            (
                self.retained_effects.painter_ranks[last],
                self.retained_effects
                    .composed
                    .depth(*index)
                    .map_or((false, std::cmp::Reverse(0)), |depth| {
                        (true, std::cmp::Reverse(depth))
                    }),
            )
        });
        for (index, mapped) in sources {
            if let Some((object, scratch, definitions, observation)) = mapped {
                let demand = super::super::glow_filter::CaptureDemand {
                    required_by_parent: self
                        .retained_effects
                        .required_by_parent(frame, index, view)?,
                    original_by_parent: self
                        .retained_effects
                        .original_by_parent(frame, index, view)?,
                    isolate: false,
                    padding_pixels: 0,
                };
                let packed = observation
                    .as_ref()
                    .map_or(object, |observation| observation.object);
                let prefix = self.retained_effects.backdrops.entries.get(&index).cloned();
                let backdrop = definitions
                    .iter()
                    .copied()
                    .find(|d| d.input_role() == noon_core::EffectInputRole::PainterPrefix);
                let painted = backdrop.map(|_| {
                    definitions
                        .iter()
                        .copied()
                        .filter(|d| d.input_role() == noon_core::EffectInputRole::Painted)
                        .collect::<Vec<_>>()
                });
                let definitions = painted.as_deref().unwrap_or(definitions);
                // The ordinary isolation can survive removal of its backdrop.
                // Retire prefix/filter textures and their source leases before
                // replacing the common capture or admitting its next budget.
                if backdrop.is_none() || prefix.is_none() {
                    self.retire_backdrop_output(object);
                }
                let extra_bytes = self.backdrop_texture_bytes();
                let request = EffectCaptureRequest {
                    object_index: scratch,
                    definitions,
                    texture_budget_bytes: texture_budget_bytes
                        .checked_sub(extra_bytes)
                        .ok_or(EffectPrepareError::ScratchBudgetExceeded)?,
                };
                let demand = if let Some(definition) = backdrop.filter(|_| prefix.is_some()) {
                    let sigma = crate::EffectCaptureParameters {
                        definition,
                        output_height: self.viewport_size[1],
                        world_view_height: f64::from(self.camera.world_size.y),
                        scope_opacity: crate::pack_style(&frame.objects[index]).opacity,
                    }
                    .sigma_pixels()?;
                    super::super::glow_filter::CaptureDemand {
                        isolate: true,
                        original_by_parent: true,
                        padding_pixels: (3.0 * sigma).ceil() as u32,
                        ..demand
                    }
                } else {
                    demand
                };
                let value = if let Some(observation) = observation {
                    self.prepare_observed_effect_capture_input(
                        device,
                        queue,
                        encoder,
                        &prepared.geometry,
                        request,
                        observation,
                        demand,
                        frame.objects[index]
                            .effects
                            .as_ref()
                            .and_then(|stack| stack.visibility())
                            .and_then(|effect| effect.definition.as_noise_dissolve()),
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
                        self.retained_effects
                            .composed
                            .scopes
                            .get(&index)
                            .map(|scope| scope.source.clone()),
                        self.retained_effects.capture_dirty.contains(&index),
                        demand,
                        frame.objects[index]
                            .effects
                            .as_ref()
                            .and_then(|stack| stack.visibility())
                            .and_then(|effect| effect.definition.as_noise_dissolve()),
                    )?
                };
                stats.accumulate(value);
                if let (Some(prefix), Some(definition)) = (prefix, backdrop) {
                    let value = self.prepare_backdrop_output(
                        device,
                        queue,
                        encoder,
                        prepared,
                        text_state,
                        index,
                        object,
                        definition,
                        super::super::source_capture::PainterPrefixCaptureRequest {
                            anchor: prefix.anchor,
                            candidates: prefix.candidates,
                            source_revision: prefix.source_epoch.get(),
                            origin: prefix.window.origin,
                            size: prefix.window.size,
                            clear_color,
                            scratch_budget_bytes: texture_budget_bytes,
                        },
                        texture_budget_bytes,
                    )?;
                    stats.accumulate_work(value);
                }
                self.retained_effects.packed.insert(index, (object, packed));
            }
        }
        // Even a clean publication must admit a newly reduced host budget.
        // Removal is processed first so retiring a scope can satisfy that budget.
        if self.effect_capture_texture_bytes() > texture_budget_bytes {
            return Err(EffectPrepareError::ScratchBudgetExceeded);
        }
        let queries = std::mem::take(&mut self.retained_effects.backdrops.stats);
        stats.prefix_admission_rows =
            std::mem::take(&mut self.retained_effects.prefix_admission_rows);
        stats.prefix_queries = queries.queries;
        stats.prefix_candidates = queries.candidates;
        stats.prefix_footprints = queries.footprints;
        stats.prefix_links = queries.links;
        stats.rows_examined = std::mem::take(&mut self.retained_effects.rows_examined);
        stats.retained_texture_bytes = self.effect_capture_texture_bytes();
        self.retained_effects.dirty.clear();
        self.retained_effects.capture_dirty.clear();
        self.retained_effects.view = Some(view);
        self.retained_effects.clear = Some(clear_color);
        self.retained_effects.invalidated = false;
        Ok(stats)
    }
}

#[cfg(all(test, feature = "ci-noop"))]
mod tests;

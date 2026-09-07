use noon_runtime::{RetainedPlannedFamilyFrame, RetainedPlannedFamilyFrameError};

/// Failure while realizing multiple concurrently installed family plans.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyPlanSetPrepareError {
    Retained(RetainedPrepareError),
    PlannedFrame(RetainedPlannedFamilyFrameError),
    Reveal(RetainedFamilyPrepareError),
    DrawBorderThenFill(RetainedFamilyDrawBorderPrepareError),
    CachedScratchShapeChanged {
        object: ObjectId,
        expected: usize,
        actual: usize,
    },
}

impl std::fmt::Display for RetainedFamilyPlanSetPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::PlannedFrame(error) => error.fmt(formatter),
            Self::Reveal(error) => error.fmt(formatter),
            Self::DrawBorderThenFill(error) => error.fmt(formatter),
            Self::CachedScratchShapeChanged {
                object,
                expected,
                actual,
            } => write!(
                formatter,
                "cached family glyph rows for object {} changed shape from {expected} to {actual}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyPlanSetPrepareError {}

impl From<RetainedPrepareError> for RetainedFamilyPlanSetPrepareError {
    fn from(value: RetainedPrepareError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedPlannedFamilyFrameError> for RetainedFamilyPlanSetPrepareError {
    fn from(value: RetainedPlannedFamilyFrameError) -> Self {
        Self::PlannedFrame(value)
    }
}

impl From<RetainedFamilyPrepareError> for RetainedFamilyPlanSetPrepareError {
    fn from(value: RetainedFamilyPrepareError) -> Self {
        Self::Reveal(value)
    }
}

impl From<RetainedFamilyDrawBorderPrepareError> for RetainedFamilyPlanSetPrepareError {
    fn from(value: RetainedFamilyDrawBorderPrepareError) -> Self {
        Self::DrawBorderThenFill(value)
    }
}

impl RetainedFramePreparer {
    /// Prepare the ordinary runtime publication with its typed, execution-derived
    /// family plans. Direct native and direct WASM callers keep the plan/frame
    /// boundary in-process; only the genuine worker bridge serializes this view.
    pub fn prepare_planned_publication_visible<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        publication: &RendererPublication<'_>,
        visible_object_indices: &[usize],
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPlanSetPrepareError> {
        let received = publication.context();
        if let Some(applied) = self.last_applied_publication {
            if publication_is_stale(received, applied) {
                return Err(RetainedPrepareError::StalePublication { received, applied }.into());
            }
        }
        validate_visible_object_indices(publication.frame(), visible_object_indices)
            .map_err(RetainedPrepareError::from)?;

        let frame = publication.planned_family_frame();
        if publication.active_family_animation_indices().is_empty() {
            self.release_planned_family_realization();
            return self
                .prepare_publication_visible(
                    device,
                    queue,
                    publication,
                    visible_object_indices,
                    metrics,
                )
                .map_err(Into::into);
        }

        let prepared = self.prepare_family_plan_set_with_changes_inner(
            device,
            queue,
            &frame,
            publication.family_animation_plans(),
            publication.changes(),
            publication.text_resources(),
            publication.font_resources(),
            publication.geometry_resources(),
            metrics,
            Some(visible_object_indices),
            Some(publication.active_family_animation_indices()),
        )?;
        *prepared.applied_publication = Some(received);
        Ok(prepared)
    }

    /// Drop renderer-local glyph substitution metadata before returning to the
    /// ordinary atlas representation at operation completion.
    pub fn release_planned_family_realization(&mut self) {
        if self.family_plan_active_signature.is_empty() {
            return;
        }
        self.family_plan_active_signature.clear();
        self.family_plan_scratch_slots.clear();
        self.scratch_ready = false;
        self.prepared_generation_ready = false;
    }

    /// Prepare a frame with any number of immutable family plans in one renderer pass.
    ///
    /// Runtime plan identity is selected per object, so disjoint active requests may
    /// use different family operations concurrently. Sequential requests on the same
    /// object likewise select their exact plan at each time. Existing reveal and
    /// DrawBorderThenFill content realizers are reused; no scheduling semantics move
    /// into the renderer.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_plan_set_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPlanSetPrepareError> {
        self.prepare_family_plan_set_with_changes_inner(
            device, queue, frame, plans, changes, texts, fonts, geometries, metrics, None, None,
        )
    }

    /// Worker-boundary equivalent of the typed publication path. The installed
    /// mirror maintains the sparse active set while plans and frame state remain
    /// the same renderer inputs as direct execution.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_active_family_plan_set_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        active_indices: &std::collections::BTreeSet<usize>,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPlanSetPrepareError> {
        self.prepare_family_plan_set_with_changes_inner(
            device,
            queue,
            frame,
            plans,
            changes,
            texts,
            fonts,
            geometries,
            metrics,
            None,
            Some(active_indices),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_family_plan_set_with_changes_inner<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
        visible_object_indices: Option<&[usize]>,
        active_indices: Option<&std::collections::BTreeSet<usize>>,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPlanSetPrepareError> {
        let active_signature = active_indices
            .map(|indices| active_family_signature(frame, plans, indices))
            .transpose()?
            .filter(|signature| {
                signature.iter().all(|(index, _)| {
                    frame.retained.objects[*index].text().is_some()
                        && frame.family_animation(*index).is_some_and(|state| {
                            state.mode == noon_core::FamilyAnimationMode::DrawBorderThenFill
                        })
                })
            });
        if active_signature
            .as_ref()
            .is_some_and(|signature| *signature == self.family_plan_active_signature)
            && self.scratch_ready
            && !changes.is_all()
            && !changes.is_structural()
        {
            return self.prepare_cached_family_plan_set(
                device,
                queue,
                frame,
                plans,
                changes,
                texts,
                fonts,
                geometries,
                metrics,
                visible_object_indices,
            );
        }

        // A changed active set is the structural boundary for renderer-local glyph
        // rows. Rebuild the canonical baseline once; steady-state frames take the
        // sparse path above.
        if active_indices.is_some() {
            self.scratch_ready = false;
            self.family_plan_active_signature.clear();
            self.family_plan_scratch_slots.clear();
        }
        self.prepare_canonical_mixed_baseline(
            device,
            queue,
            frame.retained,
            changes,
            texts,
            fonts,
            geometries,
            metrics,
        )?;
        let canonical_scratch_len = self.scratch.objects.len();

        self.prepared_generation_ready = false;
        if let Err(error) = self.apply_family_plan_set_to_scratch(frame, plans, texts, fonts, active_indices.is_some()) {
            self.scratch_ready = false;
            return Err(error);
        }
        let cache_generation = active_signature.is_some();
        if let Some(signature) = active_signature {
            let active_by_object = signature
                .iter()
                .map(|(index, _)| (frame.retained.objects[*index].id, *index))
                .collect::<HashMap<_, _>>();
            for source in &self.sources {
                let SourceItem::Geometry {
                    object_id,
                    scratch_id,
                } = source
                else {
                    continue;
                };
                let scratch_slot = scratch_id.get() as usize;
                if scratch_slot < canonical_scratch_len {
                    continue;
                }
                if let Some(&object_index) = active_by_object.get(object_id) {
                    self.family_plan_scratch_slots
                        .entry(object_index)
                        .or_default()
                        .push(scratch_slot);
                }
            }
            self.family_plan_active_signature = signature;
            self.scratch_ready = true;
        }

        let geometry = self.geometry.prepare(&self.scratch);
        self.render_items.clear();
        rebuild_mixed_order(
            &mut self.render_items,
            &self.sources,
            &self.snapshot_text_items,
            &geometry,
        );
        rebuild_render_item_ranges(&mut self.render_item_ranges, &self.render_items);
        if let Some(indices) = visible_object_indices {
            if let Some(projected) = project_mixed_visibility_cached(
                frame.retained,
                indices,
                &self.render_items,
                &self.render_item_ranges,
                &mut self.visible_projection_ready,
                &mut self.visible_projection_candidates,
                &mut self.visible_render_items,
            ) {
                self.visibility_stats.record(indices.len(), projected);
            }
        }
        self.incremental_stats.mixed_order_rebuilds = self
            .incremental_stats
            .mixed_order_rebuilds
            .saturating_add(1);
        let glyph_batches = self
            .render_items
            .iter()
            .filter(|item| matches!(item, RetainedRenderItem::Glyph { .. }))
            .count();
        let outline_cache = self.outlines.stats();
        let stats = RetainedPrepareStats {
            semantic_objects: frame.retained.objects.len(),
            geometry_slots: self.scratch.objects.len(),
            glyph_batches,
            vector_items: self.snapshot_text_stats.vector_items,
            outline_runs: self.snapshot_text_stats.outline_runs,
            outline_cache_hits: outline_cache.hits,
            outline_cache_misses: outline_cache.misses,
        };
        self.snapshot_prepare_stats = stats;
        self.snapshot_metrics = Some(metrics);
        self.prepared_generation_ready = true;
        if !cache_generation {
            self.scratch_ready = false;
        }

        let text = PreparedRetainedTextSnapshot {
            time: frame.retained.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
            partial_upload_base_generation: None,
            dirty_mask_ranges: &self.dirty_mask_ranges,
            dirty_color_ranges: &self.dirty_color_ranges,
        };
        Ok(PreparedRetainedGpuFrame {
            applied_publication: &mut self.last_applied_publication,
            geometry_only: false,
            geometry,
            text_generation: self.text_generation,
            text,
            render_items: if visible_object_indices.is_some() {
                &self.visible_render_items
            } else {
                &self.render_items
            },
            stats,
            source_geometry_slots: None,
            render_item_ranges: None,
        })
    }

    fn apply_family_plan_set_to_scratch(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        stable_rows: bool,
    ) -> Result<(), RetainedFamilyPlanSetPrepareError> {
        if frame.family_animations.len() != frame.retained.objects.len()
            || frame.family_plan_indices.len() != frame.retained.objects.len()
        {
            return Err(RetainedPlannedFamilyFrameError::FrameShapeMismatch.into());
        }

        let family_frame = frame.as_family_frame();
        let baseline_sources = std::mem::take(&mut self.sources);
        self.sources.reserve(baseline_sources.len());

        for source in baseline_sources {
            match source {
                SourceItem::Geometry {
                    object_id,
                    scratch_id,
                } => {
                    let object_index = frame
                        .retained
                        .objects
                        .iter()
                        .position(|object| object.id == object_id)
                        .ok_or(RetainedFamilyPrepareError::MissingSourceObject(object_id))?;
                    let Some((state, plan)) = selected_family_plan(frame, plans, object_index)?
                    else {
                        self.sources.push(SourceItem::Geometry {
                            object_id,
                            scratch_id,
                        });
                        continue;
                    };

                    match state.mode {
                        noon_core::FamilyAnimationMode::Reveal => {
                            if let Some(reveal) = self.family_geometry_reveal(
                                &family_frame,
                                plan,
                                object_index,
                                object_id,
                            )? {
                                let scratch_index =
                                    usize::try_from(scratch_id.get()).map_err(|_| {
                                        RetainedFamilyPrepareError::MissingScratchObject(scratch_id)
                                    })?;
                                let target = self.scratch.reveals.get_mut(scratch_index).ok_or(
                                    RetainedFamilyPrepareError::MissingScratchObject(scratch_id),
                                )?;
                                *target = reveal;
                            }
                            self.sources.push(SourceItem::Geometry {
                                object_id,
                                scratch_id,
                            });
                        }
                        noon_core::FamilyAnimationMode::DrawBorderThenFill => {
                            return Err(
                                RetainedFamilyDrawBorderPrepareError::UnsupportedTextOutlineBaseline(
                                    object_id,
                                )
                                .into(),
                            );
                        }
                    }
                }
                SourceItem::FastGlyphRun {
                    object_id,
                    object_index,
                    run_index,
                } => {
                    let object_index_usize = object_index as usize;
                    let Some((state, plan)) =
                        selected_family_plan(frame, plans, object_index_usize)?
                    else {
                        self.sources.push(SourceItem::FastGlyphRun {
                            object_id,
                            object_index,
                            run_index,
                        });
                        continue;
                    };

                    match state.mode {
                        noon_core::FamilyAnimationMode::Reveal => {
                            if self.family_text_run_needs_outline(
                                &family_frame,
                                plan,
                                object_index_usize,
                                object_id,
                                run_index,
                            )? {
                                self.push_family_glyph_run(
                                    &family_frame,
                                    plan,
                                    object_index_usize,
                                    object_id,
                                    run_index,
                                    texts,
                                    fonts,
                                    )?;
                            } else {
                                self.sources.push(SourceItem::FastGlyphRun {
                                    object_id,
                                    object_index,
                                    run_index,
                                });
                            }
                        }
                        noon_core::FamilyAnimationMode::DrawBorderThenFill => {
                            if self.family_text_run_needs_draw_border_paths(
                                &family_frame,
                                plan,
                                object_index_usize,
                                object_id,
                                run_index,
                                stable_rows,
                            )? {
                                self.push_family_draw_border_glyph_run(
                                    &family_frame,
                                    plan,
                                    object_index_usize,
                                    object_id,
                                    run_index,
                                    texts,
                                    fonts,
                                    stable_rows,
                                )?;
                            } else {
                                self.sources.push(SourceItem::FastGlyphRun {
                                    object_id,
                                    object_index,
                                    run_index,
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_cached_family_plan_set<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
        visible_object_indices: Option<&[usize]>,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPlanSetPrepareError> {
        self.prepared_generation_ready = false;
        let partial_upload_base_generation = self.text_generation;
        if changes_include_text(frame.retained, changes) {
            let prepared_text = self.text.prepare_with_changes(
                device,
                queue,
                frame.retained,
                changes,
                texts,
                fonts,
                metrics,
            ).map_err(RetainedPrepareError::from)?;
            copy_local_text_snapshot_updates(
                &mut self.snapshot_mask_quads,
                &mut self.snapshot_color_quads,
                &self.text_item_ranges,
                prepared_text.items,
                prepared_text.mask_quads,
                prepared_text.color_quads,
                changes,
                &mut self.dirty_mask_ranges,
                &mut self.dirty_color_ranges,
            );
            self.incremental_stats.text_snapshot_copies = self
                .incremental_stats
                .text_snapshot_copies
                .saturating_add(1);
        } else {
            self.dirty_mask_ranges.clear();
            self.dirty_color_ranges.clear();
        }
        if !self.dirty_mask_ranges.is_empty() || !self.dirty_color_ranges.is_empty() {
            self.text_generation = self
                .text_generation
                .checked_add(1)
                .expect("retained text generation counter exhausted");
        }

        let active_indices = self
            .family_plan_active_signature
            .iter()
            .map(|(index, _)| *index)
            .collect::<std::collections::BTreeSet<_>>();
        let mut scratch_changes = Vec::new();
        for &index in changes.object_indices() {
            if active_indices.contains(&index) {
                continue;
            }
            let Some(object) = frame.retained.objects.get(index) else {
                continue;
            };
            if object.text().is_some() {
                continue;
            }
            let Some(scratch_slot) = self.scratch_slots.get(index).and_then(|slot| *slot) else {
                continue;
            };
            let source_geometry = frame
                .retained
                .render_geometry(index)
                .or_else(|| object.geometry())
                .ok_or(RetainedPrepareError::MissingGeometryResource)?;
            let geometry = resolve_geometry_ref(source_geometry, geometries)?;
            let scratch = &mut self.scratch.objects[scratch_slot];
            scratch.content = ObjectContentRef::Geometry(geometry);
            scratch.transform = frame.retained.render_transform(index);
            scratch.style = object.style;
            scratch.appearance = object.appearance;
            self.scratch.reveals[scratch_slot] = frame.retained.reveal(index);
            self.scratch.morphs[scratch_slot] = frame.retained.morph(index);
            scratch_changes.push(scratch_slot);
        }

        let signature = self.family_plan_active_signature.clone();
        let family_frame = frame.as_family_frame();
        for (object_index, plan_index) in signature {
            let object = &frame.retained.objects[object_index];
            let plan = &plans[plan_index as usize];
            let text =
                object
                    .text()
                    .ok_or(RetainedFamilyDrawBorderPrepareError::MissingSourceObject(
                        object.id,
                    ))?;
            let resource = texts
                .get(text)
                .ok_or(RetainedPrepareError::MissingTextResource)?;
            let scratch_start = self.scratch.objects.len();
            let source_start = self.sources.len();
            for item in resource.render_items.iter().copied() {
                if let TextRenderItem::GlyphRun(run_index) = item {
                    self.push_family_draw_border_glyph_run(
                        &family_frame,
                        plan,
                        object_index,
                        object.id,
                        run_index,
                        texts,
                        fonts,
                        true,
                    )?;
                }
            }

            let generated_objects = self.scratch.objects.split_off(scratch_start);
            let generated_presences = self.scratch.presences.split_off(scratch_start);
            let generated_reveals = self.scratch.reveals.split_off(scratch_start);
            let generated_morphs = self.scratch.morphs.split_off(scratch_start);
            let generated_geometries = self.scratch.render_geometries.split_off(scratch_start);
            let generated_transforms = self.scratch.render_transforms.split_off(scratch_start);
            self.sources.truncate(source_start);
            let slots = self
                .family_plan_scratch_slots
                .get(&object_index)
                .cloned()
                .unwrap_or_default();
            if slots.len() != generated_objects.len() {
                return Err(
                    RetainedFamilyPlanSetPrepareError::CachedScratchShapeChanged {
                        object: object.id,
                        expected: slots.len(),
                        actual: generated_objects.len(),
                    },
                );
            }
            for (generated_index, scratch_slot) in slots.into_iter().enumerate() {
                let mut generated = generated_objects[generated_index].clone();
                generated.id = ObjectId::new(scratch_slot as u64);
                self.scratch.objects[scratch_slot] = generated;
                self.scratch.presences[scratch_slot] = generated_presences[generated_index];
                self.scratch.reveals[scratch_slot] = generated_reveals[generated_index];
                self.scratch.morphs[scratch_slot] = generated_morphs[generated_index];
                self.scratch.render_geometries[scratch_slot] =
                    generated_geometries[generated_index].clone();
                self.scratch.render_transforms[scratch_slot] =
                    generated_transforms[generated_index];
                scratch_changes.push(scratch_slot);
            }
        }
        scratch_changes.sort_unstable();
        scratch_changes.dedup();
        self.scratch.time = frame.retained.time;
        self.incremental_stats.scratch_reuses =
            self.incremental_stats.scratch_reuses.saturating_add(1);
        let scratch_changes = FrameChanges::objects(scratch_changes);
        self.project_mixed_visibility(frame.retained, visible_object_indices);
        let geometry = self
            .geometry
            .prepare_incremental(&self.scratch, &scratch_changes);
        let outline_cache = self.outlines.stats();
        let stats = RetainedPrepareStats {
            outline_cache_hits: outline_cache.hits,
            outline_cache_misses: outline_cache.misses,
            ..self.snapshot_prepare_stats
        };
        self.snapshot_prepare_stats = stats;
        self.snapshot_metrics = Some(metrics);
        self.prepared_generation_ready = true;
        let text = PreparedRetainedTextSnapshot {
            time: frame.retained.time,
            mask_quads: &self.snapshot_mask_quads,
            color_quads: &self.snapshot_color_quads,
            items: &self.snapshot_text_items,
            stats: self.snapshot_text_stats,
            atlas: self.text.atlas(),
            partial_upload_base_generation: Some(partial_upload_base_generation),
            dirty_mask_ranges: &self.dirty_mask_ranges,
            dirty_color_ranges: &self.dirty_color_ranges,
        };
        Ok(PreparedRetainedGpuFrame {
            applied_publication: &mut self.last_applied_publication,
            geometry_only: false,
            geometry,
            text_generation: self.text_generation,
            text,
            render_items: if visible_object_indices.is_some() {
                &self.visible_render_items
            } else {
                &self.render_items
            },
            stats,
            source_geometry_slots: None,
            render_item_ranges: None,
        })
    }
}

fn active_family_signature(
    frame: &RetainedPlannedFamilyFrame<'_>,
    plans: &[RetainedFamilyAnimationPlan],
    active_indices: &std::collections::BTreeSet<usize>,
) -> Result<Vec<(usize, u32)>, RetainedPlannedFamilyFrameError> {
    active_indices
        .iter()
        .map(|&index| {
            let object = frame
                .retained
                .objects
                .get(index)
                .ok_or(RetainedPlannedFamilyFrameError::InvalidObjectIndex(index))?;
            let plan = frame
                .family_plan_index(index)
                .ok_or(RetainedPlannedFamilyFrameError::MissingPlanIndex(object.id))?;
            if plan as usize >= plans.len() {
                return Err(RetainedPlannedFamilyFrameError::InvalidPlanIndex {
                    object: object.id,
                    plan_index: plan,
                    plan_count: plans.len(),
                });
            }
            Ok((index, plan))
        })
        .collect()
}

fn selected_family_plan<'a>(
    frame: &RetainedPlannedFamilyFrame<'_>,
    plans: &'a [RetainedFamilyAnimationPlan],
    object_index: usize,
) -> Result<
    Option<(
        noon_core::FamilyAnimationState,
        &'a RetainedFamilyAnimationPlan,
    )>,
    RetainedPlannedFamilyFrameError,
> {
    let Some(state) = frame.family_animation(object_index) else {
        return Ok(None);
    };
    let object = frame.retained.objects.get(object_index).ok_or(
        RetainedPlannedFamilyFrameError::InvalidObjectIndex(object_index),
    )?;
    let plan_index = frame
        .family_plan_index(object_index)
        .ok_or(RetainedPlannedFamilyFrameError::MissingPlanIndex(object.id))?;
    let plan = plans.get(plan_index as usize).ok_or(
        RetainedPlannedFamilyFrameError::InvalidPlanIndex {
            object: object.id,
            plan_index,
            plan_count: plans.len(),
        },
    )?;
    if plan.leaf_for_object(object.id).is_none() {
        return Err(RetainedPlannedFamilyFrameError::PlanDoesNotOwnObject {
            object: object.id,
            plan_index,
        });
    }
    Ok(Some((state, plan)))
}

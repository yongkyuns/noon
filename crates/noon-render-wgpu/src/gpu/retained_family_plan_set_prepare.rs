use noon_runtime::{RetainedPlannedFamilyFrame, RetainedPlannedFamilyFrameError};

/// Failure while realizing multiple concurrently installed family plans.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyPlanSetPrepareError {
    Retained(RetainedPrepareError),
    PlannedFrame(RetainedPlannedFamilyFrameError),
    Reveal(RetainedFamilyPrepareError),
    DrawBorderThenFill(RetainedFamilyDrawBorderPrepareError),
}

impl std::fmt::Display for RetainedFamilyPlanSetPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::PlannedFrame(error) => error.fmt(formatter),
            Self::Reveal(error) => error.fmt(formatter),
            Self::DrawBorderThenFill(error) => error.fmt(formatter),
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

        self.prepared_generation_ready = false;
        if let Err(error) = self.apply_family_plan_set_to_scratch(frame, plans, texts, fonts) {
            self.scratch_ready = false;
            return Err(error);
        }

        let geometry = self.geometry.prepare(&self.scratch);
        self.render_items.clear();
        rebuild_mixed_order(
            &mut self.render_items,
            &self.sources,
            &self.snapshot_text_items,
            &geometry,
        );
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
        self.scratch_ready = false;

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
            render_items: &self.render_items,
            stats,
        })
    }

    fn apply_family_plan_set_to_scratch(
        &mut self,
        frame: &RetainedPlannedFamilyFrame<'_>,
        plans: &[RetainedFamilyAnimationPlan],
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
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
                            )? {
                                self.push_family_draw_border_glyph_run(
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
                    }
                }
            }
        }
        Ok(())
    }
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

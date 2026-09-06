use super::super::{
    retained_family_draw_border_then_fill_members_for_object, RetainedDrawBorderThenFillPhase,
    RetainedFamilyDrawBorderThenFillError,
};

/// ManimCE Cairo's default `DrawBorderThenFill(stroke_width=2)` in Noon's
/// screen-space scene units. The public Python compatibility layer uses the same
/// 0.01 line-width conversion for authored VMobject strokes.
const DEFAULT_DRAW_BORDER_STROKE_WIDTH: f32 = 0.02;

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyDrawBorderPrepareError {
    Retained(RetainedPrepareError),
    Family(RetainedFamilyDrawBorderThenFillError),
    MissingSourceObject(ObjectId),
    InvalidTextRun {
        object: ObjectId,
        run_index: u32,
    },
    InvalidTextGlyph {
        object: ObjectId,
        glyph: TextAnimationGlyphRef,
    },
    UnexpectedGeometryMember(ObjectId),
    UnsupportedTextOutlineBaseline(ObjectId),
}

impl std::fmt::Display for RetainedFamilyDrawBorderPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
            Self::MissingSourceObject(object) => write!(
                formatter,
                "retained DrawBorderThenFill preparation cannot resolve source object {}",
                object.get()
            ),
            Self::InvalidTextRun { object, run_index } => write!(
                formatter,
                "retained DrawBorderThenFill preparation references missing Text run {run_index} on object {}",
                object.get()
            ),
            Self::InvalidTextGlyph { object, glyph } => write!(
                formatter,
                "retained DrawBorderThenFill preparation references missing Text glyph {}:{} on object {}",
                glyph.run_index,
                glyph.glyph_index,
                object.get()
            ),
            Self::UnexpectedGeometryMember(object) => write!(
                formatter,
                "retained Text object {} resolved a geometry DrawBorderThenFill member",
                object.get()
            ),
            Self::UnsupportedTextOutlineBaseline(object) => write!(
                formatter,
                "retained Text object {} requires an object-level outline/morph/vector baseline that cannot be combined with glyph-local DrawBorderThenFill",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyDrawBorderPrepareError {}

impl From<RetainedPrepareError> for RetainedFamilyDrawBorderPrepareError {
    fn from(value: RetainedPrepareError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedFamilyDrawBorderThenFillError> for RetainedFamilyDrawBorderPrepareError {
    fn from(value: RetainedFamilyDrawBorderThenFillError) -> Self {
        Self::Family(value)
    }
}

impl RetainedFramePreparer {
    /// Prepare a retained frame whose semantic family uses DrawBorderThenFill.
    ///
    /// Family traversal, member order, lag, easing, and reversal are already resolved
    /// by the runtime state. This renderer layer only realizes each Text glyph's local
    /// outline/fill phase. Completed runs stay on the atlas fast path; only runs with
    /// an in-flight member are materialized through the existing glyph-outline cache.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_draw_border_then_fill<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyDrawBorderPrepareError> {
        let changes = FrameChanges::all();
        self.prepare_family_draw_border_then_fill_with_changes(
            device, queue, frame, plan, &changes, texts, fonts, geometries, metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_draw_border_then_fill_with_changes<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        changes: &FrameChanges,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyDrawBorderPrepareError> {
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

        // The canonical generation is no longer reusable once operation-local scratch
        // substitution begins. Restore validity only after the mixed frame is complete.
        self.prepared_generation_ready = false;
        if let Err(error) =
            self.apply_family_draw_border_then_fill_to_scratch(frame, plan, texts, fonts)
        {
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

        // Family-local scratch is not a canonical retained-frame baseline. Rebuild it
        // before the next ordinary or family incremental preparation.
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
            source_geometry_slots: None,
            render_item_ranges: None,
        })
    }

    fn apply_family_draw_border_then_fill_to_scratch(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedFamilyDrawBorderPrepareError> {
        let baseline_sources = mem::take(&mut self.sources);
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
                        .ok_or(RetainedFamilyDrawBorderPrepareError::MissingSourceObject(
                            object_id,
                        ))?;
                    if let Some(mut members) =
                        retained_family_draw_border_then_fill_members_for_object(
                            frame,
                            plan,
                            object_index,
                        )?
                    {
                        if let Some(member) = members.next() {
                            match member {
                                Ok(_) => {
                                    return Err(RetainedFamilyDrawBorderPrepareError::
                                        UnsupportedTextOutlineBaseline(object_id));
                                }
                                Err(error) => return Err(error.into()),
                            }
                        }
                    }
                    self.sources.push(SourceItem::Geometry {
                        object_id,
                        scratch_id,
                    });
                }
                SourceItem::FastGlyphRun {
                    object_id,
                    object_index,
                    run_index,
                } => {
                    let object_index_usize = object_index as usize;
                    if self.family_text_run_needs_draw_border_paths(
                        frame,
                        plan,
                        object_index_usize,
                        object_id,
                        run_index,
                    )? {
                        self.push_family_draw_border_glyph_run(
                            frame,
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
        Ok(())
    }

    fn family_text_run_needs_draw_border_paths(
        &self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        object_index: usize,
        object: ObjectId,
        run_index: u32,
    ) -> Result<bool, RetainedFamilyDrawBorderPrepareError> {
        let Some(members) =
            retained_family_draw_border_then_fill_members_for_object(frame, plan, object_index)?
        else {
            return Ok(false);
        };
        for member in members {
            let member = member?;
            if member.glyph.run_index == run_index && !draw_border_phase_is_final(member.phase) {
                return Ok(true);
            }
            if member.object != object {
                return Err(RetainedFamilyDrawBorderPrepareError::UnexpectedGeometryMember(object));
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_family_draw_border_glyph_run(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        object_index: usize,
        object_id: ObjectId,
        run_index: u32,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedFamilyDrawBorderPrepareError> {
        let object = frame.retained.objects.get(object_index).ok_or(
            RetainedFamilyDrawBorderPrepareError::MissingSourceObject(object_id),
        )?;
        let text =
            object
                .text()
                .ok_or(RetainedFamilyDrawBorderPrepareError::MissingSourceObject(
                    object_id,
                ))?;
        let resource = texts
            .get(text)
            .ok_or(RetainedPrepareError::MissingTextResource)?;
        let run = resource.runs.get(run_index as usize).ok_or(
            RetainedFamilyDrawBorderPrepareError::InvalidTextRun {
                object: object_id,
                run_index,
            },
        )?;

        let Some(members) =
            retained_family_draw_border_then_fill_members_for_object(frame, plan, object_index)?
        else {
            return Ok(());
        };
        for member in members {
            let member = member?;
            if member.glyph.run_index != run_index {
                continue;
            }
            let reveal = match member.phase {
                RetainedDrawBorderThenFillPhase::Outline { reveal } if reveal <= 0.0 => continue,
                RetainedDrawBorderThenFillPhase::Outline { reveal } => reveal,
                RetainedDrawBorderThenFillPhase::Fill { .. } => 1.0,
            };
            let positioned = run.glyphs.get(member.glyph.glyph_index as usize).ok_or(
                RetainedFamilyDrawBorderPrepareError::InvalidTextGlyph {
                    object: object_id,
                    glyph: member.glyph,
                },
            )?;
            let (_, outline) = self.outlines.outline(fonts, run, positioned.glyph_id)?;
            let path = append_transformed_path(
                VectorPath::new(),
                outline.as_ref(),
                run.transform,
                positioned.origin,
            );
            if path.is_empty() {
                continue;
            }
            self.push_geometry(
                object_id,
                GeometryRef::VectorPath(path),
                object.transform,
                draw_border_glyph_style(run, object.style, member.phase),
                object.appearance,
                reveal,
                0.0,
            );
        }
        Ok(())
    }
}

fn draw_border_phase_is_final(phase: RetainedDrawBorderThenFillPhase) -> bool {
    matches!(phase, RetainedDrawBorderThenFillPhase::Fill { progress } if progress >= 1.0)
}

fn draw_border_glyph_style(
    run: &GlyphRun,
    object_style: Style,
    phase: RetainedDrawBorderThenFillPhase,
) -> Style {
    let final_fill = run.fill.or(object_style.fill).unwrap_or(Color::WHITE);
    let final_stroke = object_style
        .stroke
        .filter(|_| object_style.stroke_width > 0.0);
    let outline_color = final_stroke.unwrap_or(final_fill);

    match phase {
        RetainedDrawBorderThenFillPhase::Outline { .. } => Style {
            fill: None,
            stroke: Some(outline_color),
            stroke_width: DEFAULT_DRAW_BORDER_STROKE_WIDTH,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke_join: StrokeJoin::Miter,
            stroke_cap: StrokeCap::Butt,
            opacity: object_style.opacity,
        },
        RetainedDrawBorderThenFillPhase::Fill { progress } => {
            let progress = progress.clamp(0.0, 1.0);
            let fill = Some(scale_color_alpha(final_fill, progress));
            let (stroke, stroke_width, stroke_width_mode, stroke_join, stroke_cap) =
                if let Some(final_stroke) = final_stroke {
                    (
                        Some(interpolate_color(outline_color, final_stroke, progress)),
                        interpolate_scalar(
                            DEFAULT_DRAW_BORDER_STROKE_WIDTH,
                            object_style.stroke_width,
                            progress,
                        ),
                        if progress >= 1.0 {
                            object_style.stroke_width_mode
                        } else {
                            StrokeWidthMode::ScreenSpace
                        },
                        if progress >= 1.0 {
                            object_style.stroke_join
                        } else {
                            StrokeJoin::Miter
                        },
                        if progress >= 1.0 {
                            object_style.stroke_cap
                        } else {
                            StrokeCap::Butt
                        },
                    )
                } else {
                    let remaining = 1.0 - progress;
                    (
                        (remaining > 0.0).then(|| scale_color_alpha(outline_color, remaining)),
                        DEFAULT_DRAW_BORDER_STROKE_WIDTH * remaining,
                        StrokeWidthMode::ScreenSpace,
                        StrokeJoin::Miter,
                        StrokeCap::Butt,
                    )
                };
            Style {
                fill,
                stroke,
                stroke_width,
                stroke_width_mode,
                stroke_join,
                stroke_cap,
                opacity: object_style.opacity,
            }
        }
    }
}

fn interpolate_scalar(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn interpolate_color(from: Color, to: Color, progress: f32) -> Color {
    Color::rgba(
        interpolate_scalar(from.red, to.red, progress),
        interpolate_scalar(from.green, to.green, progress),
        interpolate_scalar(from.blue, to.blue, progress),
        interpolate_scalar(from.alpha, to.alpha, progress),
    )
}

fn scale_color_alpha(color: Color, scale: f32) -> Color {
    Color::rgba(color.red, color.green, color.blue, color.alpha * scale)
}

#[cfg(test)]
mod draw_border_tests {
    use std::sync::Arc;

    use noon_core::{
        FamilyAnimationMode, FamilyAnimationState, FontFaceIdentity, GlyphRun, ObjectContentRef,
        PositionedGlyph, RateFunction, Rect, RetainedFamilyAnimationPlanBuilder,
        RetainedObjectDefinition, SemanticStore, TextAffineTransform, TextClusterIdentity,
        TextDirection, TextRenderItem, TextResource, TextSourceKind, TextSourceSpan,
    };
    use noon_runtime::{FrameObjectState, FrameState};

    use super::*;

    fn state(progress: f32) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::DrawBorderThenFill,
            overall_progress: f64::from(progress),
            lag_ratio: 0.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn glyph(span: TextSourceSpan, glyph_id: u32, x: f32) -> PositionedGlyph {
        PositionedGlyph {
            glyph_id,
            cluster: TextClusterIdentity {
                source_span: span,
                cluster_ordinal: glyph_id,
                semantic_key: None,
            },
            origin: Vec2::new(x, 0.0),
            advance: Vec2::new(1.0, 0.0),
            bounds: Rect::new(Vec2::new(x, 0.0), Vec2::new(x + 1.0, 1.0)),
        }
    }

    fn test_run() -> GlyphRun {
        GlyphRun {
            font: FontFaceIdentity {
                family: Arc::from("Test"),
                face_key: Arc::from("test-face"),
                face_index: 0,
                variation_key: Arc::from(""),
            },
            variations: Arc::from([]),
            font_size: 24.0,
            direction: TextDirection::LeftToRight,
            fill: None,
            stroke: None,
            transform: TextAffineTransform::IDENTITY,
            glyphs: Arc::from([
                glyph(TextSourceSpan::new(0, 1), 1, 0.0),
                glyph(TextSourceSpan::new(1, 2), 2, 1.0),
            ]),
        }
    }

    fn text_fixture(
        progress: f32,
    ) -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
        TextResourceArena,
    ) {
        let resource = TextResource {
            source: Arc::from("AB"),
            kind: TextSourceKind::Plain,
            runs: Arc::from([test_run()]),
            vector_items: Arc::from([]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
            baseline: 0.0,
            layout_artifact: None,
        };
        let mut texts = TextResourceArena::new();
        let text = texts.insert(resource).unwrap();
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let object = RetainedObjectDefinition::text(ObjectId::new(20), text);
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, leaf).unwrap();
        builder.accept_leaf(leaf, &object, &texts).unwrap();
        let plan = builder.finish().unwrap();
        let frame = FrameState {
            time: 1.0,
            objects: vec![FrameObjectState {
                id: ObjectId::new(20),
                content: ObjectContentRef::Text(text),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            }],
            presences: vec![true],
            reveals: vec![1.0],
            morphs: vec![0.0],
            render_geometries: vec![None],
            render_transforms: vec![None],
        };
        (plan, frame, vec![Some(state(progress))], texts)
    }

    #[test]
    fn hidden_draw_border_text_removes_atlas_source_without_font_work() {
        let (plan, retained, states, texts) = text_fixture(0.0);
        let family = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let mut preparer = RetainedFramePreparer::default();
        preparer
            .build_scratch_frame(
                &retained,
                &texts,
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();
        assert!(matches!(
            preparer.sources.as_slice(),
            [SourceItem::FastGlyphRun { .. }]
        ));
        preparer
            .apply_family_draw_border_then_fill_to_scratch(
                &family,
                &plan,
                &texts,
                &FontResourceArena::new(),
            )
            .unwrap();
        assert!(preparer.sources.is_empty());
        assert!(preparer.scratch.objects.is_empty());
        assert_eq!(preparer.outline_cache_stats().misses, 0);
    }

    #[test]
    fn completed_draw_border_text_keeps_atlas_fast_path() {
        let (plan, retained, states, texts) = text_fixture(1.0);
        let family = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let mut preparer = RetainedFramePreparer::default();
        preparer
            .build_scratch_frame(
                &retained,
                &texts,
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();
        preparer
            .apply_family_draw_border_then_fill_to_scratch(
                &family,
                &plan,
                &texts,
                &FontResourceArena::new(),
            )
            .unwrap();
        assert!(matches!(
            preparer.sources.as_slice(),
            [SourceItem::FastGlyphRun { .. }]
        ));
        assert!(preparer.scratch.objects.is_empty());
        assert_eq!(preparer.outline_cache_stats().misses, 0);
    }

    #[test]
    fn outline_phase_uses_manim_default_border_presentation() {
        let run = test_run();
        let style = draw_border_glyph_style(
            &run,
            Style::default(),
            RetainedDrawBorderThenFillPhase::Outline { reveal: 0.5 },
        );
        assert_eq!(style.fill, None);
        assert_eq!(style.stroke, Some(Color::WHITE));
        assert_eq!(style.stroke_width, DEFAULT_DRAW_BORDER_STROKE_WIDTH);
        assert_eq!(style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
        assert_eq!(style.stroke_join, StrokeJoin::Miter);
        assert_eq!(style.stroke_cap, StrokeCap::Butt);
    }

    #[test]
    fn fill_phase_interpolates_outline_into_final_text_style() {
        let run = test_run();
        let style = draw_border_glyph_style(
            &run,
            Style::default(),
            RetainedDrawBorderThenFillPhase::Fill { progress: 0.5 },
        );
        assert_eq!(style.fill, Some(Color::rgba(1.0, 1.0, 1.0, 0.5)));
        assert_eq!(style.stroke, Some(Color::rgba(1.0, 1.0, 1.0, 0.5)));
        assert_eq!(style.stroke_width, DEFAULT_DRAW_BORDER_STROKE_WIDTH * 0.5);
        assert_eq!(style.stroke_width_mode, StrokeWidthMode::ScreenSpace);
    }
}

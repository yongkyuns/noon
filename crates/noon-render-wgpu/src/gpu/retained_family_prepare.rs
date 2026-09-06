use std::mem;

use noon_core::{RetainedFamilyAnimationPlan, TextAnimationGlyphRef};
use noon_runtime::RetainedFamilyFrame;

use super::super::{
    retained_family_reveal_members_for_object, RetainedFamilyRevealError,
    RetainedFamilyRevealMember,
};
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyPrepareError {
    Retained(RetainedPrepareError),
    Family(RetainedFamilyRevealError),
    MissingSourceObject(ObjectId),
    MissingScratchObject(ObjectId),
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

impl std::fmt::Display for RetainedFamilyPrepareError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retained(error) => error.fmt(formatter),
            Self::Family(error) => error.fmt(formatter),
            Self::MissingSourceObject(object) => write!(
                formatter,
                "retained family preparation cannot resolve source object {}",
                object.get()
            ),
            Self::MissingScratchObject(object) => write!(
                formatter,
                "retained family preparation cannot resolve scratch object {}",
                object.get()
            ),
            Self::InvalidTextRun { object, run_index } => write!(
                formatter,
                "retained family preparation references missing Text run {run_index} on object {}",
                object.get()
            ),
            Self::InvalidTextGlyph { object, glyph } => write!(
                formatter,
                "retained family preparation references missing Text glyph {}:{} on object {}",
                glyph.run_index,
                glyph.glyph_index,
                object.get()
            ),
            Self::UnexpectedGeometryMember(object) => write!(
                formatter,
                "retained Text object {} resolved a geometry family member",
                object.get()
            ),
            Self::UnsupportedTextOutlineBaseline(object) => write!(
                formatter,
                "retained Text object {} requires an object-level outline/morph baseline that cannot be combined with glyph-local family reveal",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyPrepareError {}

impl From<RetainedPrepareError> for RetainedFamilyPrepareError {
    fn from(value: RetainedPrepareError) -> Self {
        Self::Retained(value)
    }
}

impl From<RetainedFamilyRevealError> for RetainedFamilyPrepareError {
    fn from(value: RetainedFamilyRevealError) -> Self {
        Self::Family(value)
    }
}

impl RetainedFramePreparer {
    /// Prepare a retained frame with one already-prepared semantic family plan.
    ///
    /// The ordinary retained preparer first establishes its canonical frame/resource
    /// state. Family realization then changes only renderer-local scratch state:
    /// ordinary geometry receives member 0's global reveal progress, while partial
    /// Text runs replace their atlas painter item with individual glyph outlines from
    /// the existing outline cache. Family timing and semantic traversal stay outside
    /// this layer.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family<'a>(
        &'a mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        metrics: TextDeviceMetrics,
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPrepareError> {
        let changes = FrameChanges::all();
        self.prepare_family_with_changes(
            device, queue, frame, plan, &changes, texts, fonts, geometries, metrics,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_family_with_changes<'a>(
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
    ) -> Result<PreparedRetainedGpuFrame<'a>, RetainedFamilyPrepareError> {
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

        // The canonical generation is no longer reusable once family-local scratch
        // substitution begins. Restore validity only after the complete mixed frame is
        // rebuilt; errors therefore cannot expose a partially substituted generation.
        self.prepared_generation_ready = false;
        if let Err(error) = self.apply_family_reveal_to_scratch(frame, plan, texts, fonts) {
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

        // `self.scratch` now represents a family-local renderer view rather than the
        // canonical retained frame. Force the next ordinary/family preparation to
        // rebuild that canonical baseline before applying incremental assumptions.
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

    fn apply_family_reveal_to_scratch(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedFamilyPrepareError> {
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
                        .ok_or(RetainedFamilyPrepareError::MissingSourceObject(object_id))?;
                    if let Some(reveal) =
                        self.family_geometry_reveal(frame, plan, object_index, object_id)?
                    {
                        let scratch_index = usize::try_from(scratch_id.get()).map_err(|_| {
                            RetainedFamilyPrepareError::MissingScratchObject(scratch_id)
                        })?;
                        let target =
                            self.scratch.reveals.get_mut(scratch_index).ok_or(
                                RetainedFamilyPrepareError::MissingScratchObject(scratch_id),
                            )?;
                        *target = reveal;
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
                    if self.family_text_run_needs_outline(
                        frame,
                        plan,
                        object_index_usize,
                        object_id,
                        run_index,
                    )? {
                        self.push_family_glyph_run(
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

    fn family_geometry_reveal(
        &self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        object_index: usize,
        object: ObjectId,
    ) -> Result<Option<f32>, RetainedFamilyPrepareError> {
        let Some(mut members) =
            retained_family_reveal_members_for_object(frame, plan, object_index)?
        else {
            return Ok(None);
        };
        let Some(member) = members.next() else {
            return Ok(None);
        };
        match member? {
            RetainedFamilyRevealMember::Geometry { reveal, .. } => {
                if members.next().is_some() {
                    return Err(RetainedFamilyPrepareError::UnexpectedGeometryMember(object));
                }
                Ok(Some(reveal))
            }
            RetainedFamilyRevealMember::TextGlyph { .. } => Err(
                RetainedFamilyPrepareError::UnsupportedTextOutlineBaseline(object),
            ),
        }
    }

    fn family_text_run_needs_outline(
        &self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        object_index: usize,
        object: ObjectId,
        run_index: u32,
    ) -> Result<bool, RetainedFamilyPrepareError> {
        let Some(members) = retained_family_reveal_members_for_object(frame, plan, object_index)?
        else {
            return Ok(false);
        };
        for member in members {
            match member? {
                RetainedFamilyRevealMember::TextGlyph { glyph, reveal, .. }
                    if glyph.run_index == run_index && reveal < 1.0 =>
                {
                    return Ok(true);
                }
                RetainedFamilyRevealMember::TextGlyph { .. } => {}
                RetainedFamilyRevealMember::Geometry { .. } => {
                    return Err(RetainedFamilyPrepareError::UnexpectedGeometryMember(object));
                }
            }
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_family_glyph_run(
        &mut self,
        frame: &RetainedFamilyFrame<'_>,
        plan: &RetainedFamilyAnimationPlan,
        object_index: usize,
        object_id: ObjectId,
        run_index: u32,
        texts: &(impl TextResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<(), RetainedFamilyPrepareError> {
        let object = frame
            .retained
            .objects
            .get(object_index)
            .ok_or(RetainedFamilyPrepareError::MissingSourceObject(object_id))?;
        let text = object
            .text()
            .ok_or(RetainedFamilyPrepareError::MissingSourceObject(object_id))?;
        let resource = texts
            .get(text)
            .ok_or(RetainedPrepareError::MissingTextResource)?;
        let run = resource.runs.get(run_index as usize).ok_or(
            RetainedFamilyPrepareError::InvalidTextRun {
                object: object_id,
                run_index,
            },
        )?;

        let Some(members) = retained_family_reveal_members_for_object(frame, plan, object_index)?
        else {
            return Ok(());
        };
        for member in members {
            match member? {
                RetainedFamilyRevealMember::TextGlyph { glyph, reveal, .. }
                    if glyph.run_index == run_index =>
                {
                    if reveal <= 0.0 {
                        continue;
                    }
                    let positioned = run.glyphs.get(glyph.glyph_index as usize).ok_or(
                        RetainedFamilyPrepareError::InvalidTextGlyph {
                            object: object_id,
                            glyph,
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
                    let fill = run.fill.or(object.style.fill).unwrap_or(Color::WHITE);
                    self.push_geometry(
                        object_id,
                        GeometryRef::VectorPath(path),
                        object.transform,
                        Style {
                            fill: Some(fill),
                            stroke: None,
                            stroke_width: 0.0,
                            stroke_width_mode: StrokeWidthMode::ScaleWithObject,
                            stroke_join: StrokeJoin::Round,
                            stroke_cap: StrokeCap::Round,
                            opacity: object.style.opacity,
                        },
                        object.appearance,
                        reveal,
                        0.0,
                    );
                }
                RetainedFamilyRevealMember::TextGlyph { .. } => {}
                RetainedFamilyRevealMember::Geometry { .. } => {
                    return Err(RetainedFamilyPrepareError::UnexpectedGeometryMember(
                        object_id,
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
            mode: FamilyAnimationMode::Reveal,
            overall_progress: f64::from(progress),
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn geometry_fixture() -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, first).unwrap();
        store.add_member(family, second).unwrap();
        let first_object =
            RetainedObjectDefinition::geometry(ObjectId::new(10), GeometryRef::circle(1.0));
        let second_object =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(2.0));
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder.accept_leaf(first, &first_object, &texts).unwrap();
        builder.accept_leaf(second, &second_object, &texts).unwrap();
        let plan = builder.finish().unwrap();
        let frame = FrameState {
            time: 1.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(10),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    text_bounds: None,
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        };
        (plan, frame, vec![Some(state(0.5)), Some(state(0.5))])
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
            runs: Arc::from([GlyphRun {
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
            }]),
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
    fn family_geometry_progress_overrides_legacy_scalar_reveal() {
        let (plan, retained, states) = geometry_fixture();
        let family = RetainedFamilyFrame {
            retained: &retained,
            family_animations: &states,
        };
        let mut preparer = RetainedFramePreparer::default();
        preparer
            .build_scratch_frame(
                &retained,
                &TextResourceArena::new(),
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();
        assert_eq!(preparer.scratch.reveals, vec![1.0, 1.0]);
        preparer
            .apply_family_reveal_to_scratch(
                &family,
                &plan,
                &TextResourceArena::new(),
                &FontResourceArena::new(),
            )
            .unwrap();
        assert_eq!(preparer.scratch.reveals, vec![1.0, 0.0]);
    }

    #[test]
    fn hidden_family_text_removes_atlas_source_without_font_work() {
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
            .apply_family_reveal_to_scratch(&family, &plan, &texts, &FontResourceArena::new())
            .unwrap();
        assert!(preparer.sources.is_empty());
        assert!(preparer.scratch.objects.is_empty());
    }

    #[test]
    fn completed_family_text_keeps_the_atlas_fast_path() {
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
            .apply_family_reveal_to_scratch(&family, &plan, &texts, &FontResourceArena::new())
            .unwrap();
        assert!(matches!(
            preparer.sources.as_slice(),
            [SourceItem::FastGlyphRun { .. }]
        ));
        assert!(preparer.scratch.objects.is_empty());
    }
}

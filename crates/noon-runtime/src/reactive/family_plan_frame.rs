use noon_core::{
    RetainedFamilyAnimationEvaluationError, RetainedFamilyAnimationLeafFrame,
    RetainedFamilyAnimationPlan,
};

use crate::RetainedFamilyFrame;

/// Failure while binding one runtime family-animation slot to a prepared retained leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyFramePlanError {
    ObjectIndexOutOfBounds { index: usize, object_count: usize },
    Evaluation(RetainedFamilyAnimationEvaluationError),
}

impl std::fmt::Display for RetainedFamilyFramePlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ObjectIndexOutOfBounds {
                index,
                object_count,
            } => write!(
                formatter,
                "retained family frame object index {index} is outside object count {object_count}"
            ),
            Self::Evaluation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyFramePlanError {}

impl From<RetainedFamilyAnimationEvaluationError> for RetainedFamilyFramePlanError {
    fn from(value: RetainedFamilyAnimationEvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

impl RetainedFamilyFrame<'_> {
    /// Bind one retained runtime object slot to an already prepared family member plan.
    ///
    /// The runtime owns only object-local animation state. The plan owns semantic family
    /// order and global member spans. Joining them here keeps renderer preparation free of
    /// semantic traversal, timing recomputation, and retained-resource inspection.
    ///
    /// `Ok(None)` means either the object is not part of this plan or this plan member is
    /// not currently animated. This allows multiple prepared plans to coexist without
    /// treating unrelated active family animations as errors.
    pub fn planned_family_leaf<'plan>(
        &self,
        plan: &'plan RetainedFamilyAnimationPlan,
        object_index: usize,
    ) -> Result<Option<RetainedFamilyAnimationLeafFrame<'plan>>, RetainedFamilyFramePlanError> {
        let object = self.retained.objects.get(object_index).ok_or(
            RetainedFamilyFramePlanError::ObjectIndexOutOfBounds {
                index: object_index,
                object_count: self.retained.objects.len(),
            },
        )?;

        if plan.leaf_for_object(object.id).is_none() {
            return Ok(None);
        }
        let Some(state) = self.family_animation(object_index) else {
            return Ok(None);
        };

        Ok(Some(plan.leaf_frame_for_object(state, object.id)?))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FamilyAnimationMode, FamilyAnimationState, FontFaceIdentity, GeometryRef, GlyphRun,
        ObjectContentRef, ObjectId, PositionedGlyph, RateFunction, Rect,
        RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition, SemanticStore, Style,
        TextAffineTransform, TextClusterIdentity, TextDirection, TextRenderItem, TextResource,
        TextResourceArena, TextSourceKind, TextSourceSpan, Transform2D, Vec2,
    };

    use crate::{FrameObjectState, FrameState};

    use super::*;

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

    fn text_resource() -> TextResource {
        TextResource {
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
        }
    }

    fn state() -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: 0.5,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order: false,
        }
    }

    fn mixed_fixture() -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let mut store = SemanticStore::new();
        let text_leaf = store.insert_authoring_object();
        let circle_leaf = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, text_leaf).unwrap();
        store.add_member(family, circle_leaf).unwrap();

        let mut texts = TextResourceArena::new();
        let text_handle = texts.insert(text_resource()).unwrap();
        let text = RetainedObjectDefinition::text(ObjectId::new(10), text_handle);
        let circle =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(1.0));
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder.accept_leaf(text_leaf, &text, &texts).unwrap();
        builder.accept_leaf(circle_leaf, &circle, &texts).unwrap();
        let plan = builder.finish().unwrap();

        let frame = FrameState {
            time: 1.0,
            objects: vec![
                FrameObjectState {
                    id: ObjectId::new(10),
                    content: ObjectContentRef::Text(text_handle),
                    text_bounds: Some(Rect::new(Vec2::ZERO, Vec2::ONE)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                FrameObjectState {
                    id: ObjectId::new(11),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
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
        (plan, frame, vec![Some(state()), Some(state())])
    }

    #[test]
    fn runtime_slots_bind_to_one_global_mixed_family_timeline() {
        let (plan, frame, states) = mixed_fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };

        let text = family.planned_family_leaf(&plan, 0).unwrap().unwrap();
        let circle = family.planned_family_leaf(&plan, 1).unwrap().unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }

    #[test]
    fn inactive_or_unrelated_slots_do_not_force_plan_errors() {
        let (plan, mut frame, mut states) = mixed_fixture();
        states[0] = None;
        frame.objects.push(FrameObjectState {
            id: ObjectId::new(99),
            content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
        });
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);
        states.push(Some(state()));
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };

        assert!(family.planned_family_leaf(&plan, 0).unwrap().is_none());
        assert!(family.planned_family_leaf(&plan, 2).unwrap().is_none());
    }

    #[test]
    fn invalid_object_index_fails_closed() {
        let (plan, frame, states) = mixed_fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        assert_eq!(
            family.planned_family_leaf(&plan, 2).unwrap_err(),
            RetainedFamilyFramePlanError::ObjectIndexOutOfBounds {
                index: 2,
                object_count: 2,
            }
        );
    }
}

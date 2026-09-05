use noon_core::{
    FamilyAnimationMode, ObjectId, RetainedAnimationMember, RetainedFamilyAnimationEvaluationError,
    RetainedFamilyAnimationLeafFrame, RetainedFamilyAnimationPlan, TextAnimationGlyphRef,
};
use noon_runtime::{RetainedFamilyFrame, RetainedFamilyFramePlanError};

/// Renderer-local phase for one already-scheduled DrawBorderThenFill family member.
///
/// Manim's `integer_interpolate(0, 2, alpha)` splits one member into an outline-reveal
/// half followed by an outline-to-final-style half. Family lag/easing is deliberately
/// resolved before this boundary; this type only tells the renderer how to realize the
/// resulting member-local progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedDrawBorderThenFillPhase {
    Outline { reveal: f32 },
    Fill { progress: f32 },
}

impl RetainedDrawBorderThenFillPhase {
    pub fn from_member_progress(progress: f32) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        if progress < 0.5 {
            Self::Outline {
                reveal: progress * 2.0,
            }
        } else {
            Self::Fill {
                progress: progress * 2.0 - 1.0,
            }
        }
    }
}

/// One Text-glyph realization command for DrawBorderThenFill.
///
/// Glyph identity is renderer-local and derived from the immutable retained Text
/// resource. It is never added to the semantic family request or execution wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedFamilyDrawBorderThenFillMember {
    pub object: ObjectId,
    pub glyph: TextAnimationGlyphRef,
    pub phase: RetainedDrawBorderThenFillPhase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyDrawBorderThenFillError {
    UnsupportedMode(FamilyAnimationMode),
    UnsupportedGeometry(ObjectId),
    MissingPreparedMember { object: ObjectId, local_member: u32 },
    FramePlan(RetainedFamilyFramePlanError),
    Evaluation(RetainedFamilyAnimationEvaluationError),
}

impl std::fmt::Display for RetainedFamilyDrawBorderThenFillError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMode(mode) => write!(
                formatter,
                "family DrawBorderThenFill realization does not support {mode:?}"
            ),
            Self::UnsupportedGeometry(object) => write!(
                formatter,
                "retained geometry object {} does not yet support family DrawBorderThenFill realization",
                object.get()
            ),
            Self::MissingPreparedMember {
                object,
                local_member,
            } => write!(
                formatter,
                "retained object {} has no prepared family member {local_member}",
                object.get()
            ),
            Self::FramePlan(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyDrawBorderThenFillError {}

impl From<RetainedFamilyFramePlanError> for RetainedFamilyDrawBorderThenFillError {
    fn from(value: RetainedFamilyFramePlanError) -> Self {
        Self::FramePlan(value)
    }
}

impl From<RetainedFamilyAnimationEvaluationError> for RetainedFamilyDrawBorderThenFillError {
    fn from(value: RetainedFamilyAnimationEvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Zero-allocation iterator over DrawBorderThenFill realization for one prepared leaf.
#[derive(Clone, Copy, Debug)]
pub struct RetainedFamilyDrawBorderThenFillMembers<'a> {
    frame: RetainedFamilyAnimationLeafFrame<'a>,
    next_local_member: u32,
}

pub fn retained_family_draw_border_then_fill_members(
    frame: RetainedFamilyAnimationLeafFrame<'_>,
) -> Result<RetainedFamilyDrawBorderThenFillMembers<'_>, RetainedFamilyDrawBorderThenFillError> {
    if frame.state().mode != FamilyAnimationMode::DrawBorderThenFill {
        return Err(RetainedFamilyDrawBorderThenFillError::UnsupportedMode(
            frame.state().mode,
        ));
    }
    Ok(RetainedFamilyDrawBorderThenFillMembers {
        frame,
        next_local_member: 0,
    })
}

/// Resolve one retained runtime slot through an immutable family plan.
pub fn retained_family_draw_border_then_fill_members_for_object<'plan>(
    frame: &RetainedFamilyFrame<'_>,
    plan: &'plan RetainedFamilyAnimationPlan,
    object_index: usize,
) -> Result<
    Option<RetainedFamilyDrawBorderThenFillMembers<'plan>>,
    RetainedFamilyDrawBorderThenFillError,
> {
    let Some(leaf) = frame.planned_family_leaf(plan, object_index)? else {
        return Ok(None);
    };
    Ok(Some(retained_family_draw_border_then_fill_members(leaf)?))
}

impl Iterator for RetainedFamilyDrawBorderThenFillMembers<'_> {
    type Item =
        Result<RetainedFamilyDrawBorderThenFillMember, RetainedFamilyDrawBorderThenFillError>;

    fn next(&mut self) -> Option<Self::Item> {
        let member_count = self.frame.span().member_count;
        if self.next_local_member >= member_count {
            return None;
        }

        let local_member = self.next_local_member;
        self.next_local_member += 1;
        let object = self.frame.span().object;
        let member = match self.frame.member(local_member) {
            Some(member) => member,
            None => {
                return Some(Err(
                    RetainedFamilyDrawBorderThenFillError::MissingPreparedMember {
                        object,
                        local_member,
                    },
                ))
            }
        };
        let progress = match self.frame.member_progress(local_member) {
            Ok(progress) => progress,
            Err(error) => return Some(Err(error.into())),
        };

        Some(match member {
            RetainedAnimationMember::Geometry => Err(
                RetainedFamilyDrawBorderThenFillError::UnsupportedGeometry(object),
            ),
            RetainedAnimationMember::Text(member) => Ok(RetainedFamilyDrawBorderThenFillMember {
                object,
                glyph: member.glyph,
                phase: RetainedDrawBorderThenFillPhase::from_member_progress(progress),
            }),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self
            .frame
            .span()
            .member_count
            .saturating_sub(self.next_local_member) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for RetainedFamilyDrawBorderThenFillMembers<'_> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FamilyAnimationState, FontFaceIdentity, GeometryRef, GlyphRun, ObjectContentRef,
        PositionedGlyph, RateFunction, Rect, RetainedFamilyAnimationPlanBuilder,
        RetainedObjectDefinition, SemanticStore, Style, TextAffineTransform, TextClusterIdentity,
        TextDirection, TextRenderItem, TextResource, TextResourceArena, TextSourceKind,
        TextSourceSpan, Transform2D, Vec2,
    };
    use noon_runtime::{FrameObjectState, FrameState};

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

    fn text_plan() -> (
        RetainedFamilyAnimationPlan,
        FrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let mut store = SemanticStore::new();
        let text_leaf = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, text_leaf).unwrap();

        let mut texts = TextResourceArena::new();
        let text_handle = texts.insert(text_resource()).unwrap();
        let object = RetainedObjectDefinition::text(ObjectId::new(10), text_handle);
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder.accept_leaf(text_leaf, &object, &texts).unwrap();
        let plan = builder.finish().unwrap();
        let frame = FrameState {
            time: 1.0,
            objects: vec![FrameObjectState {
                id: ObjectId::new(10),
                content: ObjectContentRef::Text(text_handle),
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
        (plan, frame, vec![Some(state(0.375))])
    }

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

    #[test]
    fn phase_matches_manim_integer_interpolate_split() {
        assert_eq!(
            RetainedDrawBorderThenFillPhase::from_member_progress(0.0),
            RetainedDrawBorderThenFillPhase::Outline { reveal: 0.0 }
        );
        assert_eq!(
            RetainedDrawBorderThenFillPhase::from_member_progress(0.25),
            RetainedDrawBorderThenFillPhase::Outline { reveal: 0.5 }
        );
        assert_eq!(
            RetainedDrawBorderThenFillPhase::from_member_progress(0.5),
            RetainedDrawBorderThenFillPhase::Fill { progress: 0.0 }
        );
        assert_eq!(
            RetainedDrawBorderThenFillPhase::from_member_progress(0.75),
            RetainedDrawBorderThenFillPhase::Fill { progress: 0.5 }
        );
        assert_eq!(
            RetainedDrawBorderThenFillPhase::from_member_progress(1.0),
            RetainedDrawBorderThenFillPhase::Fill { progress: 1.0 }
        );
    }

    #[test]
    fn text_glyph_identity_is_preserved_after_family_scheduling() {
        let (plan, frame, states) = text_plan();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let members = retained_family_draw_border_then_fill_members_for_object(&family, &plan, 0)
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(members.len(), 2);
        assert_eq!(members[0].object, ObjectId::new(10));
        assert_eq!(members[0].glyph.run_index, 0);
        assert_eq!(members[0].glyph.glyph_index, 0);
        assert_eq!(
            members[0].phase,
            RetainedDrawBorderThenFillPhase::Outline { reveal: 0.75 }
        );
        assert_eq!(members[1].glyph.glyph_index, 1);
        assert_eq!(
            members[1].phase,
            RetainedDrawBorderThenFillPhase::Outline { reveal: 0.75 }
        );
    }

    #[test]
    fn reveal_mode_is_not_silently_treated_as_draw_border_then_fill() {
        let (plan, frame, mut states) = text_plan();
        states[0].as_mut().unwrap().mode = FamilyAnimationMode::Reveal;
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        assert_eq!(
            retained_family_draw_border_then_fill_members_for_object(&family, &plan, 0)
                .unwrap_err(),
            RetainedFamilyDrawBorderThenFillError::UnsupportedMode(FamilyAnimationMode::Reveal)
        );
    }

    #[test]
    fn geometry_draw_border_then_fill_stays_explicitly_unsupported() {
        let mut store = SemanticStore::new();
        let leaf = store.insert_authoring_object();
        let family_id = store.insert_family();
        store.add_member(family_id, leaf).unwrap();
        let object =
            RetainedObjectDefinition::geometry(ObjectId::new(20), GeometryRef::circle(1.0));
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family_id).unwrap();
        builder.accept_leaf(leaf, &object, &texts).unwrap();
        let plan = builder.finish().unwrap();
        let frame = FrameState {
            time: 1.0,
            objects: vec![FrameObjectState {
                id: ObjectId::new(20),
                content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
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
        let states = vec![Some(state(0.25))];
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let mut members =
            retained_family_draw_border_then_fill_members_for_object(&family, &plan, 0)
                .unwrap()
                .unwrap();
        assert_eq!(
            members.next().unwrap().unwrap_err(),
            RetainedFamilyDrawBorderThenFillError::UnsupportedGeometry(ObjectId::new(20))
        );
    }
}

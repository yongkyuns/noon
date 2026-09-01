use noon_core::{
    FamilyAnimationMode, ObjectId, RetainedAnimationMember, RetainedFamilyAnimationEvaluationError,
    RetainedFamilyAnimationLeafFrame, RetainedFamilyAnimationPlan, TextAnimationGlyphRef,
};
use noon_runtime::{RetainedFamilyFrame, RetainedFamilyFramePlanError};

/// One renderer-facing reveal command derived from an already prepared family leaf.
///
/// Timing, lag, easing, and member-order reversal have already been resolved by the
/// shared family plan. The renderer only chooses the concrete realization for the
/// prepared member kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyRevealMember {
    Geometry {
        object: ObjectId,
        reveal: f32,
    },
    TextGlyph {
        object: ObjectId,
        glyph: TextAnimationGlyphRef,
        reveal: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyRevealError {
    UnsupportedMode(FamilyAnimationMode),
    MissingPreparedMember { object: ObjectId, local_member: u32 },
    FramePlan(RetainedFamilyFramePlanError),
    Evaluation(RetainedFamilyAnimationEvaluationError),
}

impl std::fmt::Display for RetainedFamilyRevealError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMode(mode) => {
                write!(
                    formatter,
                    "family reveal realization does not support {mode:?}"
                )
            }
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

impl std::error::Error for RetainedFamilyRevealError {}

impl From<RetainedFamilyFramePlanError> for RetainedFamilyRevealError {
    fn from(value: RetainedFamilyFramePlanError) -> Self {
        Self::FramePlan(value)
    }
}

impl From<RetainedFamilyAnimationEvaluationError> for RetainedFamilyRevealError {
    fn from(value: RetainedFamilyAnimationEvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Zero-allocation iterator over reveal realization for one already prepared leaf.
///
/// The iterator deliberately has no knowledge of semantic family traversal or timing
/// composition. It consumes the immutable descriptors and global member progress
/// produced by `noon-core` and exposes only content realization data.
#[derive(Clone, Copy, Debug)]
pub struct RetainedFamilyRevealMembers<'a> {
    frame: RetainedFamilyAnimationLeafFrame<'a>,
    next_local_member: u32,
}

pub fn retained_family_reveal_members(
    frame: RetainedFamilyAnimationLeafFrame<'_>,
) -> Result<RetainedFamilyRevealMembers<'_>, RetainedFamilyRevealError> {
    if frame.state().mode != FamilyAnimationMode::Reveal {
        return Err(RetainedFamilyRevealError::UnsupportedMode(
            frame.state().mode,
        ));
    }
    Ok(RetainedFamilyRevealMembers {
        frame,
        next_local_member: 0,
    })
}

/// Resolve one retained runtime object slot through the prepared family plan and expose
/// its renderer-facing reveal members.
///
/// `Ok(None)` preserves the runtime bridge's distinction between an inactive slot and a
/// slot owned by another prepared plan. No semantic traversal, resource lookup, lag
/// mapping, easing, or member-order logic is repeated here.
pub fn retained_family_reveal_members_for_object<'plan>(
    frame: &RetainedFamilyFrame<'_>,
    plan: &'plan RetainedFamilyAnimationPlan,
    object_index: usize,
) -> Result<Option<RetainedFamilyRevealMembers<'plan>>, RetainedFamilyRevealError> {
    let Some(leaf) = frame.planned_family_leaf(plan, object_index)? else {
        return Ok(None);
    };
    Ok(Some(retained_family_reveal_members(leaf)?))
}

impl Iterator for RetainedFamilyRevealMembers<'_> {
    type Item = Result<RetainedFamilyRevealMember, RetainedFamilyRevealError>;

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
                return Some(Err(RetainedFamilyRevealError::MissingPreparedMember {
                    object,
                    local_member,
                }))
            }
        };
        let reveal = match self.frame.member_progress(local_member) {
            Ok(progress) => progress,
            Err(error) => return Some(Err(error.into())),
        };

        Some(Ok(match member {
            RetainedAnimationMember::Geometry => {
                RetainedFamilyRevealMember::Geometry { object, reveal }
            }
            RetainedAnimationMember::Text(member) => RetainedFamilyRevealMember::TextGlyph {
                object,
                glyph: member.glyph,
                reveal,
            },
        }))
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

impl ExactSizeIterator for RetainedFamilyRevealMembers<'_> {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FamilyAnimationState, FontFaceIdentity, GeometryRef, GlyphRun, ObjectContentRef, ObjectId,
        PositionedGlyph, RateFunction, Rect, RetainedFamilyAnimationPlan,
        RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition, SemanticNodeId,
        SemanticStore, Style, TextAffineTransform, TextClusterIdentity, TextDirection,
        TextRenderItem, TextResource, TextResourceArena, TextSourceKind, TextSourceSpan,
        Transform2D, Vec2,
    };
    use noon_runtime::{RetainedFrameObjectState, RetainedFrameState};

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

    fn mixed_plan() -> (RetainedFamilyAnimationPlan, SemanticNodeId, SemanticNodeId) {
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
        (builder.finish().unwrap(), text_leaf, circle_leaf)
    }

    fn geometry_runtime_fixture() -> (
        RetainedFamilyAnimationPlan,
        RetainedFrameState,
        Vec<Option<FamilyAnimationState>>,
    ) {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, first).unwrap();
        store.add_member(family, second).unwrap();

        let first_object =
            RetainedObjectDefinition::geometry(ObjectId::new(20), GeometryRef::circle(1.0));
        let second_object =
            RetainedObjectDefinition::geometry(ObjectId::new(21), GeometryRef::circle(2.0));
        let texts = TextResourceArena::new();
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder.accept_leaf(first, &first_object, &texts).unwrap();
        builder.accept_leaf(second, &second_object, &texts).unwrap();
        let plan = builder.finish().unwrap();

        let frame = RetainedFrameState {
            time: 1.0,
            objects: vec![
                RetainedFrameObjectState {
                    id: ObjectId::new(20),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
                RetainedFrameObjectState {
                    id: ObjectId::new(21),
                    content: ObjectContentRef::Geometry(GeometryRef::circle(2.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
        };
        let animation = state(false);
        (plan, frame, vec![Some(animation), Some(animation)])
    }

    fn state(reverse_member_order: bool) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress: 0.5,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function: false,
            reverse_member_order,
        }
    }

    #[test]
    fn mixed_text_and_geometry_realize_one_global_reveal_sequence() {
        let (plan, text_leaf, circle_leaf) = mixed_plan();
        let text =
            retained_family_reveal_members(plan.leaf_frame(state(false), text_leaf).unwrap())
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let circle =
            retained_family_reveal_members(plan.leaf_frame(state(false), circle_leaf).unwrap())
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

        assert!(matches!(
            text.as_slice(),
            [
                RetainedFamilyRevealMember::TextGlyph { reveal: 1.0, .. },
                RetainedFamilyRevealMember::TextGlyph { reveal: 0.5, .. }
            ]
        ));
        assert_eq!(
            circle,
            vec![RetainedFamilyRevealMember::Geometry {
                object: ObjectId::new(11),
                reveal: 0.0,
            }]
        );
    }

    #[test]
    fn runtime_object_helper_composes_plan_binding_and_reveal_realization() {
        let (plan, frame, states) = geometry_runtime_fixture();
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        let first = retained_family_reveal_members_for_object(&family, &plan, 0)
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let second = retained_family_reveal_members_for_object(&family, &plan, 1)
            .unwrap()
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            first,
            vec![RetainedFamilyRevealMember::Geometry {
                object: ObjectId::new(20),
                reveal: 1.0,
            }]
        );
        assert_eq!(
            second,
            vec![RetainedFamilyRevealMember::Geometry {
                object: ObjectId::new(21),
                reveal: 0.0,
            }]
        );
    }

    #[test]
    fn runtime_object_helper_preserves_inactive_slots() {
        let (plan, frame, mut states) = geometry_runtime_fixture();
        states[0] = None;
        let family = RetainedFamilyFrame {
            retained: &frame,
            family_animations: &states,
        };
        assert!(retained_family_reveal_members_for_object(&family, &plan, 0)
            .unwrap()
            .is_none());
    }

    #[test]
    fn global_member_reversal_is_preserved_without_renderer_timing_logic() {
        let (plan, text_leaf, circle_leaf) = mixed_plan();
        let text = retained_family_reveal_members(plan.leaf_frame(state(true), text_leaf).unwrap())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let circle =
            retained_family_reveal_members(plan.leaf_frame(state(true), circle_leaf).unwrap())
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

        assert!(matches!(
            text.as_slice(),
            [
                RetainedFamilyRevealMember::TextGlyph { reveal: 0.0, .. },
                RetainedFamilyRevealMember::TextGlyph { reveal: 0.5, .. }
            ]
        ));
        assert!(matches!(
            circle.as_slice(),
            [RetainedFamilyRevealMember::Geometry { reveal: 1.0, .. }]
        ));
    }

    #[test]
    fn unsupported_operation_fails_before_content_realization() {
        let (plan, text_leaf, _) = mixed_plan();
        let mut unsupported = state(false);
        unsupported.mode = FamilyAnimationMode::DrawBorderThenFill;
        assert_eq!(
            retained_family_reveal_members(plan.leaf_frame(unsupported, text_leaf).unwrap())
                .unwrap_err(),
            RetainedFamilyRevealError::UnsupportedMode(FamilyAnimationMode::DrawBorderThenFill)
        );
    }
}

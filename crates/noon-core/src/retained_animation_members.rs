use crate::{
    plain_text_animation_members, FamilyAnimationError, FamilyAnimationState, ObjectContentRef,
    TextAnimationMember, TextAnimationMemberError, TextResourceArena, TextResourceHandle,
};

/// Lightweight content-local identity for one Manim-visible animation member.
///
/// This is derived execution metadata, not persistent scene identity. Heavy geometry,
/// shaped glyph payloads, and frontend wrapper objects stay out of the member sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetainedAnimationMember {
    /// One ordinary retained geometry leaf. Path/analytic realization stays in the
    /// geometry renderer; the family planner only needs this leaf-level cardinality.
    Geometry,
    /// One rendered plain-Text glyph member in retained painter order.
    Text(TextAnimationMember),
}

/// Ordered content-local animation members for one retained semantic leaf.
///
/// The global family planner consumes only [`Self::member_count`]. Renderer/content
/// adapters use this same sequence to map a leaf-local member index back to its
/// lightweight retained-content identity. Per-member timing is evaluated here as well
/// so every renderer observes the same authoritative count/order and cannot subtly
/// reimplement Manim's lag/reversal indexing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedAnimationMembers {
    members: Vec<RetainedAnimationMember>,
    member_count: u32,
}

impl RetainedAnimationMembers {
    pub fn resolve(
        content: &ObjectContentRef,
        texts: &TextResourceArena,
    ) -> Result<Self, RetainedAnimationMemberError> {
        let members = match content {
            // Geometry is one Manim-visible family leaf at this boundary. Whether a
            // specific operation can realize that geometry is checked separately by
            // the operation/renderer capability layer; family timing must not inspect
            // geometry variants.
            ObjectContentRef::Geometry(_) => vec![RetainedAnimationMember::Geometry],
            ObjectContentRef::Text(handle) => {
                let resource = texts
                    .get(*handle)
                    .ok_or(RetainedAnimationMemberError::MissingTextResource(*handle))?;
                plain_text_animation_members(resource)?
                    .into_iter()
                    .map(RetainedAnimationMember::Text)
                    .collect()
            }
        };
        let member_count = u32::try_from(members.len())
            .map_err(|_| RetainedAnimationMemberError::TooManyMembers(members.len()))?;
        Ok(Self {
            members,
            member_count,
        })
    }

    pub const fn member_count(&self) -> u32 {
        self.member_count
    }

    pub fn members(&self) -> &[RetainedAnimationMember] {
        &self.members
    }

    pub fn member(&self, local_member: u32) -> Option<RetainedAnimationMember> {
        self.members.get(local_member as usize).copied()
    }

    /// Evaluate one valid local member through the shared Manim family schedule.
    ///
    /// This is allocation-free after member resolution. `FamilyAnimationState` owns
    /// overall progress, lag ratio, rate-function reversal, and member-order reversal;
    /// this container contributes only authoritative content-local cardinality/order.
    pub fn member_progress(
        &self,
        state: FamilyAnimationState,
        local_member: u32,
    ) -> Result<f32, FamilyAnimationError> {
        state.member_progress(local_member, self.member_count)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedAnimationMemberError {
    MissingTextResource(TextResourceHandle),
    Text(TextAnimationMemberError),
    TooManyMembers(usize),
}

impl std::fmt::Display for RetainedAnimationMemberError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTextResource(handle) => write!(
                formatter,
                "retained animation members reference missing text resource {}:{}",
                handle.id.get(),
                handle.version
            ),
            Self::Text(error) => error.fmt(formatter),
            Self::TooManyMembers(count) => write!(
                formatter,
                "retained animation leaf has {count} members, exceeding the u32 member range"
            ),
        }
    }
}

impl std::error::Error for RetainedAnimationMemberError {}

impl From<TextAnimationMemberError> for RetainedAnimationMemberError {
    fn from(value: TextAnimationMemberError) -> Self {
        Self::Text(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        FamilyAnimationMode, FontFaceIdentity, GeometryId, GeometryRef, GlyphRun, PositionedGlyph,
        RateFunction, Rect, TextAffineTransform, TextClusterIdentity, TextDirection, TextRenderItem,
        TextResource, TextResourceId, TextSourceKind, TextSourceSpan, Vec2, VectorPath,
    };

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

    fn plain_resource(source: &str, glyphs: Vec<PositionedGlyph>) -> TextResource {
        TextResource {
            source: Arc::from(source),
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
                glyphs: glyphs.into(),
            }]),
            vector_items: Arc::from([]),
            render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    fn family_state(
        overall_progress: f64,
        reverse_rate_function: bool,
        reverse_member_order: bool,
    ) -> FamilyAnimationState {
        FamilyAnimationState {
            mode: FamilyAnimationMode::Reveal,
            overall_progress,
            lag_ratio: 1.0,
            rate_function: RateFunction::Linear,
            reverse_rate_function,
            reverse_member_order,
        }
    }

    #[test]
    fn retained_geometry_is_one_local_animation_member_without_payload_copying() {
        let texts = TextResourceArena::new();
        let geometries = [
            GeometryRef::circle(1.0),
            GeometryRef::rectangle(2.0, 3.0),
            GeometryRef::line(Vec2::ZERO, Vec2::ONE),
            GeometryRef::path(VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE)),
            GeometryRef::External(GeometryId::new(99)),
        ];

        for geometry in geometries {
            let members =
                RetainedAnimationMembers::resolve(&ObjectContentRef::Geometry(geometry), &texts)
                    .unwrap();
            assert_eq!(members.member_count(), 1);
            assert_eq!(members.members(), &[RetainedAnimationMember::Geometry]);
            assert_eq!(members.member(0), Some(RetainedAnimationMember::Geometry));
            assert_eq!(members.member(1), None);
        }
    }

    #[test]
    fn plain_text_reuses_rendered_glyph_member_order_and_whitespace_filtering() {
        let mut texts = TextResourceArena::new();
        let handle = texts
            .insert(plain_resource(
                "A B",
                vec![
                    glyph(TextSourceSpan::new(0, 1), 1, 0.0),
                    glyph(TextSourceSpan::new(1, 2), 2, 1.0),
                    glyph(TextSourceSpan::new(2, 3), 3, 2.0),
                ],
            ))
            .unwrap();

        let members =
            RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), &texts).unwrap();
        assert_eq!(members.member_count(), 2);
        assert!(matches!(
            members.member(0),
            Some(RetainedAnimationMember::Text(member))
                if member.source_span == TextSourceSpan::new(0, 1)
                    && member.glyph.run_index == 0
                    && member.glyph.glyph_index == 0
        ));
        assert!(matches!(
            members.member(1),
            Some(RetainedAnimationMember::Text(member))
                if member.source_span == TextSourceSpan::new(2, 3)
                    && member.glyph.run_index == 0
                    && member.glyph.glyph_index == 2
        ));
    }

    #[test]
    fn resolved_member_count_drives_shared_lag_and_reversal_semantics() {
        let mut texts = TextResourceArena::new();
        let handle = texts
            .insert(plain_resource(
                "A B",
                vec![
                    glyph(TextSourceSpan::new(0, 1), 1, 0.0),
                    glyph(TextSourceSpan::new(1, 2), 2, 1.0),
                    glyph(TextSourceSpan::new(2, 3), 3, 2.0),
                ],
            ))
            .unwrap();
        let members =
            RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), &texts).unwrap();

        let create = family_state(0.25, false, false);
        assert_eq!(members.member_progress(create, 0).unwrap(), 0.5);
        assert_eq!(members.member_progress(create, 1).unwrap(), 0.0);

        let uncreate = family_state(0.25, true, false);
        assert_eq!(members.member_progress(uncreate, 0).unwrap(), 0.5);
        assert_eq!(members.member_progress(uncreate, 1).unwrap(), 1.0);

        let reversed_order = family_state(0.25, false, true);
        assert_eq!(members.member_progress(reversed_order, 0).unwrap(), 0.0);
        assert_eq!(members.member_progress(reversed_order, 1).unwrap(), 0.5);
    }

    #[test]
    fn invalid_member_index_is_rejected_by_shared_family_state() {
        let texts = TextResourceArena::new();
        let members = RetainedAnimationMembers::resolve(
            &ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
            &texts,
        )
        .unwrap();
        assert!(members
            .member_progress(family_state(0.5, false, false), 1)
            .is_err());
    }

    #[test]
    fn whitespace_only_text_is_a_valid_zero_member_leaf() {
        let mut texts = TextResourceArena::new();
        let handle = texts
            .insert(plain_resource(
                " ",
                vec![glyph(TextSourceSpan::new(0, 1), 1, 0.0)],
            ))
            .unwrap();
        let members =
            RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), &texts).unwrap();
        assert_eq!(members.member_count(), 0);
        assert!(members.members().is_empty());
    }

    #[test]
    fn missing_or_unsupported_text_resources_fail_closed() {
        let texts = TextResourceArena::new();
        let missing = TextResourceHandle {
            id: TextResourceId::new(99),
            version: 7,
        };
        assert_eq!(
            RetainedAnimationMembers::resolve(&ObjectContentRef::Text(missing), &texts),
            Err(RetainedAnimationMemberError::MissingTextResource(missing))
        );

        let mut texts = TextResourceArena::new();
        let mut typst = plain_resource("A", vec![glyph(TextSourceSpan::new(0, 1), 1, 0.0)]);
        typst.kind = TextSourceKind::Typst;
        let handle = texts.insert(typst).unwrap();
        assert_eq!(
            RetainedAnimationMembers::resolve(&ObjectContentRef::Text(handle), &texts),
            Err(RetainedAnimationMemberError::Text(
                TextAnimationMemberError::UnsupportedSourceKind(TextSourceKind::Typst)
            ))
        );
    }
}

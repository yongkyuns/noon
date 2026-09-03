use crate::{
    FamilyAnimationLeafProgress, FamilyAnimationLeafSpan, FamilyAnimationMemberEvaluationError,
    FamilyAnimationMemberPlan, FamilyAnimationMemberPlanBuilder, FamilyAnimationRequest,
    FamilyAnimationRequestError, FamilyAnimationState, ObjectId, RetainedAnimationMember,
    RetainedAnimationMembers, RetainedFamilyAnimationMemberPlanError, RetainedObjectDefinition,
    SemanticNodeId, SemanticStore, TextResourceArena,
};

/// Prepared retained-content binding for one semantic family leaf.
///
/// Content-local animation members are resolved once while the plan is built. Frame
/// evaluation borrows this immutable metadata and never reshapes Text, looks up
/// resources, or copies geometry payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedFamilyAnimationLeafPlan {
    span: FamilyAnimationLeafSpan,
    members: RetainedAnimationMembers,
}

impl RetainedFamilyAnimationLeafPlan {
    pub const fn span(&self) -> FamilyAnimationLeafSpan {
        self.span
    }

    pub fn members(&self) -> &RetainedAnimationMembers {
        &self.members
    }
}

/// Immutable retained preparation for one semantic family animation target.
///
/// The generic member plan owns semantic/runtime spans and global indexing. This
/// companion stores only the lightweight content-local descriptors required to
/// realize those spans. Timing remains in [`FamilyAnimationState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedFamilyAnimationPlan {
    member_plan: FamilyAnimationMemberPlan,
    leaves: Vec<RetainedFamilyAnimationLeafPlan>,
}

impl RetainedFamilyAnimationPlan {
    pub fn member_plan(&self) -> &FamilyAnimationMemberPlan {
        &self.member_plan
    }

    pub fn leaves(&self) -> &[RetainedFamilyAnimationLeafPlan] {
        &self.leaves
    }

    pub fn leaf(&self, semantic_leaf: SemanticNodeId) -> Option<&RetainedFamilyAnimationLeafPlan> {
        self.leaves
            .iter()
            .find(|leaf| leaf.span.semantic_leaf == semantic_leaf)
    }

    /// Resolve a prepared leaf through the retained runtime identity consumed by
    /// render preparation. Semantic identity remains authoritative in the plan; this
    /// is only the lowering-time bridge back to the retained object slot.
    pub fn leaf_for_object(&self, object: ObjectId) -> Option<&RetainedFamilyAnimationLeafPlan> {
        self.leaves.iter().find(|leaf| leaf.span.object == object)
    }

    /// Lower an authoring request only after retained content has been materialized.
    ///
    /// The request contributes stable semantic/runtime identities and authoritative
    /// leaf order. Text glyph/member identity is derived exclusively from the supplied
    /// execution-local retained objects and text resource arena.
    pub fn from_request(
        request: &FamilyAnimationRequest,
        objects: &[RetainedObjectDefinition],
        texts: &TextResourceArena,
    ) -> Result<Self, RetainedFamilyAnimationRequestPlanError> {
        request.validate()?;
        let expected_leaves = request
            .bindings()
            .iter()
            .map(|binding| binding.semantic_leaf)
            .collect();
        let mut builder =
            RetainedFamilyAnimationPlanBuilder::begin_ordered(request.target(), expected_leaves)?;

        for binding in request.bindings() {
            let object = objects
                .iter()
                .find(|object| object.id == binding.object)
                .ok_or(RetainedFamilyAnimationRequestPlanError::MissingObject(
                    binding.object,
                ))?;
            builder.accept_leaf(binding.semantic_leaf, object, texts)?;
        }
        Ok(builder.finish()?)
    }

    /// Borrow prepared content metadata together with the globally evaluated leaf
    /// progress view for one frame.
    pub fn leaf_frame(
        &self,
        state: FamilyAnimationState,
        semantic_leaf: SemanticNodeId,
    ) -> Result<RetainedFamilyAnimationLeafFrame<'_>, RetainedFamilyAnimationEvaluationError> {
        let progress = self.member_plan.leaf_progress(state, semantic_leaf)?;
        let leaf = self
            .leaf(semantic_leaf)
            .ok_or(RetainedFamilyAnimationEvaluationError::MissingLeafDescriptor(semantic_leaf))?;
        Ok(RetainedFamilyAnimationLeafFrame {
            leaf,
            progress,
            state,
        })
    }

    /// Renderer-facing object lookup for an already prepared family plan.
    pub fn leaf_frame_for_object(
        &self,
        state: FamilyAnimationState,
        object: ObjectId,
    ) -> Result<RetainedFamilyAnimationLeafFrame<'_>, RetainedFamilyAnimationEvaluationError> {
        state
            .validate()
            .map_err(FamilyAnimationMemberEvaluationError::from)?;
        let leaf = self
            .leaf_for_object(object)
            .ok_or(RetainedFamilyAnimationEvaluationError::MissingObjectDescriptor(object))?;
        let progress = self
            .member_plan
            .leaf_progress(state, leaf.span.semantic_leaf)?;
        Ok(RetainedFamilyAnimationLeafFrame {
            leaf,
            progress,
            state,
        })
    }
}

/// Transactional retained-content builder over the generic global member planner.
#[derive(Clone, Debug)]
pub struct RetainedFamilyAnimationPlanBuilder {
    inner: FamilyAnimationMemberPlanBuilder,
    members_by_leaf: Vec<RetainedAnimationMembers>,
}

impl RetainedFamilyAnimationPlanBuilder {
    pub fn begin(
        store: &SemanticStore,
        target: SemanticNodeId,
    ) -> Result<Self, RetainedFamilyAnimationMemberPlanError> {
        Ok(Self {
            inner: FamilyAnimationMemberPlanBuilder::begin(store, target)?,
            members_by_leaf: Vec::new(),
        })
    }

    /// Begin from semantic leaf order already snapshotted by the authoring side.
    pub fn begin_ordered(
        target: SemanticNodeId,
        expected_leaves: Vec<SemanticNodeId>,
    ) -> Result<Self, RetainedFamilyAnimationMemberPlanError> {
        Ok(Self {
            inner: FamilyAnimationMemberPlanBuilder::begin_ordered(target, expected_leaves)?,
            members_by_leaf: Vec::new(),
        })
    }

    pub fn expected_leaf_count(&self) -> usize {
        self.inner.expected_leaf_count()
    }

    /// Resolve one leaf exactly once and commit its lightweight descriptors only
    /// after the generic span builder accepts the same semantic/runtime binding.
    pub fn accept_leaf(
        &mut self,
        semantic_leaf: SemanticNodeId,
        object: &RetainedObjectDefinition,
        texts: &TextResourceArena,
    ) -> Result<(), RetainedFamilyAnimationMemberPlanError> {
        let members = self
            .inner
            .accept_retained_leaf(semantic_leaf, object, texts)?;
        self.members_by_leaf.push(members);
        Ok(())
    }

    pub fn finish(
        self,
    ) -> Result<RetainedFamilyAnimationPlan, RetainedFamilyAnimationMemberPlanError> {
        let member_plan = self.inner.finish()?;
        debug_assert_eq!(member_plan.leaves().len(), self.members_by_leaf.len());
        let leaves = member_plan
            .leaves()
            .iter()
            .copied()
            .zip(self.members_by_leaf)
            .map(|(span, members)| RetainedFamilyAnimationLeafPlan { span, members })
            .collect();
        Ok(RetainedFamilyAnimationPlan {
            member_plan,
            leaves,
        })
    }
}

/// Failure while resolving a canonical authoring request against one materialized
/// retained scene.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyAnimationRequestPlanError {
    Request(FamilyAnimationRequestError),
    Plan(RetainedFamilyAnimationMemberPlanError),
    MissingObject(ObjectId),
}

impl std::fmt::Display for RetainedFamilyAnimationRequestPlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
            Self::MissingObject(object) => write!(
                formatter,
                "family animation request references missing retained object {}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyAnimationRequestPlanError {}

impl From<FamilyAnimationRequestError> for RetainedFamilyAnimationRequestPlanError {
    fn from(value: FamilyAnimationRequestError) -> Self {
        Self::Request(value)
    }
}

impl From<RetainedFamilyAnimationMemberPlanError> for RetainedFamilyAnimationRequestPlanError {
    fn from(value: RetainedFamilyAnimationMemberPlanError) -> Self {
        Self::Plan(value)
    }
}

/// Borrowed frame-time view of one already-prepared retained family leaf.
#[derive(Clone, Copy, Debug)]
pub struct RetainedFamilyAnimationLeafFrame<'a> {
    leaf: &'a RetainedFamilyAnimationLeafPlan,
    progress: FamilyAnimationLeafProgress,
    state: FamilyAnimationState,
}

impl<'a> RetainedFamilyAnimationLeafFrame<'a> {
    pub const fn span(self) -> FamilyAnimationLeafSpan {
        self.progress.span()
    }

    pub const fn state(self) -> FamilyAnimationState {
        self.state
    }

    pub fn members(self) -> &'a [RetainedAnimationMember] {
        self.leaf.members.members()
    }

    pub fn member(self, local_member: u32) -> Option<RetainedAnimationMember> {
        self.leaf.members.member(local_member)
    }

    pub fn member_progress(
        self,
        local_member: u32,
    ) -> Result<f32, RetainedFamilyAnimationEvaluationError> {
        Ok(self.progress.member_progress(local_member)?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RetainedFamilyAnimationEvaluationError {
    Progress(FamilyAnimationMemberEvaluationError),
    MissingLeafDescriptor(SemanticNodeId),
    MissingObjectDescriptor(ObjectId),
}

impl std::fmt::Display for RetainedFamilyAnimationEvaluationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Progress(error) => error.fmt(formatter),
            Self::MissingLeafDescriptor(leaf) => write!(
                formatter,
                "retained family animation plan has no prepared descriptor for semantic leaf {leaf:?}"
            ),
            Self::MissingObjectDescriptor(object) => write!(
                formatter,
                "retained family animation plan has no prepared descriptor for retained object {}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyAnimationEvaluationError {}

impl From<FamilyAnimationMemberEvaluationError> for RetainedFamilyAnimationEvaluationError {
    fn from(value: FamilyAnimationMemberEvaluationError) -> Self {
        Self::Progress(value)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        FamilyAnimationLeafBinding, FamilyAnimationMode, FamilyAnimationSpec, FontFaceIdentity,
        GeometryRef, GlyphRun, ObjectId, PositionedGlyph, RateFunction, Rect, TextAffineTransform,
        TextClusterIdentity, TextDirection, TextRenderItem, TextResource, TextSourceKind,
        TextSourceSpan, Vec2,
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

    fn mixed_plan() -> (RetainedFamilyAnimationPlan, SemanticNodeId, SemanticNodeId) {
        let mut store = SemanticStore::new();
        let text_leaf = store.insert_authoring_object();
        let circle_leaf = store.insert_authoring_object();
        let family = store.insert_family();
        store.add_member(family, text_leaf).unwrap();
        store.add_member(family, circle_leaf).unwrap();

        let mut texts = TextResourceArena::new();
        let text_handle = texts
            .insert(plain_resource(
                "AB",
                vec![
                    glyph(TextSourceSpan::new(0, 1), 1, 0.0),
                    glyph(TextSourceSpan::new(1, 2), 2, 1.0),
                ],
            ))
            .unwrap();
        let text = RetainedObjectDefinition::text(ObjectId::new(10), text_handle);
        let circle =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(1.0));

        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&store, family).unwrap();
        builder.accept_leaf(text_leaf, &text, &texts).unwrap();
        builder.accept_leaf(circle_leaf, &circle, &texts).unwrap();
        (builder.finish().unwrap(), text_leaf, circle_leaf)
    }

    #[test]
    fn prepared_mixed_family_borrows_descriptors_and_global_progress() {
        let (plan, text_leaf, circle_leaf) = mixed_plan();
        assert_eq!(plan.member_plan().total_member_count(), 3);

        let text = plan.leaf_frame(state(false), text_leaf).unwrap();
        let circle = plan.leaf_frame(state(false), circle_leaf).unwrap();
        assert_eq!(text.members().len(), 2);
        assert!(matches!(
            text.member(0),
            Some(RetainedAnimationMember::Text(_))
        ));
        assert!(matches!(
            text.member(1),
            Some(RetainedAnimationMember::Text(_))
        ));
        assert_eq!(circle.members(), &[RetainedAnimationMember::Geometry]);
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
        assert_eq!(text.state(), state(false));

        let prepared = plan.leaf(text_leaf).unwrap();
        assert!(std::ptr::eq(text.members(), prepared.members().members()));
    }

    #[test]
    fn request_lowering_resolves_members_from_execution_local_resources() {
        let text_leaf = SemanticNodeId::new(41, 3);
        let circle_leaf = SemanticNodeId::new(12, 8);
        let family = SemanticNodeId::new(99, 7);
        let text_id = ObjectId::new(10);
        let circle_id = ObjectId::new(11);
        let spec = FamilyAnimationSpec::new(
            FamilyAnimationMode::Reveal,
            1.0,
            2.0,
            1.0,
            RateFunction::Linear,
            false,
            false,
        )
        .unwrap();
        let request = FamilyAnimationRequest::new(
            family,
            vec![
                FamilyAnimationLeafBinding::new(text_leaf, text_id),
                FamilyAnimationLeafBinding::new(circle_leaf, circle_id),
            ],
            spec,
        )
        .unwrap();

        let mut texts = TextResourceArena::new();
        let text_handle = texts
            .insert(plain_resource(
                "AB",
                vec![
                    glyph(TextSourceSpan::new(0, 1), 1, 0.0),
                    glyph(TextSourceSpan::new(1, 2), 2, 1.0),
                ],
            ))
            .unwrap();
        let objects = vec![
            RetainedObjectDefinition::text(text_id, text_handle),
            RetainedObjectDefinition::geometry(circle_id, GeometryRef::circle(1.0)),
        ];

        let plan = RetainedFamilyAnimationPlan::from_request(&request, &objects, &texts).unwrap();
        assert_eq!(plan.member_plan().target(), family);
        assert_eq!(plan.member_plan().total_member_count(), 3);
        let midpoint = spec.state_at(2.0).unwrap();
        let text = plan.leaf_frame_for_object(midpoint, text_id).unwrap();
        let circle = plan.leaf_frame_for_object(midpoint, circle_id).unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }

    #[test]
    fn request_lowering_rejects_unknown_runtime_object() {
        let leaf = SemanticNodeId::new(3, 4);
        let target = SemanticNodeId::new(5, 6);
        let request = FamilyAnimationRequest::new(
            target,
            vec![FamilyAnimationLeafBinding::new(leaf, ObjectId::new(99))],
            FamilyAnimationSpec::new(
                FamilyAnimationMode::Reveal,
                0.0,
                1.0,
                0.0,
                RateFunction::Linear,
                false,
                false,
            )
            .unwrap(),
        )
        .unwrap();
        let error =
            RetainedFamilyAnimationPlan::from_request(&request, &[], &TextResourceArena::new())
                .unwrap_err();
        assert_eq!(
            error,
            RetainedFamilyAnimationRequestPlanError::MissingObject(ObjectId::new(99))
        );
    }

    #[test]
    fn retained_object_lookup_reuses_the_same_prepared_leaf() {
        let (plan, text_leaf, circle_leaf) = mixed_plan();
        let by_semantic = plan.leaf_frame(state(false), text_leaf).unwrap();
        let by_object = plan
            .leaf_frame_for_object(state(false), ObjectId::new(10))
            .unwrap();
        assert_eq!(by_semantic.span(), by_object.span());
        assert!(std::ptr::eq(by_semantic.members(), by_object.members()));

        let circle = plan
            .leaf_frame_for_object(state(false), ObjectId::new(11))
            .unwrap();
        assert_eq!(circle.span().semantic_leaf, circle_leaf);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }

    #[test]
    fn prepared_descriptors_reuse_global_member_order_reversal() {
        let (plan, text_leaf, circle_leaf) = mixed_plan();
        let text = plan.leaf_frame(state(true), text_leaf).unwrap();
        let circle = plan.leaf_frame(state(true), circle_leaf).unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 0.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 1.0);
    }

    #[test]
    fn unknown_leaf_or_object_fails_before_any_content_realization() {
        let (plan, _, _) = mixed_plan();
        let unknown = SemanticNodeId::new(u32::MAX, 0);
        assert_eq!(
            plan.leaf_frame(state(false), unknown).unwrap_err(),
            RetainedFamilyAnimationEvaluationError::Progress(
                FamilyAnimationMemberEvaluationError::UnknownLeaf(unknown)
            )
        );
        assert_eq!(
            plan.leaf_frame_for_object(state(false), ObjectId::new(99))
                .unwrap_err(),
            RetainedFamilyAnimationEvaluationError::MissingObjectDescriptor(ObjectId::new(99))
        );
    }
}

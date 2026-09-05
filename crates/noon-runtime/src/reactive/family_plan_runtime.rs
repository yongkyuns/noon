use noon_compile::CompiledScene;
use noon_core::{
    FamilyAnimationError, FamilyAnimationSpec, FamilyAnimationState, ObjectId,
    RetainedFamilyAnimationPlan,
};

use crate::{EvaluationError, FrameChanges, RetainedFamilyFrame, SceneInstance};

/// Failure while binding one target-independent family animation to a prepared retained plan.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyPlanRuntimeError {
    Animation(FamilyAnimationError),
    Evaluation(EvaluationError),
    UnknownObject(ObjectId),
}

impl std::fmt::Display for RetainedFamilyPlanRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Animation(error) => error.fmt(formatter),
            Self::Evaluation(error) => error.fmt(formatter),
            Self::UnknownObject(object) => write!(
                formatter,
                "retained family plan references unknown compiled object {}",
                object.get()
            ),
        }
    }
}

impl std::error::Error for RetainedFamilyPlanRuntimeError {}

impl From<FamilyAnimationError> for RetainedFamilyPlanRuntimeError {
    fn from(value: FamilyAnimationError) -> Self {
        Self::Animation(value)
    }
}

impl From<EvaluationError> for RetainedFamilyPlanRuntimeError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}

/// Deterministic retained runtime for one prepared semantic-family animation.
///
/// The immutable plan owns semantic leaf order and global member spans. This owner
/// evaluates the target-independent [`FamilyAnimationSpec`] exactly once per time and
/// projects the same resulting [`FamilyAnimationState`] onto only the retained object
/// slots referenced by that plan. Ordinary object properties remain owned by
/// [`SceneInstance`].
#[derive(Clone, Debug)]
pub struct RetainedFamilyPlanSceneInstance {
    inner: SceneInstance,
    plan: RetainedFamilyAnimationPlan,
    spec: FamilyAnimationSpec,
    leaf_indices: Vec<usize>,
    states: Vec<Option<FamilyAnimationState>>,
    state: Option<FamilyAnimationState>,
    family_changed_indices: Vec<usize>,
}

impl RetainedFamilyPlanSceneInstance {
    pub fn new(
        compiled: CompiledScene,
        plan: RetainedFamilyAnimationPlan,
        spec: FamilyAnimationSpec,
    ) -> Result<Self, RetainedFamilyPlanRuntimeError> {
        spec.validate()?;
        let leaf_indices = plan
            .leaves()
            .iter()
            .map(|leaf| {
                let object = leaf.span().object;
                compiled
                    .object_index(object)
                    .map(|index| index as usize)
                    .ok_or(RetainedFamilyPlanRuntimeError::UnknownObject(object))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let object_count = compiled.objects().len();
        let state = active_state_at(spec, 0.0)?;
        let mut states = vec![None; object_count];
        for &index in &leaf_indices {
            states[index] = state;
        }

        Ok(Self {
            inner: SceneInstance::new(compiled),
            plan,
            spec,
            leaf_indices,
            states,
            state,
            family_changed_indices: Vec::new(),
        })
    }

    pub fn frame(&self) -> RetainedFamilyFrame<'_> {
        RetainedFamilyFrame {
            retained: self.inner.frame(),
            family_animations: &self.states,
        }
    }

    pub fn plan(&self) -> &RetainedFamilyAnimationPlan {
        &self.plan
    }

    pub const fn spec(&self) -> FamilyAnimationSpec {
        self.spec
    }

    pub fn evaluate(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyPlanRuntimeError> {
        self.inner.evaluate(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn seek(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyPlanRuntimeError> {
        self.inner.seek(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn advance_to(
        &mut self,
        time: f64,
    ) -> Result<RetainedFamilyFrame<'_>, RetainedFamilyPlanRuntimeError> {
        self.inner.advance_to(time)?;
        self.update_family_state(time)?;
        Ok(self.frame())
    }

    pub fn take_frame_changes(&mut self) -> FrameChanges {
        let base = self.inner.take_frame_changes();
        if base.is_all() {
            self.family_changed_indices.clear();
            return FrameChanges::all();
        }

        let mut object_indices = base.object_indices().to_vec();
        object_indices.append(&mut self.family_changed_indices);
        FrameChanges::with_structure(
            object_indices,
            base.added_indices().to_vec(),
            base.removed_indices().to_vec(),
        )
    }

    pub fn inner(&self) -> &SceneInstance {
        &self.inner
    }

    fn update_family_state(&mut self, time: f64) -> Result<(), RetainedFamilyPlanRuntimeError> {
        let next = active_state_at(self.spec, time)?;
        if next == self.state {
            return Ok(());
        }

        for &index in &self.leaf_indices {
            self.states[index] = next;
            self.family_changed_indices.push(index);
        }
        self.state = next;
        Ok(())
    }
}

fn active_state_at(
    spec: FamilyAnimationSpec,
    time: f64,
) -> Result<Option<FamilyAnimationState>, FamilyAnimationError> {
    if time < spec.start_time || time > spec.end_time() {
        // Reuse the shared evaluator for non-finite time rejection instead of
        // introducing a second validation path here.
        if !time.is_finite() {
            spec.state_at(time)?;
        }
        return Ok(None);
    }
    Ok(Some(spec.state_at(time)?))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_compile::{CompiledObject, CompiledScene};
    use noon_core::{
        FamilyAnimationMode, FontFaceIdentity, GeometryRef, GlyphRun, ObjectId, PositionedGlyph,
        RateFunction, Rect, RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition,
        SemanticStore, TextAffineTransform, TextClusterIdentity, TextDirection, TextRenderItem,
        TextResource, TextResourceArena, TextSourceKind, TextSourceSpan, Vec2,
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

    fn fixture() -> (
        CompiledScene,
        RetainedFamilyAnimationPlan,
        FamilyAnimationSpec,
    ) {
        let mut semantics = SemanticStore::new();
        let text_leaf = semantics.insert_authoring_object();
        let circle_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, text_leaf).unwrap();
        semantics.add_member(family, circle_leaf).unwrap();

        let mut texts = TextResourceArena::new();
        let text_handle = texts.insert(text_resource()).unwrap();
        let text = RetainedObjectDefinition::text(ObjectId::new(10), text_handle);
        let circle =
            RetainedObjectDefinition::geometry(ObjectId::new(11), GeometryRef::circle(1.0));
        let unrelated =
            RetainedObjectDefinition::geometry(ObjectId::new(12), GeometryRef::circle(2.0));
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, family).unwrap();
        builder.accept_leaf(text_leaf, &text, &texts).unwrap();
        builder.accept_leaf(circle_leaf, &circle, &texts).unwrap();
        let plan = builder.finish().unwrap();
        let compiled = CompiledScene::compile_objects(
            [text, circle, unrelated]
                .into_iter()
                .map(|object| {
                    CompiledObject::new(object.id, object.content, object.transform, object.style)
                })
                .collect(),
            &[],
        )
        .unwrap();
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
        (compiled, plan, spec)
    }

    #[test]
    fn one_shared_state_drives_every_planned_leaf_and_global_member_progress() {
        let (compiled, plan, spec) = fixture();
        let mut runtime = RetainedFamilyPlanSceneInstance::new(compiled, plan, spec).unwrap();
        let plan = runtime.plan().clone();
        let frame = runtime.evaluate(2.0).unwrap();

        let shared = spec.state_at(2.0).unwrap();
        assert_eq!(frame.family_animation(0), Some(shared));
        assert_eq!(frame.family_animation(1), Some(shared));
        assert_eq!(frame.family_animation(2), None);

        let text = frame.planned_family_leaf(&plan, 0).unwrap().unwrap();
        let circle = frame.planned_family_leaf(&plan, 1).unwrap().unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }

    #[test]
    fn direct_seek_matches_forward_playback_and_dirties_only_plan_leaves() {
        let (compiled, plan, spec) = fixture();
        let mut forward =
            RetainedFamilyPlanSceneInstance::new(compiled.clone(), plan.clone(), spec).unwrap();
        forward.take_frame_changes();
        forward.advance_to(1.5).unwrap();
        assert_eq!(forward.take_frame_changes().object_indices(), &[0, 1]);
        let forward_frame = forward.advance_to(2.0).unwrap();
        let forward_states = forward_frame.family_animations.to_vec();

        let mut direct = RetainedFamilyPlanSceneInstance::new(compiled, plan, spec).unwrap();
        direct.take_frame_changes();
        let direct_frame = direct.seek(2.0).unwrap();
        assert_eq!(direct_frame.family_animations, forward_states);
        assert!(direct.take_frame_changes().is_all());
    }

    #[test]
    fn pre_and_post_interval_state_is_absent_and_boundaries_are_exact() {
        let (compiled, plan, spec) = fixture();
        let mut runtime = RetainedFamilyPlanSceneInstance::new(compiled, plan, spec).unwrap();
        assert_eq!(runtime.evaluate(0.5).unwrap().family_animation(0), None);
        assert_eq!(
            runtime.evaluate(1.0).unwrap().family_animation(0),
            Some(spec.state_at(1.0).unwrap())
        );
        assert_eq!(
            runtime.evaluate(3.0).unwrap().family_animation(0),
            Some(spec.state_at(3.0).unwrap())
        );
        assert_eq!(runtime.evaluate(3.1).unwrap().family_animation(0), None);
    }

    #[test]
    fn unknown_plan_object_fails_before_runtime_construction() {
        let (compiled, _, spec) = fixture();
        let mut semantics = SemanticStore::new();
        let leaf = semantics.insert_authoring_object();
        let missing =
            RetainedObjectDefinition::geometry(ObjectId::new(99), GeometryRef::circle(1.0));
        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, leaf).unwrap();
        builder
            .accept_leaf(leaf, &missing, &TextResourceArena::new())
            .unwrap();
        let plan = builder.finish().unwrap();
        assert_eq!(
            RetainedFamilyPlanSceneInstance::new(compiled, plan, spec).unwrap_err(),
            RetainedFamilyPlanRuntimeError::UnknownObject(ObjectId::new(99))
        );
    }
}

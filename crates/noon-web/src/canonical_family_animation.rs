use noon::RetainedScene;
use noon_core::{
    FamilyAnimationSpec, ObjectId, RetainedFamilyAnimationPlan,
    RetainedFamilyAnimationRequestPlanError, TrackDefinition,
};
use noon_ir::{SceneSpec, SceneSpecError};

use crate::{
    retained_scene_spec_runtime::CanonicalRetainedAuthoringScene, MixedRetainedAuthoringError,
};

/// One canonical family animation after source-level scene materialization.
///
/// The request is gone at this point. The execution side retains only the immutable
/// retained plan plus target-independent timing semantics consumed by the shared family
/// runtime/player.
#[derive(Clone, Debug)]
pub struct CanonicalRetainedFamilyAnimation {
    plan: RetainedFamilyAnimationPlan,
    spec: FamilyAnimationSpec,
}

impl CanonicalRetainedFamilyAnimation {
    pub fn plan(&self) -> &RetainedFamilyAnimationPlan {
        &self.plan
    }

    pub const fn spec(&self) -> FamilyAnimationSpec {
        self.spec
    }
}

/// Canonical mixed scene with semantic-family requests lowered against its own retained
/// resource arenas.
///
/// Source-level `SceneSpec` text is compiled by the established canonical materializer
/// first. Only then are family member descriptors resolved, so no shaped glyph/font/
/// atlas identity ever needs to cross the authoring boundary. This is the production
/// handoff consumed by the canonical engine-selection layer: it exposes retained scene,
/// normalized tracks/camera identity, and prepared family animations without exposing
/// the private source-materialization implementation.
#[derive(Clone, Debug)]
pub struct CanonicalRetainedFamilyAnimationScene {
    scene: CanonicalRetainedAuthoringScene,
    animations: Vec<CanonicalRetainedFamilyAnimation>,
}

impl CanonicalRetainedFamilyAnimationScene {
    pub fn from_scene_spec(
        mut spec: SceneSpec,
    ) -> Result<Self, CanonicalRetainedFamilyAnimationSceneError> {
        spec.validate()?;
        let requests = std::mem::take(&mut spec.family_animations);
        let scene = CanonicalRetainedAuthoringScene::from_scene_spec(spec)?;
        let animations = requests
            .into_iter()
            .map(|request| {
                let spec = request.spec();
                let plan = RetainedFamilyAnimationPlan::from_request(
                    &request,
                    scene.scene().objects(),
                    scene.scene().texts(),
                )?;
                Ok(CanonicalRetainedFamilyAnimation { plan, spec })
            })
            .collect::<Result<Vec<_>, CanonicalRetainedFamilyAnimationSceneError>>()?;
        Ok(Self { scene, animations })
    }

    pub const fn retained_scene(&self) -> &RetainedScene {
        self.scene.scene()
    }

    pub fn tracks(&self) -> &[TrackDefinition] {
        self.scene.tracks()
    }

    pub const fn camera_object(&self) -> Option<ObjectId> {
        self.scene.camera_object()
    }

    pub fn animations(&self) -> &[CanonicalRetainedFamilyAnimation] {
        &self.animations
    }
}

#[derive(Debug)]
pub enum CanonicalRetainedFamilyAnimationSceneError {
    SceneSpec(SceneSpecError),
    Authoring(MixedRetainedAuthoringError),
    Plan(RetainedFamilyAnimationRequestPlanError),
}

impl std::fmt::Display for CanonicalRetainedFamilyAnimationSceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneSpec(error) => error.fmt(formatter),
            Self::Authoring(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CanonicalRetainedFamilyAnimationSceneError {}

impl From<SceneSpecError> for CanonicalRetainedFamilyAnimationSceneError {
    fn from(value: SceneSpecError) -> Self {
        Self::SceneSpec(value)
    }
}

impl From<MixedRetainedAuthoringError> for CanonicalRetainedFamilyAnimationSceneError {
    fn from(value: MixedRetainedAuthoringError) -> Self {
        Self::Authoring(value)
    }
}

impl From<RetainedFamilyAnimationRequestPlanError> for CanonicalRetainedFamilyAnimationSceneError {
    fn from(value: RetainedFamilyAnimationRequestPlanError) -> Self {
        Self::Plan(value)
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FamilyAnimationLeafBinding, FamilyAnimationMode, FamilyAnimationRequest,
        FamilyAnimationSpec, GeometryRef, ObjectId, RateFunction, SemanticStore,
    };
    use noon_ir::{ObjectSpec, SceneSpec, TextSpec};

    use super::*;

    #[test]
    fn canonical_mixed_text_geometry_request_resolves_after_text_materialization() {
        let text_id = ObjectId::new(1_u64 << 52);
        let circle_id = ObjectId::new(7);
        let mut scene_spec = SceneSpec::new(
            vec![
                ObjectSpec::text(
                    text_id,
                    TextSpec::native_plain("AB", noon::DEFAULT_NATIVE_TEXT_FONT_FAMILY, 48.0, -1.0),
                ),
                ObjectSpec::geometry(circle_id, GeometryRef::circle(1.0)),
            ],
            Vec::new(),
        )
        .unwrap();

        let mut semantics = SemanticStore::new();
        let text_leaf = semantics.insert_authoring_object();
        let circle_leaf = semantics.insert_authoring_object();
        let family = semantics.insert_family();
        semantics.add_member(family, text_leaf).unwrap();
        semantics.add_member(family, circle_leaf).unwrap();
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
        let request = FamilyAnimationRequest::from_semantic_bindings(
            &semantics,
            family,
            spec,
            [
                // Bind in the opposite order to prove frontend/materialization order
                // cannot replace authoritative SemanticStore family order.
                FamilyAnimationLeafBinding::new(circle_leaf, circle_id),
                FamilyAnimationLeafBinding::new(text_leaf, text_id),
            ],
        )
        .unwrap();
        scene_spec.family_animations.push(request);

        let lowered = CanonicalRetainedFamilyAnimationScene::from_scene_spec(scene_spec).unwrap();
        assert_eq!(
            lowered
                .retained_scene()
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![text_id, circle_id]
        );
        assert_eq!(lowered.animations().len(), 1);

        let animation = &lowered.animations()[0];
        assert_eq!(animation.plan().member_plan().target(), family);
        assert_eq!(animation.plan().member_plan().total_member_count(), 3);
        assert_eq!(animation.plan().leaves()[0].span().semantic_leaf, text_leaf);
        assert_eq!(animation.plan().leaves()[0].span().object, text_id);
        assert_eq!(animation.plan().leaves()[0].span().member_count, 2);
        assert_eq!(
            animation.plan().leaves()[1].span().semantic_leaf,
            circle_leaf
        );
        assert_eq!(animation.plan().leaves()[1].span().object, circle_id);
        assert_eq!(animation.plan().leaves()[1].span().member_count, 1);

        let midpoint = animation.spec().state_at(2.0).unwrap();
        let text = animation
            .plan()
            .leaf_frame_for_object(midpoint, text_id)
            .unwrap();
        let circle = animation
            .plan()
            .leaf_frame_for_object(midpoint, circle_id)
            .unwrap();
        assert_eq!(text.member_progress(0).unwrap(), 1.0);
        assert_eq!(text.member_progress(1).unwrap(), 0.5);
        assert_eq!(circle.member_progress(0).unwrap(), 0.0);
    }
}

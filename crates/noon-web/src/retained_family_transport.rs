use std::collections::HashSet;

use noon_core::{
    FamilyAnimationError, FamilyAnimationState, ObjectId, RetainedFamilyAnimationMemberPlanError,
    RetainedFamilyAnimationPlan, RetainedFamilyAnimationPlanBuilder, RetainedObjectDefinition,
    SemanticStore, SemanticStoreError, TextResourceLookup,
};
use noon_runtime::FrameState;
use serde::{Deserialize, Serialize};

/// Per-object content-independent family-animation state carried at frame time.
///
/// The transport deliberately contains only the evaluated scheduler state. Content
/// member identity stays in the immutable retained resources and the installed global
/// member plan, so geometry/Text share one wire shape without glyph IDs or renderer
/// payloads entering frame deltas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RetainedFamilyTransportState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_animation: Option<FamilyAnimationState>,
}

impl RetainedFamilyTransportState {
    pub const fn empty() -> Self {
        Self {
            family_animation: None,
        }
    }

    pub fn new(
        family_animation: Option<FamilyAnimationState>,
    ) -> Result<Self, RetainedFamilyTransportError> {
        let state = Self { family_animation };
        state.validate()?;
        Ok(state)
    }

    /// Revalidate scheduler state after any serialization boundary.
    pub fn validate(self) -> Result<(), RetainedFamilyTransportError> {
        if let Some(state) = self.family_animation {
            state
                .validate()
                .map_err(RetainedFamilyTransportError::InvalidState)?;
        }
        Ok(())
    }
}

/// Immutable wire description of one already-flattened semantic family plan.
///
/// The engine sends retained leaf order and, for a projected leaf, its global range.
/// The render worker rebuilds
/// the core plan once from the resolved snapshot + immutable text resources, so shaped
/// glyph descriptors never cross the wire and frame-time scheduling never recomputes
/// semantic traversal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedFamilyPlanTransport {
    pub objects: Vec<ObjectId>,
    /// A single resident leaf may keep its range in a larger global sequence.
    /// Glyph identities remain derived from immutable worker-local resources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_span: Option<RetainedFamilyGlobalSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedFamilyGlobalSpan {
    pub first_member: u32,
    pub total_member_count: u32,
}

impl RetainedFamilyPlanTransport {
    pub fn new(objects: Vec<ObjectId>) -> Result<Self, RetainedFamilyTransportError> {
        let plan = Self {
            objects,
            global_span: None,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn from_plan(plan: &RetainedFamilyAnimationPlan) -> Self {
        let spans = plan.member_plan().leaves();
        let global_span = match spans {
            [span]
                if span.first_member != 0
                    || span.member_count != plan.member_plan().total_member_count() =>
            {
                Some(RetainedFamilyGlobalSpan {
                    first_member: span.first_member,
                    total_member_count: plan.member_plan().total_member_count(),
                })
            }
            _ => None,
        };
        Self {
            objects: plan
                .member_plan()
                .leaves()
                .iter()
                .map(|leaf| leaf.object)
                .collect(),
            global_span,
        }
    }

    pub fn validate(&self) -> Result<(), RetainedFamilyTransportError> {
        if self.objects.is_empty() {
            return Err(RetainedFamilyTransportError::EmptyPlan);
        }
        if self.global_span.is_some_and(|span| {
            self.objects.len() != 1 || span.first_member > span.total_member_count
        }) {
            return Err(RetainedFamilyTransportError::InvalidGlobalSpan);
        }
        let mut seen = HashSet::with_capacity(self.objects.len());
        for &object in &self.objects {
            if !seen.insert(object) {
                return Err(RetainedFamilyTransportError::DuplicateObject(object));
            }
        }
        Ok(())
    }

    /// Rebuild the canonical retained family plan from authoritative leaf order and
    /// renderer-local retained content handles.
    ///
    /// The temporary semantic store exists only to feed the shared plan builder; it
    /// does not invent a frontend identity model. Leaf order is exactly the engine's
    /// flattened order, while content-local member descriptors are resolved from the
    /// already-installed local resources.
    pub fn install(
        &self,
        frame: &FrameState,
        texts: &(impl TextResourceLookup + ?Sized),
    ) -> Result<RetainedFamilyAnimationPlan, RetainedFamilyTransportError> {
        self.install_with_object_lookup(texts, |object_id| {
            frame.objects.iter().find(|object| object.id == object_id)
        })
    }

    /// Rebuild a plan using an indexed retained-object lookup supplied by the caller.
    ///
    /// Incremental receivers use this form so installing a sparse plan visits only
    /// that plan's leaves instead of searching every resident frame row per leaf.
    pub(crate) fn install_with_object_lookup<'a>(
        &self,
        texts: &(impl TextResourceLookup + ?Sized),
        mut object_for_id: impl FnMut(ObjectId) -> Option<&'a noon_runtime::FrameObjectState>,
    ) -> Result<RetainedFamilyAnimationPlan, RetainedFamilyTransportError> {
        self.validate()?;

        let mut semantics = SemanticStore::new();
        let leaves = self
            .objects
            .iter()
            .map(|_| semantics.insert_authoring_object())
            .collect::<Vec<_>>();
        let target = if leaves.len() == 1 {
            leaves[0]
        } else {
            let family = semantics.insert_family();
            for &leaf in &leaves {
                semantics
                    .add_member(family, leaf)
                    .map_err(RetainedFamilyTransportError::Semantic)?;
            }
            family
        };

        let mut builder = RetainedFamilyAnimationPlanBuilder::begin(&semantics, target)
            .map_err(RetainedFamilyTransportError::Plan)?;
        for (&leaf, &object_id) in leaves.iter().zip(&self.objects) {
            let object = object_for_id(object_id)
                .ok_or(RetainedFamilyTransportError::MissingObject(object_id))?;
            let definition = RetainedObjectDefinition {
                id: object.id,
                content: object.content.clone(),
                transform: object.transform,
                style: object.style,
            };
            builder
                .accept_leaf(leaf, &definition, texts)
                .map_err(RetainedFamilyTransportError::Plan)?;
        }
        let plan = builder
            .finish()
            .map_err(RetainedFamilyTransportError::Plan)?;
        if let Some(span) = self.global_span {
            let leaf = &plan.leaves()[0];
            RetainedFamilyAnimationPlan::single_leaf_span(
                leaf.span().semantic_leaf,
                leaf.span().object,
                leaf.members().clone(),
                span.first_member,
                span.total_member_count,
            )
            .map_err(RetainedFamilyTransportError::Plan)
        } else {
            Ok(plan)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedFamilyTransportError {
    InvalidGlobalSpan,
    EmptyPlan,
    DuplicateObject(ObjectId),
    MissingObject(ObjectId),
    InvalidState(FamilyAnimationError),
    Semantic(SemanticStoreError),
    Plan(RetainedFamilyAnimationMemberPlanError),
}

impl std::fmt::Display for RetainedFamilyTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGlobalSpan => {
                formatter.write_str("invalid single-leaf global member span")
            }
            Self::EmptyPlan => formatter.write_str("retained family transport plan has no leaves"),
            Self::DuplicateObject(object) => write!(
                formatter,
                "retained family transport plan contains object {} more than once",
                object.get()
            ),
            Self::MissingObject(object) => write!(
                formatter,
                "retained family transport plan references missing object {}",
                object.get()
            ),
            Self::InvalidState(error) => {
                write!(
                    formatter,
                    "invalid retained family transport state: {error}"
                )
            }
            Self::Semantic(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RetainedFamilyTransportError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use noon_core::{
        FamilyAnimationMode, FontFaceIdentity, GeometryRef, GlyphRun, ObjectContentRef,
        PositionedGlyph, RateFunction, Rect, Style, TextAffineTransform, TextClusterIdentity,
        TextDirection, TextRenderItem, TextResource, TextResourceArena, TextSourceKind,
        TextSourceSpan, Transform2D, Vec2,
    };
    use noon_runtime::FrameObjectState;

    use super::*;

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

    #[test]
    fn generic_state_round_trips_for_geometry_without_text_special_casing() {
        let transport = RetainedFamilyTransportState::new(Some(state())).unwrap();
        let json = serde_json::to_string(&transport).unwrap();
        assert!(json.contains("family_animation"));
        assert!(!json.contains("glyph"));
        let decoded: RetainedFamilyTransportState = serde_json::from_str(&json).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, transport);
    }

    #[test]
    fn mixed_plan_transport_rebuilds_global_text_and_geometry_member_order() {
        let mut texts = TextResourceArena::new();
        let text = texts.insert(text_resource()).unwrap();
        let text_id = ObjectId::new(10);
        let circle_id = ObjectId::new(11);
        let frame = FrameState {
            family_animations: Vec::new(),
            family_animation_plan_indices: Vec::new(),
            time: 0.0,
            objects: vec![
                FrameObjectState {
                    id: text_id,
                    content: ObjectContentRef::Text(text),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
                FrameObjectState {
                    id: circle_id,
                    content: ObjectContentRef::Geometry(GeometryRef::circle(1.0)),
                    transform: Transform2D::IDENTITY,
                    style: Style::default(),
                    appearance: 1.0,
                    text_bounds: None,
                },
            ],
            presences: vec![true, true],
            reveals: vec![1.0, 1.0],
            morphs: vec![0.0, 0.0],
            render_geometries: vec![None, None],
            render_transforms: vec![None, None],
        };
        let transport = RetainedFamilyPlanTransport::new(vec![text_id, circle_id]).unwrap();
        let json = serde_json::to_string(&transport).unwrap();
        assert!(!json.contains("glyph"));

        let installed = transport.install(&frame, &texts).unwrap();
        assert_eq!(installed.member_plan().total_member_count(), 3);
        assert_eq!(
            installed
                .member_plan()
                .leaves()
                .iter()
                .map(|leaf| leaf.object)
                .collect::<Vec<_>>(),
            vec![text_id, circle_id]
        );
        let text_leaf = installed.leaf_for_object(text_id).unwrap();
        assert_eq!(text_leaf.members().member_count(), 2);
        let circle_leaf = installed.leaf_for_object(circle_id).unwrap();
        assert_eq!(circle_leaf.members().member_count(), 1);
    }

    #[test]
    fn malformed_plan_descriptors_fail_before_installation() {
        let object = ObjectId::new(7);
        assert_eq!(
            RetainedFamilyPlanTransport::new(Vec::new()).unwrap_err(),
            RetainedFamilyTransportError::EmptyPlan
        );
        assert_eq!(
            RetainedFamilyPlanTransport::new(vec![object, object]).unwrap_err(),
            RetainedFamilyTransportError::DuplicateObject(object)
        );
    }

    #[test]
    fn single_leaf_span_transport_preserves_global_timing_and_checks_resource_bounds() {
        let mut texts = TextResourceArena::new();
        let text = texts.insert(text_resource()).unwrap();
        let object = FrameObjectState {
            id: ObjectId::new(12),
            content: ObjectContentRef::Text(text),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            text_bounds: None,
        };
        let members =
            noon_core::RetainedAnimationMembers::resolve(&object.content, &texts).unwrap();
        let plan = RetainedFamilyAnimationPlan::single_leaf_span(
            noon_core::SemanticNodeId::new(42, 0),
            object.id,
            members,
            3,
            7,
        )
        .unwrap();
        let wire = RetainedFamilyPlanTransport::from_plan(&plan);
        let json = serde_json::to_string(&wire).unwrap();
        assert!(!json.contains("glyph"));
        let mut decoded: RetainedFamilyPlanTransport = serde_json::from_str(&json).unwrap();
        let installed = decoded
            .install_with_object_lookup(&texts, |id| (id == object.id).then_some(&object))
            .unwrap();
        assert_eq!(installed.leaves().len(), 1);
        assert_eq!(installed.member_plan().total_member_count(), 7);
        assert_eq!(installed.leaves()[0].span().first_member, 3);
        for reverse_member_order in [false, true] {
            let state = FamilyAnimationState {
                reverse_member_order,
                ..state()
            };
            let progress = installed
                .member_plan()
                .leaf_progress(state, installed.leaves()[0].span().semantic_leaf)
                .unwrap();
            for local in 0..2 {
                assert_eq!(
                    progress.member_progress(local).unwrap(),
                    state.member_progress(local + 3, 7).unwrap()
                );
            }
        }
        // A well-formed range still must contain the resource's actual member count.
        decoded.global_span.as_mut().unwrap().total_member_count = 4;
        assert!(matches!(
            decoded.install_with_object_lookup(&texts, |_| Some(&object)),
            Err(RetainedFamilyTransportError::Plan(_))
        ));
        decoded.objects.push(ObjectId::new(13));
        assert_eq!(
            decoded.validate(),
            Err(RetainedFamilyTransportError::InvalidGlobalSpan)
        );
    }
}

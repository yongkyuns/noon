use std::sync::Arc;

use noon_compile::{
    lower_semantic_execution, CompilePatchError, CompiledFamilyAnimation, CompiledScene,
    ExecutionPatch, SemanticExecutionIndex,
};
use noon_core::{
    CompositionTimeMap, FamilyAnimationMode, FamilyAnimationSpec, FontFaceIdentity,
    FontResourceArena, GeometryResourceArena, GlyphRun, ObjectId, PositionedGlyph, RateFunction,
    Rect, RetainedAnimationMembers, RetainedFamilyAnimationPlan, SemanticObjectState,
    SemanticStore, StoredGeometry, TextAffineTransform, TextClusterIdentity, TextDirection,
    TextRenderItem, TextResource, TextSourceKind, TextSourceSpan, Vec2,
};
use noon_runtime::{RetainedFamilyFrame, SceneInstance};

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

// The fixture reaches runtime through ordinary semantic lowering; the two leaf
// channels share one global member span and the existing main scheduler.
fn fixture() -> (
    CompiledScene,
    Vec<RetainedFamilyAnimationPlan>,
    FamilyAnimationSpec,
) {
    let mut store = SemanticStore::new();
    let resource = text_resource();
    let mut fonts = FontResourceArena::new();
    // This scheduler fixture consumes already-shaped glyph metadata, not font
    // outlines. Retain its explicit font dependency through normal lowering.
    fonts
        .intern_face(&resource.runs[0].font, Arc::<[u8]>::from([1_u8, 2, 3]))
        .unwrap();
    let handle = store
        .import_text_resource(resource, &fonts, &GeometryResourceArena::new())
        .unwrap();
    let text = store.insert_semantic_object(SemanticObjectState::new(handle));
    let circle = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    let unrelated =
        store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
            radius: 2.0,
        }));
    for node in [text, circle, unrelated] {
        store.attach_to_scene(node).unwrap();
    }
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let (mut compiled, _) = lowered.into_parts();
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
    let mut plans = Vec::new();
    for (node, row, first_member) in [(text, 0, 0), (circle, 1, 2)] {
        let object = &compiled.objects()[row];
        let target = index.execution_object_id(node).unwrap();
        assert_eq!(object.id, target);
        let members =
            RetainedAnimationMembers::resolve(&object.content, compiled.text_resources()).unwrap();
        let plan =
            RetainedFamilyAnimationPlan::single_leaf_span(node, target, members, first_member, 3)
                .unwrap();
        compiled
            .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
                CompiledFamilyAnimation {
                    target,
                    plan: plan.clone(),
                    spec,
                    time_map: CompositionTimeMap::identity(),
                },
            ))
            .unwrap();
        plans.push(plan);
    }
    (compiled, plans, spec)
}

#[test]
fn main_scheduler_drives_text_and_geometry_in_one_global_family_span() {
    let (compiled, plans, spec) = fixture();
    let mut runtime = SceneInstance::new(compiled);
    let frame = runtime.seek(2.0).unwrap();
    let shared = spec.state_at(2.0).unwrap();
    assert_eq!(frame.family_animations, [Some(shared), Some(shared), None]);
    let family = RetainedFamilyFrame {
        retained: frame,
        family_animations: &frame.family_animations,
    };
    let text = family.planned_family_leaf(&plans[0], 0).unwrap().unwrap();
    let circle = family.planned_family_leaf(&plans[1], 1).unwrap().unwrap();
    assert_eq!(text.member_progress(0).unwrap(), 1.0);
    assert_eq!(text.member_progress(1).unwrap(), 0.5);
    assert_eq!(circle.member_progress(0).unwrap(), 0.0);
}

#[test]
fn family_forward_seek_rewind_and_endpoint_expiry_agree() {
    let (compiled, _, spec) = fixture();
    let mut forward = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);
    forward.take_frame_changes();
    forward.advance_to(0.5).unwrap();
    assert!(forward.take_frame_changes().is_empty());
    for time in [1.0, 1.5, 2.0, 3.0, 3.1] {
        forward.advance_to(time).unwrap();
        direct.seek(time).unwrap();
        assert_eq!(forward.frame(), direct.frame());
        assert_eq!(forward.take_frame_changes().object_indices(), &[0, 1]);
        assert_eq!(forward.frame().family_animations[2], None);
        let expected = if time <= 3.0 {
            Some(spec.state_at(time).unwrap())
        } else {
            None
        };
        assert_eq!(forward.frame().family_animations[0], expected);
    }
    forward.advance_to(4.0).unwrap();
    assert!(forward.take_frame_changes().is_empty());
    assert!(forward.active_family_animation_indices().is_empty());
    direct.seek(2.0).unwrap();
    assert_eq!(
        direct.frame().family_animations[0],
        Some(spec.state_at(2.0).unwrap())
    );
    direct.seek(0.5).unwrap();
    assert!(direct.frame().family_animations.iter().all(Option::is_none));
}

#[test]
fn adjacent_family_channels_select_the_new_start_on_shared_scheduler() {
    let (mut compiled, plans, _) = fixture();
    let target = compiled.objects()[0].id;
    let next = FamilyAnimationSpec::new(
        FamilyAnimationMode::Reveal,
        3.0,
        1.0,
        1.0,
        RateFunction::Linear,
        true,
        true,
    )
    .unwrap();
    compiled
        .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
            CompiledFamilyAnimation {
                target,
                plan: plans[0].clone(),
                spec: next,
                time_map: CompositionTimeMap::identity(),
            },
        ))
        .unwrap();
    let mut runtime = SceneInstance::new(compiled);
    runtime.seek(3.0).unwrap();
    let state = runtime.frame().family_animations[0].unwrap();
    assert_eq!(state, next.state_at(3.0).unwrap());
    assert!(state.reverse_rate_function);
    assert!(state.reverse_member_order);
}

#[test]
fn invalid_family_channel_is_rejected_without_publishing() {
    let (compiled, plans, spec) = fixture();
    let mut runtime = SceneInstance::new(compiled);
    runtime.seek(2.0).unwrap();
    runtime.take_frame_changes();
    let before = runtime.frame().clone();
    let publication = runtime.publication_context();
    let missing = ObjectId::new(99);
    assert_eq!(
        runtime
            .apply_execution_patch(&ExecutionPatch::AddFamilyAnimation(
                CompiledFamilyAnimation {
                    target: missing,
                    plan: plans[0].clone(),
                    spec,
                    time_map: CompositionTimeMap::identity(),
                }
            ))
            .unwrap_err(),
        CompilePatchError::UnknownObject(missing)
    );
    assert_eq!(runtime.frame(), &before);
    assert_eq!(runtime.publication_context(), publication);
    assert!(runtime.take_frame_changes().is_empty());
}

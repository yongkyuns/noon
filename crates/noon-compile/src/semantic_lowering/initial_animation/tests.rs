use std::sync::Arc;

use noon_core::{
    AnimationOptions, CompositionTimeMap, FamilyAnimationMode, FontFaceIdentity, FontResourceArena,
    GeometryResourceArena, GlyphRun, PositionedGlyph, Property, RateFunction, Rect,
    SemanticAnimationCompositionKind, SemanticFamilyAnimationMember, SemanticMutationTransaction,
    SemanticObjectState, SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticStore,
    SemanticVec3, StoredGeometry, TextAffineTransform, TextClusterIdentity, TextDirection,
    TextRenderItem, TextResource, TextSourceKind, TextSourceSpan, TrackTiming, Vec2,
};

use crate::{
    lower_semantic_execution_root, lower_semantic_execution_root_with_animation_root_at,
    SemanticExecutionIndex, SemanticExecutionLoweringError, SemanticInitialAnimationError,
    TextGlyphLoweringError,
};

fn plain_text(store: &mut SemanticStore, source: &str) -> noon_core::SemanticNodeId {
    let face = FontFaceIdentity {
        family: Arc::from("Initial Import Test"),
        face_key: Arc::from("initial-import-test-face"),
        face_index: 0,
        variation_key: Arc::from(""),
    };
    let mut fonts = FontResourceArena::new();
    fonts.intern_face(&face, Arc::<[u8]>::from([1_u8])).unwrap();
    let glyphs = source
        .char_indices()
        .enumerate()
        .map(|(index, (start, character))| PositionedGlyph {
            glyph_id: u32::try_from(index + 1).unwrap(),
            cluster: TextClusterIdentity {
                source_span: TextSourceSpan::new(
                    u32::try_from(start).unwrap(),
                    u32::try_from(start + character.len_utf8()).unwrap(),
                ),
                cluster_ordinal: u32::try_from(index).unwrap(),
                semantic_key: None,
            },
            origin: Vec2::new(index as f32, 0.0),
            advance: Vec2::ONE,
            bounds: Rect::new(
                Vec2::new(index as f32, 0.0),
                Vec2::new(index as f32 + 1.0, 1.0),
            ),
        })
        .collect::<Vec<_>>();
    let handle = store
        .import_text_resource(
            TextResource {
                source: Arc::from(source),
                kind: TextSourceKind::Plain,
                runs: Arc::from([GlyphRun {
                    font: face,
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
                bounds: Rect::new(Vec2::ZERO, Vec2::new(source.len() as f32, 1.0)),
                baseline: 0.0,
                layout_artifact: None,
            },
            &fonts,
            &GeometryResourceArena::new(),
        )
        .unwrap();
    store.insert_semantic_object(SemanticObjectState::new(handle))
}

fn neutral_family_options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::Linear)
        .lag_ratio(0.25)
        .reverse_rate_function(true)
        .introducer(false)
        .remover(false)
}

#[test]
fn mixed_family_and_exact_track_install_with_independent_absolute_time() {
    let mut store = SemanticStore::new();
    let scene_root = store.insert_family();
    let text = plain_text(&mut store, "AB");
    let geometry = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 1.0,
    }));
    store.add_semantic_family_member(scene_root, text).unwrap();
    store
        .add_semantic_family_member(scene_root, geometry)
        .unwrap();

    let mut transaction = SemanticMutationTransaction::new();
    let text_reveal = transaction.create_family_animation_member(
        text,
        FamilyAnimationMode::Reveal,
        true,
        SemanticFamilyAnimationMember {
            family: scene_root,
            leaf_index: 0,
        },
        neutral_family_options(),
    );
    let geometry_reveal = transaction.create_family_animation_member(
        geometry,
        FamilyAnimationMode::Reveal,
        true,
        SemanticFamilyAnimationMember {
            family: scene_root,
            leaf_index: 1,
        },
        neutral_family_options(),
    );
    let family_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [text_reveal, geometry_reveal],
        AnimationOptions::new().rate_func(RateFunction::Linear),
    );
    let exact_timing = TrackTiming::new(3.0, 1.5, RateFunction::Smooth);
    let exact = transaction.create_object_property_track(
        geometry,
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::ZERO,
            to: SemanticVec3::new(4.0, 0.0, 0.0),
        },
        exact_timing,
        CompositionTimeMap::identity(),
    );
    let initial_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [family_root, exact],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let initial_root = committed.resolve(initial_root).unwrap();

    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution_root_with_animation_root_at(
        &store,
        scene_root,
        &mut index,
        initial_root,
        -1.0,
    )
    .unwrap();
    assert_eq!(lowered.compiled().family_animations().len(), 2);
    assert!(lowered
        .compiled()
        .family_animations()
        .iter()
        .all(|animation| animation.spec.start_time == -1.0
            && animation.spec.duration == 2.0
            && animation.spec.reverse_rate_function
            && animation.spec.reverse_member_order));
    assert!(lowered
        .compiled()
        .family_animation_plans()
        .iter()
        .all(|plan| plan.member_plan().total_member_count() == 3));
    let exact = lowered
        .compiled()
        .tracks_iter()
        .find(|track| track.property == Property::Position)
        .unwrap();
    assert_eq!(exact.timing, exact_timing);
}

#[test]
fn aggregate_family_overlap_rejects_without_publishing_staged_identity() {
    let mut store = SemanticStore::new();
    let prior_root = store.insert_family();
    let prior = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius: 0.5,
    }));
    store.add_semantic_family_member(prior_root, prior).unwrap();

    let scene_root = store.insert_family();
    let text = plain_text(&mut store, "AB");
    store.add_semantic_family_member(scene_root, text).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    let request = |transaction: &mut SemanticMutationTransaction| {
        let leaf = transaction.create_family_animation_member(
            text,
            FamilyAnimationMode::DrawBorderThenFill,
            false,
            SemanticFamilyAnimationMember {
                family: scene_root,
                leaf_index: 0,
            },
            neutral_family_options(),
        );
        transaction.create_animation_composition(
            SemanticAnimationCompositionKind::Parallel,
            [leaf],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
    };
    let first = request(&mut transaction);
    let second = request(&mut transaction);
    let initial_root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [first, second],
        AnimationOptions::new(),
    );
    let committed = transaction.apply(&mut store).unwrap();
    let initial_root = committed.resolve(initial_root).unwrap();

    let mut index = SemanticExecutionIndex::new();
    lower_semantic_execution_root(&store, prior_root, &mut index).unwrap();
    let prior_id = index.execution_object_id(prior).unwrap();
    assert!(matches!(
        lower_semantic_execution_root_with_animation_root_at(
            &store,
            scene_root,
            &mut index,
            initial_root,
            0.0,
        ),
        Err(SemanticExecutionLoweringError::InitialAnimation(
            SemanticInitialAnimationError::Family(
                TextGlyphLoweringError::ConflictingObjectDrivers { .. }
            )
        ))
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(index.execution_object_id(prior), Some(prior_id));
    assert_eq!(index.execution_object_id(text), None);
}

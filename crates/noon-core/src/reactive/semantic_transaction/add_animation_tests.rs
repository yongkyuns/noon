use super::*;
use crate::{
    AnimationOptions, CompositionTimeMapStep, FamilyAnimationMode, NativeStateSource, RateFunction,
    SemanticAffineLifecycleDirection, SemanticAffineLifecycleEndpoint,
    SemanticAnimationCompositionKind, SemanticAnimationIntent, SemanticAnimationState,
    SemanticFadeDirection, SemanticFamilyAnimationMember, SemanticObjectState,
    SemanticObjectTrackProperty, SemanticObjectTrackValues, SemanticVec3, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius }))
}

fn transform(store: &mut SemanticStore, radius: f32) -> SemanticNodeId {
    let target = object(store, radius);
    let target_state = object(store, radius + 0.5);
    store
        .insert_semantic_transform_animation(target, target_state, AnimationOptions::new())
        .unwrap()
}

fn transform_state(
    target: SemanticNodeId,
    target_state: SemanticNodeId,
    options: AnimationOptions,
) -> SemanticAnimationState {
    SemanticAnimationState::new(
        SemanticAnimationIntent::TransformTo {
            target,
            target_state,
            interpolation: SemanticTransformInterpolation::Affine,
        },
        options,
    )
}

fn scalar_input(store: &SemanticStore, signal: SemanticNodeId) -> f64 {
    let SemanticSignalSource::Input(SemanticSignalValue::Scalar(value)) =
        store.semantic_signal_state(signal).unwrap().source()
    else {
        panic!("expected scalar input signal")
    };
    *value
}

#[test]
fn add_animation_commits_one_authored_animation_and_reports_its_identity() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let state = transform_state(
        target,
        target_state,
        AnimationOptions::new().run_time(1.5).lag_ratio(0.25),
    );
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(state.clone());
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!("expected one animation-added impact")
    };
    assert_eq!(store.semantic_animation_state(*animation).unwrap(), &state);
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn prepared_fade_retains_one_semantic_target_and_direction() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let local = transaction.create_fade_animation(
        target,
        SemanticFadeDirection::Out,
        AnimationOptions::new().run_time(0.75),
    );

    let result = transaction.apply(&mut store).unwrap();
    let animation = result.resolve(local).unwrap();
    assert_eq!(
        store.semantic_animation_state(animation).unwrap().intent(),
        &SemanticAnimationIntent::Fade {
            target,
            direction: SemanticFadeDirection::Out,
            endpoint: crate::SemanticFadeEndpoint::default(),
        }
    );
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_affine_fade_endpoint_rolls_back_before_identity_allocation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_fade_animation_with_endpoint(
        target,
        SemanticFadeDirection::Out,
        crate::SemanticFadeEndpoint {
            scale_factor: f64::NAN,
            translation: crate::SemanticFadeTranslation::Shift(SemanticVec3::ZERO),
            scale_center: SemanticVec3::ZERO,
        },
        AnimationOptions::new(),
    );

    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidFadeEndpoint { index: 0 })
    ));
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn prepared_draw_border_then_fill_preserves_shared_outline_semantics() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    let local = transaction.create_draw_border_then_fill_animation(
        target,
        0.125,
        Some(crate::BLUE),
        RateFunction::Smooth,
        AnimationOptions::new().run_time(2.0).introducer(true),
    );

    let result = transaction.apply(&mut store).unwrap();
    let animation = result.resolve(local).unwrap();
    assert_eq!(
        store.semantic_animation_state(animation).unwrap().intent(),
        &SemanticAnimationIntent::DrawBorderThenFill {
            target,
            stroke_width: 0.125,
            stroke_color: Some(crate::BLUE),
            phase_rate_function: RateFunction::Smooth,
        }
    );
    assert_eq!(store.len(), before_len + 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_draw_border_then_fill_outline_rolls_back_before_identity_allocation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_draw_border_then_fill_animation(
        target,
        -0.5,
        None,
        RateFunction::Smooth,
        AnimationOptions::new(),
    );

    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidDrawBorderThenFillOutline { index: 0 })
    ));
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn prepared_subset_member_preserves_shared_index_and_threshold_mode() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let mut transaction = SemanticMutationTransaction::new();
    let local = transaction.create_subset_display_member_animation(
        target,
        1,
        3,
        crate::SemanticSubsetDisplayMode::OneByOneCeil,
        AnimationOptions::new().introducer(true),
    );
    let result = transaction.apply(&mut store).unwrap();
    assert_eq!(
        store
            .semantic_animation_state(result.resolve(local).unwrap())
            .unwrap()
            .intent(),
        &SemanticAnimationIntent::SubsetDisplayMember {
            target,
            index: 1,
            count: 3,
            mode: crate::SemanticSubsetDisplayMode::OneByOneCeil,
        }
    );
}

#[test]
fn invalid_subset_member_rolls_back_before_identity_allocation() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_subset_display_member_animation(
        target,
        2,
        2,
        crate::SemanticSubsetDisplayMode::IncreasingFloor,
        AnimationOptions::new(),
    );
    assert!(matches!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidSubsetDisplayMember { index: 0 })
    ));
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn add_animation_preserves_composition_order_and_unresolved_options() {
    let mut store = SemanticStore::new();
    let first = transform(&mut store, 1.0);
    let second = transform(&mut store, 2.0);
    let options = AnimationOptions::new().run_time(3.0).path_arc(0.75);
    let state = SemanticAnimationState::new(
        SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            children: vec![second, first],
        },
        options,
    );

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(state.clone());
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation }] = result.impacts() else {
        panic!("expected one animation-added impact")
    };
    assert_eq!(store.semantic_animation_state(*animation).unwrap(), &state);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_animation_target_rolls_back_earlier_mutation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_animation(transform_state(target, family, AnimationOptions::new()));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationTarget {
            index: 1,
            error: SemanticSceneOperationError::NotSemanticObject(family),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn stale_animation_reference_rolls_back_before_allocation() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let target = object(&mut store, 1.0);
    let stale = object(&mut store, 2.0);
    store.remove_node(stale).unwrap();
    let replacement = object(&mut store, 3.0);
    assert_eq!(stale.slot(), replacement.slot());
    assert_ne!(stale.generation(), replacement.generation());
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .set_signal(signal, 2.0_f64)
        .add_animation(transform_state(target, stale, AnimationOptions::new()));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationTarget {
            index: 1,
            error: SemanticSceneOperationError::UnknownNode(stale),
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn malformed_animation_options_use_equality_safe_transaction_errors() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let before_len = store.len();

    let mut invalid_run_time = SemanticMutationTransaction::new();
    invalid_run_time.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().run_time(0.0),
    ));
    assert_eq!(
        invalid_run_time.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationRunTime { index: 0 })
    );

    let mut invalid_lag_ratio = SemanticMutationTransaction::new();
    invalid_lag_ratio.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().lag_ratio(-0.1),
    ));
    assert_eq!(
        invalid_lag_ratio.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationLagRatio { index: 0 })
    );

    let mut invalid_path_arc = SemanticMutationTransaction::new();
    invalid_path_arc.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new().path_arc(f64::NAN),
    ));
    assert_eq!(
        invalid_path_arc.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAnimationPathArc { index: 0 })
    );

    assert_eq!(store.len(), before_len);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn animation_cannot_reference_a_node_removed_by_the_same_transaction() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ))
        .remove_node(target_state);

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::AnimationUsesRemovedNode {
            index: 0,
            node: target_state,
        })
    );
    assert_eq!(store.len(), before_len);
    assert!(store.node(target_state).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn structural_removal_then_add_animation_is_rejected_before_commit() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let unrelated = object(&mut store, 3.0);
    let before_len = store.len();

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .remove_node(unrelated)
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ));

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::MutationAfterRemove { index: 1 })
    );
    assert_eq!(store.len(), before_len);
    assert!(store.node(unrelated).is_some());
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn repeated_identical_additions_create_distinct_semantic_identities() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let state = transform_state(target, target_state, AnimationOptions::new());

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(state.clone())
        .add_animation(state);
    let result = transaction.apply(&mut store).unwrap();

    let [SemanticMutationImpact::AnimationAdded { animation: first }, SemanticMutationImpact::AnimationAdded { animation: second }] =
        result.impacts()
    else {
        panic!("expected two animation-added impacts")
    };
    assert_ne!(first, second);
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn add_animation_can_precede_an_unrelated_terminal_removal() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let target_state = object(&mut store, 2.0);
    let unrelated = object(&mut store, 3.0);

    let mut transaction = SemanticMutationTransaction::new();
    transaction
        .add_animation(transform_state(
            target,
            target_state,
            AnimationOptions::new(),
        ))
        .remove_node(unrelated);
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(result.impacts().len(), 2);
    assert!(matches!(
        result.impacts()[0],
        SemanticMutationImpact::AnimationAdded { .. }
    ));
    assert_eq!(
        result.impacts()[1],
        SemanticMutationImpact::NodeRemoved { node: unrelated }
    );
    assert_eq!(store.last_mutation_stats().slots_written, 2);
}

#[test]
fn add_animation_is_local_with_large_unrelated_scene() {
    let mut store = SemanticStore::new();
    for index in 0..10_000 {
        object(&mut store, index as f32 + 1.0);
    }
    let target = object(&mut store, 0.25);
    let target_state = object(&mut store, 0.5);

    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_animation(transform_state(
        target,
        target_state,
        AnimationOptions::new(),
    ));
    let result = transaction.apply(&mut store).unwrap();

    assert_eq!(result.impacts().len(), 1);
    assert!(matches!(
        result.impacts()[0],
        SemanticMutationImpact::AnimationAdded { .. }
    ));
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn pending_animation_tree_reads_and_commits_through_one_identity_map() {
    let mut store = SemanticStore::new();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let source = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    let first_target = transaction.create_node(SemanticNodeCreation::object(
        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }),
    ));
    let second_target = transaction.create_node(SemanticNodeCreation::object(
        SemanticObjectState::new(StoredGeometry::Circle { radius: 1.5 }),
    ));
    let first = transaction.create_transform_animation(
        source,
        first_target,
        AnimationOptions::new().run_time(1.0),
    );
    let second = transaction.create_transform_animation(
        source,
        second_target,
        AnimationOptions::new().run_time(2.0),
    );
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [first, second],
        AnimationOptions::new().lag_ratio(1.0),
    );

    let prepared = transaction.prepare(&mut store).unwrap();
    assert_eq!(prepared.store().len(), 0);
    assert_eq!(
        prepared.pending_animation(root).unwrap().intent(),
        &SemanticTransactionAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            children: vec![first.into(), second.into()],
        }
    );

    let result = prepared.commit();
    let source = result.resolve(source).unwrap();
    let first_target = result.resolve(first_target).unwrap();
    let second_target = result.resolve(second_target).unwrap();
    let first = result.resolve(first).unwrap();
    let second = result.resolve(second).unwrap();
    let root = result.resolve(root).unwrap();
    assert_eq!(
        store.semantic_animation_state(first).unwrap().intent(),
        &SemanticAnimationIntent::TransformTo {
            target: source,
            target_state: first_target,
            interpolation: SemanticTransformInterpolation::Affine,
        }
    );
    assert_eq!(
        store.semantic_animation_state(second).unwrap().intent(),
        &SemanticAnimationIntent::TransformTo {
            target: source,
            target_state: second_target,
            interpolation: SemanticTransformInterpolation::Affine,
        }
    );
    assert_eq!(
        store.semantic_animation_state(root).unwrap().intent(),
        &SemanticAnimationIntent::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            children: vec![first, second],
        }
    );
    assert_eq!(
        store.scene_revision(),
        before_revision.checked_next().unwrap()
    );
    assert_eq!(store.last_mutation_stats().slots_written, 6);
    assert_eq!(result.impacts().len(), 6);
}

#[test]
fn composition_rejects_foreign_pending_animation_without_allocating() {
    let mut store = SemanticStore::new();
    let source = object(&mut store, 0.5);
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let before_revision = store.scene_revision();

    let mut owner = SemanticMutationTransaction::new();
    let foreign = owner.create_transform_animation(source, target, AnimationOptions::new());
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [foreign],
        AnimationOptions::new(),
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(
            SemanticMutationTransactionError::PendingNodeFromDifferentTransaction {
                index: 0,
                token: foreign,
            },
        )
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn composition_rejects_pending_node_of_the_wrong_kind() {
    let mut store = SemanticStore::new();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let object = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [object],
        AnimationOptions::new(),
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::PendingNodeKindMismatch {
            index: 1,
            token: object,
            expected: SemanticPendingNodeKind::Animation,
        })
    );
    assert_eq!(store.len(), 0);
    assert_eq!(store.scene_revision(), before_revision);
}

#[test]
fn malformed_forward_animation_reference_fails_before_commit() {
    let mut store = SemanticStore::new();
    let source = object(&mut store, 0.5);
    let target = object(&mut store, 1.0);
    let before_len = store.len();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let future = SemanticLocalNodeToken::new(transaction.id, transaction.next_token + 1);
    transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [future],
        AnimationOptions::new(),
    );
    let actual = transaction.create_transform_animation(source, target, AnimationOptions::new());
    assert_eq!(future, actual);

    assert_eq!(
        transaction.apply(&mut store),
        Err(
            SemanticMutationTransactionError::PendingAnimationForwardReference {
                index: 0,
                animation: future,
            },
        )
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn late_invalid_composition_rolls_back_earlier_writes_and_declarations() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(1.0_f64).unwrap();
    let source = object(&mut store, 0.5);
    let target = object(&mut store, 1.0);
    let family = store.insert_family();
    let before_len = store.len();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_signal(signal, 2.0_f64);
    let animation = transaction.create_transform_animation(source, target, AnimationOptions::new());
    transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [
            SemanticTransactionNodeRef::from(animation),
            SemanticTransactionNodeRef::from(family),
        ],
        AnimationOptions::new(),
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::NotAnimation {
            index: 2,
            animation: family,
        })
    );
    assert_eq!(scalar_input(&store, signal), 1.0);
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn canceling_pending_animation_dependency_cancels_parent_compositions() {
    let mut store = SemanticStore::new();
    let mut transaction = SemanticMutationTransaction::new();
    let source = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 0.5 },
    )));
    let target = transaction.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let leaf = transaction.create_transform_animation(source, target, AnimationOptions::new());
    let inner = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [leaf],
        AnimationOptions::new(),
    );
    let outer = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [inner],
        AnimationOptions::new(),
    );
    transaction.remove_node(target);

    let result = transaction.apply(&mut store).unwrap();
    let source = result.resolve(source).unwrap();
    assert_eq!(result.resolve(target), None);
    assert_eq!(result.resolve(leaf), None);
    assert_eq!(result.resolve(inner), None);
    assert_eq!(result.resolve(outer), None);
    assert_eq!(
        result.impacts(),
        &[SemanticMutationImpact::NodeAdded { node: source }]
    );
    assert_eq!(store.len(), 1);
    assert_eq!(store.last_mutation_stats().slots_written, 1);
}

#[test]
fn invalid_affine_lifecycle_endpoint_is_atomic() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 0.5);
    let before_len = store.len();
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.create_affine_lifecycle_animation(
        target,
        SemanticAffineLifecycleDirection::IntroduceFrom,
        SemanticAffineLifecycleEndpoint {
            point: SemanticVec3::new(f64::NAN, 0.0, 0.0),
            rotation_offset: 0.0,
            point_color: None,
        },
        AnimationOptions::new(),
    );

    assert_eq!(
        transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidAffineLifecycleEndpoint { index: 0 })
    );
    assert_eq!(store.len(), before_len);
    assert_eq!(store.scene_revision(), before_revision);
    assert_eq!(store.last_mutation_stats().slots_written, 0);
}

#[test]
fn nested_add_and_wait_declarations_commit_in_one_ordered_transaction() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 0.5);
    let before_revision = store.scene_revision();
    let mut transaction = SemanticMutationTransaction::new();
    let add = transaction.create_add_animation(target, AnimationOptions::new().run_time(0.2));
    let first_wait = transaction.create_wait_animation(0.2);
    let nested = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [first_wait],
        AnimationOptions::new(),
    );
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Sequence,
        [add, nested],
        AnimationOptions::new(),
    );

    let result = transaction.apply(&mut store).unwrap();
    let root = result.resolve(root).unwrap();
    let nested = result.resolve(nested).unwrap();
    let add = result.resolve(add).unwrap();
    let first_wait = result.resolve(first_wait).unwrap();
    assert_eq!(
        store
            .semantic_animation_state(root)
            .unwrap()
            .intent()
            .children(),
        &[add, nested]
    );
    assert_eq!(
        store
            .semantic_animation_state(nested)
            .unwrap()
            .intent()
            .children(),
        &[first_wait]
    );
    assert_eq!(
        store.semantic_animation_state(add).unwrap().intent(),
        &SemanticAnimationIntent::Add { target }
    );
    assert_eq!(
        store.semantic_animation_state(first_wait).unwrap().intent(),
        &SemanticAnimationIntent::Wait
    );
    assert_eq!(store.scene_revision().get(), before_revision.get() + 1);
}

#[test]
fn instant_add_accepts_zero_duration_without_relaxing_other_animation_leaves() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 0.5);
    let mut transaction = SemanticMutationTransaction::new();
    let add = transaction.create_add_animation(target, AnimationOptions::new().run_time(0.0));
    let result = transaction.apply(&mut store).unwrap();
    let add = result.resolve(add).unwrap();
    assert_eq!(
        store
            .semantic_animation_state(add)
            .unwrap()
            .options()
            .run_time,
        Some(0.0)
    );
}

#[test]
fn exact_family_members_accept_geometry_and_preserve_independent_reversal_and_lifecycle() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 0.5);
    let family = store.insert_family();
    store.add_semantic_family_member(family, target).unwrap();
    let member = SemanticFamilyAnimationMember {
        family,
        leaf_index: 0,
    };
    let reveal_options = AnimationOptions::new()
        .run_time(2.0)
        .rate_func(RateFunction::RushInto)
        .lag_ratio(0.25)
        .reverse_rate_function(false)
        .introducer(false)
        .remover(false);
    let border_options = AnimationOptions::new()
        .run_time(3.0)
        .rate_func(RateFunction::Smooth)
        .lag_ratio(0.5)
        .reverse_rate_function(true)
        .introducer(false)
        .remover(false);

    let mut transaction = SemanticMutationTransaction::new();
    let reveal = transaction.create_family_animation_member(
        target,
        FamilyAnimationMode::Reveal,
        true,
        member,
        reveal_options,
    );
    let border = transaction.create_family_animation_member(
        target,
        FamilyAnimationMode::DrawBorderThenFill,
        false,
        member,
        border_options,
    );
    let committed = transaction.apply(&mut store).unwrap();

    for (token, mode, reverse_member_order, options) in [
        (reveal, FamilyAnimationMode::Reveal, true, reveal_options),
        (
            border,
            FamilyAnimationMode::DrawBorderThenFill,
            false,
            border_options,
        ),
    ] {
        let animation = store
            .semantic_animation_state(committed.resolve(token).unwrap())
            .unwrap();
        assert_eq!(
            animation.intent(),
            &SemanticAnimationIntent::TextGlyph {
                target,
                mode,
                reverse_member_order,
                family_member: Some(member),
            }
        );
        assert_eq!(animation.options(), options);
    }
}

#[test]
fn scalar_animation_rejects_native_and_non_finite_targets_atomically() {
    let mut store = SemanticStore::new();
    let signal = store.insert_semantic_input_signal(0.0_f64).unwrap();
    store
        .bind_semantic_native_state_input(
            signal,
            NativeStateSource::Control {
                name: "speed".to_owned(),
            },
        )
        .unwrap();
    let revision = store.scene_revision();
    let mut native = SemanticMutationTransaction::new();
    native.create_scalar_animation(signal, 1.0, AnimationOptions::new());
    assert!(matches!(
        native.apply(&mut store),
        Err(SemanticMutationTransactionError::SignalTrack {
            error: SemanticScalarSignalTrackError::NativeOwnedSignal(actual),
            ..
        }) if actual == signal
    ));
    assert_eq!(store.scene_revision(), revision);

    store.clear_semantic_native_input(signal).unwrap();
    let revision = store.scene_revision();
    let mut invalid = SemanticMutationTransaction::new();
    invalid.create_scalar_animation(signal, f64::NAN, AnimationOptions::new());
    assert!(matches!(
        invalid.apply(&mut store),
        Err(SemanticMutationTransactionError::SignalTrack {
            error: SemanticScalarSignalTrackError::NonFiniteValue { signal: actual, .. },
            ..
        }) if actual == signal
    ));
    assert_eq!(store.scene_revision(), revision);
}

#[test]
fn exact_object_tracks_retain_high_precision_timing_and_semantic_identity() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
        0.25,
        0.5,
        RateFunction::Smooth,
    )]);
    let timing = TrackTiming::new(-0.5, 3.0, RateFunction::RushInto);
    let mut transaction = SemanticMutationTransaction::new();
    let leaf = transaction.create_object_property_track(
        target,
        SemanticObjectTrackProperty::Position,
        SemanticObjectTrackValues::Vec3 {
            from: SemanticVec3::new(1.0 / 3.0, -2.0, 0.0),
            to: SemanticVec3::new(4.0, 5.0, 0.0),
        },
        timing,
        map.clone(),
    );
    let root = transaction.create_animation_composition(
        SemanticAnimationCompositionKind::Parallel,
        [leaf],
        AnimationOptions::new(),
    );

    let result = transaction.apply(&mut store).unwrap();
    let leaf = result.resolve(leaf).unwrap();
    let root = result.resolve(root).unwrap();
    assert_eq!(
        store.semantic_animation_state(leaf).unwrap(),
        &SemanticAnimationState::new(
            SemanticAnimationIntent::ObjectPropertyTrack {
                target,
                property: SemanticObjectTrackProperty::Position,
                values: SemanticObjectTrackValues::Vec3 {
                    from: SemanticVec3::new(1.0 / 3.0, -2.0, 0.0),
                    to: SemanticVec3::new(4.0, 5.0, 0.0),
                },
                timing,
                time_map: map,
            },
            AnimationOptions::new(),
        )
    );
    assert_eq!(
        store
            .semantic_animation_state(root)
            .unwrap()
            .intent()
            .children(),
        &[leaf]
    );
}

#[test]
fn exact_object_track_validation_is_atomic_and_checks_endpoint_provenance() {
    let mut store = SemanticStore::new();
    let target = object(&mut store, 1.0);
    let from = object(&mut store, 2.0);
    let to = object(&mut store, 3.0);
    let revision = store.scene_revision();
    let before_len = store.len();
    let mut mismatch = SemanticMutationTransaction::new();
    mismatch.create_object_property_track(
        target,
        SemanticObjectTrackProperty::Fill,
        SemanticObjectTrackValues::Scalar { from: 0.0, to: 1.0 },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    assert!(matches!(
        mismatch.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidObjectPropertyTrack { .. })
    ));
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.len(), before_len);

    let family = store.insert_family();
    store.add_semantic_family_member(family, to).unwrap();
    let revision = store.scene_revision();
    let mut attached_endpoint = SemanticMutationTransaction::new();
    attached_endpoint.create_object_property_track(
        target,
        SemanticObjectTrackProperty::Transform,
        SemanticObjectTrackValues::Object {
            from: from.into(),
            to: to.into(),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    assert!(matches!(
        attached_endpoint.apply(&mut store),
        Err(SemanticMutationTransactionError::InvalidObjectPropertyTrack { .. })
    ));
    assert_eq!(store.scene_revision(), revision);

    let mut foreign = SemanticMutationTransaction::new();
    let foreign_endpoint = foreign.create_node(SemanticNodeCreation::object(
        SemanticObjectState::new(StoredGeometry::Circle { radius: 4.0 }),
    ));
    let mut wrong_transaction = SemanticMutationTransaction::new();
    wrong_transaction.create_object_property_track(
        target,
        SemanticObjectTrackProperty::Transform,
        SemanticObjectTrackValues::Object {
            from: from.into(),
            to: foreign_endpoint.into(),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    assert!(matches!(
        wrong_transaction.apply(&mut store),
        Err(SemanticMutationTransactionError::PendingNodeFromDifferentTransaction { .. })
    ));
    assert_eq!(store.scene_revision(), revision);

    let mut local_endpoints = SemanticMutationTransaction::new();
    let local_from = local_endpoints.create_node(SemanticNodeCreation::object(
        SemanticObjectState::new(StoredGeometry::Circle { radius: 5.0 }),
    ));
    let local_to = local_endpoints.create_node(SemanticNodeCreation::object(
        SemanticObjectState::new(StoredGeometry::Circle { radius: 6.0 }),
    ));
    let animation = local_endpoints.create_object_property_track(
        target,
        SemanticObjectTrackProperty::Transform,
        SemanticObjectTrackValues::Object {
            from: local_from.into(),
            to: local_to.into(),
        },
        TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        CompositionTimeMap::identity(),
    );
    let committed = local_endpoints.apply(&mut store).unwrap();
    let SemanticAnimationIntent::ObjectPropertyTrack {
        values: SemanticObjectTrackValues::Object { from, to },
        ..
    } = store
        .semantic_animation_state(committed.resolve(animation).unwrap())
        .unwrap()
        .intent()
    else {
        panic!("expected exact object-valued track")
    };
    assert_eq!(*from, committed.resolve(local_from).unwrap());
    assert_eq!(*to, committed.resolve(local_to).unwrap());
}

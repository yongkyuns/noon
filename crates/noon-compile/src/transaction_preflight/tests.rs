#![cfg(test)]

// Fixtures originate in shared semantic lowering. Preflight and the sequential
// compiler oracle both consume the typed execution mutation vocabulary.
use noon_core::{
    Easing, GeometryRef, ObjectId, SemanticObjectState, SemanticStore, StoredGeometry, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues,
};

use crate::{
    lower_semantic_execution, CompilePatchError, CompiledObject, CompiledScene,
    ExecutionMutationTransaction, ExecutionPatch, SemanticExecutionIndex,
};

fn compiled_circles(radii: impl IntoIterator<Item = f32>) -> (CompiledScene, Vec<ObjectId>) {
    let mut store = SemanticStore::new();
    let nodes = radii
        .into_iter()
        .map(|radius| {
            let node =
                store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius,
                }));
            store.attach_semantic_object(node).unwrap();
            node
        })
        .collect::<Vec<_>>();
    let mut index = SemanticExecutionIndex::new();
    let (compiled, _) = lower_semantic_execution(&store, &mut index)
        .unwrap()
        .into_parts();
    let objects = nodes
        .into_iter()
        .map(|node| index.execution_object_id(node).unwrap())
        .collect();
    (compiled, objects)
}

fn add_position(compiled: &mut CompiledScene, object: ObjectId, id: u64) -> TrackId {
    let id = TrackId::new(id);
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id,
            object,
            property: noon_core::Property::Position,
            values: TrackValues::Vec2 {
                from: noon_core::Vec2::ZERO,
                to: noon_core::Vec2::ONE,
            },
            timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::identity(),
        }))
        .unwrap();
    id
}

fn add_presence(
    compiled: &mut CompiledScene,
    object: ObjectId,
    id: u64,
    from: bool,
    to: bool,
    time: f64,
) -> TrackId {
    let id = TrackId::new(id);
    compiled
        .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
            id,
            object,
            property: noon_core::Property::Presence,
            values: TrackValues::Bool { from, to },
            timing: TrackTiming::instant(time),
            time_map: noon_core::CompositionTimeMap::identity(),
        }))
        .unwrap();
    id
}

#[test]
fn property_batch_ignores_one_hundred_thousand_unrelated_objects_and_tracks() {
    let (mut compiled, objects) = compiled_circles(std::iter::repeat_n(1.0, 100_000));
    for (id, object) in objects.iter().take(1_000).enumerate() {
        add_position(&mut compiled, *object, id as u64);
    }
    let target = objects[99_999];
    let track = add_position(&mut compiled, target, 1_000);

    let stats = compiled
        .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::SetStyle {
                object: target,
                style: Style::default(),
            },
        ]))
        .unwrap();

    assert_eq!(stats.objects_indexed, 1);
    assert_eq!(stats.tracks_indexed, 0);
    assert_eq!(stats.track_metadata_visits, 0);
    assert_eq!(stats.mutations_preflighted, 1);
    assert_eq!(stats.staged_compiled_scene_clones, 0);
    assert_eq!(compiled.track(track).unwrap().object_index, 99_999);
}

#[test]
fn late_presence_replacement_rejects_without_touching_unrelated_tracks() {
    let (mut compiled, objects) = compiled_circles([1.0, 1.0]);
    let (target, unrelated) = (objects[0], objects[1]);
    let first = add_presence(&mut compiled, target, 0, false, true, 1.0);
    let second = add_presence(&mut compiled, target, 1, true, false, 2.0);
    let unrelated_track = add_position(&mut compiled, unrelated, 2);
    let before = compiled.track(second).unwrap().clone();

    let error = compiled
        .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::SetStyle {
                object: unrelated,
                style: Style::default(),
            },
            ExecutionPatch::ReplaceTrack(TrackDefinition {
                id: second,
                object: target,
                property: noon_core::Property::Presence,
                values: TrackValues::Bool {
                    from: false,
                    to: false,
                },
                timing: TrackTiming::instant(2.0),
                time_map: noon_core::CompositionTimeMap::identity(),
            }),
        ]))
        .unwrap_err();

    assert_eq!(
        error,
        CompilePatchError::DiscontinuousPresence {
            previous: first,
            next: second,
        }
    );
    assert_eq!(compiled.track(second), Some(&before));
    assert!(compiled.track(unrelated_track).is_some());
}

#[test]
fn mapped_presence_preflight_orders_by_compiled_event_boundary() {
    let (mut compiled, objects) = compiled_circles([1.0]);
    let target = objects[0];
    for (id, from, to, time) in [(0, false, true, 1.0), (1, true, false, 2.0)] {
        compiled
            .apply_execution_patch(&ExecutionPatch::AddTrack(TrackDefinition {
                id: TrackId::new(id),
                object: target,
                property: noon_core::Property::Presence,
                values: TrackValues::Bool { from, to },
                timing: TrackTiming::instant(time),
                time_map: noon_core::CompositionTimeMap::identity(),
            }))
            .unwrap();
    }
    let mapped = TrackDefinition {
        id: TrackId::new(2),
        object: target,
        property: noon_core::Property::Presence,
        values: TrackValues::Bool {
            from: false,
            to: true,
        },
        timing: TrackTiming::new(0.0, 6.0, Easing::Linear),
        time_map: noon_core::CompositionTimeMap::from_steps(vec![
            noon_core::CompositionTimeMapStep::new(0.5, 0.5, Easing::Linear),
        ]),
    };
    let transaction =
        ExecutionMutationTransaction::from_mutations([ExecutionPatch::AddTrack(mapped)]);

    compiled
        .preflight_execution_transaction(&transaction)
        .unwrap();
    for patch in transaction.mutations() {
        compiled.apply_execution_patch(patch).unwrap();
    }
    assert_eq!(
        compiled.track(TrackId::new(2)).unwrap().timing,
        TrackTiming::instant(3.0)
    );
}

#[test]
fn unsupported_mapped_presence_transaction_is_atomic() {
    let (compiled, objects) = compiled_circles([1.0]);
    let before = compiled.clone();
    let transaction =
        ExecutionMutationTransaction::from_mutations([ExecutionPatch::AddTrack(TrackDefinition {
            id: TrackId::new(0),
            object: objects[0],
            property: noon_core::Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
            time_map: noon_core::CompositionTimeMap::from_steps(vec![
                noon_core::CompositionTimeMapStep::new(
                    0.0,
                    1.0,
                    noon_core::RateFunction::ThereAndBack,
                ),
            ]),
        })]);

    assert!(matches!(
        compiled.preflight_execution_transaction(&transaction),
        Err(CompilePatchError::InvalidTrack(_))
    ));
    assert_eq!(compiled, before);
}

#[test]
fn remove_recreate_and_track_replacement_use_only_transaction_overlay() {
    let (mut compiled, objects) = compiled_circles([1.0]);
    let object = objects[0];
    let track = add_position(&mut compiled, object, 0);

    let replacement = TrackDefinition {
        id: track,
        object,
        property: noon_core::Property::Position,
        values: TrackValues::Vec2 {
            from: noon_core::Vec2::ONE,
            to: noon_core::Vec2::new(2.0, 2.0),
        },
        timing: TrackTiming::new(1.0, 1.0, Easing::Linear),
        time_map: noon_core::CompositionTimeMap::identity(),
    };
    let stats = compiled
        .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::RemoveObject(object),
            ExecutionPatch::CreateObject(CompiledObject::new(
                object,
                GeometryRef::circle(2.0),
                noon_core::Transform2D::IDENTITY,
                Style::default(),
            )),
            ExecutionPatch::AddTrack(replacement.clone()),
            ExecutionPatch::ReplaceTrack(replacement),
        ]))
        .unwrap();

    assert_eq!(stats.objects_indexed, 1);
    assert_eq!(stats.tracks_indexed, 1);
    assert_eq!(stats.staged_compiled_scene_clones, 0);
}

#[test]
fn removing_original_owner_preserves_a_track_moved_earlier_in_the_batch() {
    let (mut compiled, objects) = compiled_circles([1.0, 2.0]);
    let (first, second) = (objects[0], objects[1]);
    let track = add_position(&mut compiled, first, 0);
    let moved = TrackDefinition {
        id: track,
        object: second,
        property: noon_core::Property::Position,
        values: TrackValues::Vec2 {
            from: noon_core::Vec2::ZERO,
            to: noon_core::Vec2::ONE,
        },
        timing: TrackTiming::new(0.0, 1.0, Easing::Linear),
        time_map: noon_core::CompositionTimeMap::identity(),
    };
    let prefix = [
        ExecutionPatch::ReplaceTrack(moved.clone()),
        ExecutionPatch::RemoveObject(first),
    ];
    for last in [
        ExecutionPatch::RemoveTrack(track),
        ExecutionPatch::ReplaceTrack(moved.clone()),
    ] {
        let mutations = prefix.clone().into_iter().chain([last]).collect::<Vec<_>>();
        compiled
            .preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations(
                mutations.clone(),
            ))
            .unwrap();
        let mut applied = compiled.clone();
        for patch in mutations {
            applied.apply_execution_patch(&patch).unwrap();
        }
    }
    assert_eq!(
        compiled.preflight_execution_transaction(&ExecutionMutationTransaction::from_mutations(
            prefix.into_iter().chain([ExecutionPatch::AddTrack(moved)])
        )),
        Err(CompilePatchError::DuplicateTrack(track))
    );
}

#[test]
fn presence_validation_counts_only_the_affected_base_channel() {
    let (mut compiled, objects) = compiled_circles([1.0, 2.0]);
    let (target, unrelated) = (objects[0], objects[1]);
    add_presence(&mut compiled, target, 0, false, true, 1.0);
    add_presence(&mut compiled, target, 1, true, false, 2.0);
    for index in 0..1_000 {
        add_presence(
            &mut compiled,
            unrelated,
            index + 2,
            index % 2 != 0,
            index % 2 == 0,
            index as f64,
        );
    }
    let transaction =
        ExecutionMutationTransaction::from_mutations([ExecutionPatch::AddTrack(TrackDefinition {
            id: noon_core::TrackId::new(10_000),
            object: target,
            property: noon_core::Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::instant(3.0),
            time_map: noon_core::CompositionTimeMap::identity(),
        })]);
    let stats = compiled
        .preflight_execution_transaction(&transaction)
        .unwrap();
    assert_eq!(stats.tracks_indexed, 2);
    assert_eq!(stats.objects_indexed, 1);
}

#[test]
fn sparse_preflight_agrees_with_sequential_compiler_validation() {
    let (mut compiled, objects) = compiled_circles([1.0, 2.0]);
    add_position(&mut compiled, objects[0], 0);
    // A deterministic model comparison explores interactions between moved
    // tracks, deleted/recreated objects, and presence-channel replacements.
    let mut random = 37_u64;
    for _ in 0..2_000 {
        let mut patches = Vec::new();
        for _ in 0..6 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let object = objects[((random >> 32) & 1) as usize];
            let id = noon_core::TrackId::new((random >> 40) % 4);
            let presence = random & 8 != 0;
            let track = TrackDefinition {
                id,
                object,
                property: if presence {
                    noon_core::Property::Presence
                } else {
                    noon_core::Property::Position
                },
                values: if presence {
                    TrackValues::Bool {
                        from: random & 16 != 0,
                        to: random & 32 != 0,
                    }
                } else {
                    TrackValues::Vec2 {
                        from: noon_core::Vec2::ZERO,
                        to: noon_core::Vec2::ONE,
                    }
                },
                timing: if presence {
                    TrackTiming::instant((random >> 48) as f64 % 4.0)
                } else {
                    TrackTiming::new(0.0, 1.0, Easing::Linear)
                },
                time_map: noon_core::CompositionTimeMap::identity(),
            };
            patches.push(match (random >> 24) % 6 {
                0 => ExecutionPatch::CreateObject(CompiledObject::new(
                    object,
                    GeometryRef::circle(3.0),
                    noon_core::Transform2D::IDENTITY,
                    Style::default(),
                )),
                1 => ExecutionPatch::RemoveObject(object),
                2 => ExecutionPatch::AddTrack(track),
                3 => ExecutionPatch::ReplaceTrack(track),
                4 => ExecutionPatch::RemoveTrack(id),
                _ => ExecutionPatch::SetStyle {
                    object,
                    style: Style::default(),
                },
            });
        }
        let mut reference = compiled.clone();
        let expected = patches
            .iter()
            .try_for_each(|patch| reference.apply_execution_patch(patch));
        let transaction = ExecutionMutationTransaction::from_mutations(patches);
        let actual = compiled
            .preflight_execution_transaction(&transaction)
            .map(|_| ());
        assert_eq!(actual, expected, "transaction: {transaction:?}");
    }
}

#[test]
fn independent_presence_edits_visit_linear_metadata_in_a_large_batch() {
    let (mut compiled, objects) = compiled_circles(std::iter::repeat_n(1.0, 1_000));
    for (id, object) in objects.iter().enumerate() {
        add_presence(&mut compiled, *object, id as u64, false, true, 1.0);
    }
    for size in [16, 1_000] {
        let transaction = ExecutionMutationTransaction::from_mutations(
            objects
                .iter()
                .take(size)
                .enumerate()
                .map(|(index, object)| {
                    ExecutionPatch::AddTrack(TrackDefinition {
                        id: noon_core::TrackId::new(10_000 + index as u64),
                        object: *object,
                        property: noon_core::Property::Presence,
                        values: TrackValues::Bool {
                            from: true,
                            to: false,
                        },
                        timing: TrackTiming::instant(2.0),
                        time_map: noon_core::CompositionTimeMap::identity(),
                    })
                }),
        );
        let stats = compiled
            .preflight_execution_transaction(&transaction)
            .unwrap();
        assert_eq!(stats.objects_indexed, size);
        assert_eq!(stats.tracks_indexed, size);
        assert_eq!(stats.track_metadata_visits, size * 3);
    }
}

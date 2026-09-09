use std::collections::{HashMap, HashSet};

use noon_core::{SemanticObjectState, StoredGeometry};

use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, RateFunction, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticNodeId, SemanticObjectProperty, SemanticSignalValue,
    SemanticStore, SemanticStoreError, SemanticVec3, SourceIdentity,
};

#[derive(Clone, Copy, Debug)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn index(&mut self, len: usize) -> usize {
        assert!(len > 0);
        self.next() as usize % len
    }

    fn scalar(&mut self) -> f32 {
        let raw = (self.next() % 20_001) as f32;
        (raw - 10_000.0) / 1000.0
    }
}

#[derive(Clone, Debug)]
struct ModelNode {
    id: SemanticNodeId,
    family: bool,
    live: bool,
    parents: Vec<SemanticNodeId>,
    members: Vec<SemanticNodeId>,
    source: Option<String>,
}

fn object() -> SemanticObjectState {
    SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 })
}

fn model_index(nodes: &[ModelNode], id: SemanticNodeId) -> usize {
    nodes
        .iter()
        .position(|node| node.id == id)
        .expect("generated handle belongs to model")
}

fn model_reaches(nodes: &[ModelNode], start: SemanticNodeId, target: SemanticNodeId) -> bool {
    let mut stack = vec![start];
    let mut seen = HashSet::new();
    while let Some(current) = stack.pop() {
        if !seen.insert(current) {
            continue;
        }
        if current == target {
            return true;
        }
        let node = &nodes[model_index(nodes, current)];
        if node.live {
            stack.extend(node.members.iter().copied());
        }
    }
    false
}

fn remove_from_model(nodes: &mut [ModelNode], id: SemanticNodeId) {
    let index = model_index(nodes, id);
    let parents = nodes[index].parents.clone();
    let members = nodes[index].members.clone();
    for parent in parents {
        let parent = model_index(nodes, parent);
        nodes[parent].members.retain(|candidate| *candidate != id);
    }
    for member in members {
        let member = model_index(nodes, member);
        nodes[member].parents.retain(|candidate| *candidate != id);
    }
    nodes[index].live = false;
    nodes[index].parents.clear();
    nodes[index].members.clear();
    nodes[index].source = None;
}

fn assert_store_matches_model(store: &SemanticStore, nodes: &[ModelNode], seed: u64, step: usize) {
    let live = nodes.iter().filter(|node| node.live).count();
    assert_eq!(store.len(), live, "seed={seed} step={step}: live count");

    for expected in nodes {
        let actual = store.node(expected.id);
        if !expected.live {
            assert!(
                actual.is_none(),
                "seed={seed} step={step}: stale handle {:?} remained live",
                expected.id
            );
            continue;
        }

        let actual = actual.unwrap_or_else(|| {
            panic!(
                "seed={seed} step={step}: live model handle {:?} disappeared",
                expected.id
            )
        });
        assert_eq!(
            actual.parents(),
            expected.parents,
            "seed={seed} step={step}: parents for {:?}",
            expected.id
        );
        assert_eq!(
            actual.members(),
            expected.members,
            "seed={seed} step={step}: members for {:?}",
            expected.id
        );
        match (&expected.source, actual.source_identity()) {
            (None, None) => {}
            (Some(key), Some(SourceIdentity::ExplicitKey(actual))) => {
                assert_eq!(actual, key, "seed={seed} step={step}: source key")
            }
            (expected_source, actual_source) => panic!(
                "seed={seed} step={step}: source mismatch for {:?}: model={expected_source:?} store={actual_source:?}",
                expected.id
            ),
        }
    }
}

#[test]
fn semantic_store_matches_reference_model_across_seeded_mutation_sequences() {
    for seed in 1_u64..=32 {
        let mut rng = Rng::new(seed);
        let mut store = SemanticStore::new();
        let mut nodes: Vec<ModelNode> = Vec::new();

        for step in 0..750 {
            let live_indices = nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| node.live.then_some(index))
                .collect::<Vec<_>>();
            let live_families = live_indices
                .iter()
                .copied()
                .filter(|index| nodes[*index].family)
                .collect::<Vec<_>>();

            match rng.next() % 7 {
                0 | 1 if nodes.len() < 180 => {
                    let family = rng.next().is_multiple_of(4);
                    let id = if family {
                        store.insert_family()
                    } else {
                        store.insert_semantic_object(object())
                    };
                    nodes.push(ModelNode {
                        id,
                        family,
                        live: true,
                        parents: Vec::new(),
                        members: Vec::new(),
                        source: None,
                    });
                }
                2 if !live_families.is_empty() && !live_indices.is_empty() => {
                    let family_index = live_families[rng.index(live_families.len())];
                    let member_index = live_indices[rng.index(live_indices.len())];
                    let family = nodes[family_index].id;
                    let member = nodes[member_index].id;
                    let already_member = nodes[family_index].members.contains(&member);
                    let would_cycle = family == member || model_reaches(&nodes, member, family);

                    let result = store.add_member(family, member);
                    if would_cycle && !already_member {
                        assert!(
                            matches!(result, Err(SemanticStoreError::FamilyCycle { .. })),
                            "seed={seed} step={step}: expected cycle rejection, got {result:?}"
                        );
                    } else {
                        result.unwrap_or_else(|error| {
                            panic!("seed={seed} step={step}: valid family edge failed: {error}")
                        });
                        if !already_member {
                            nodes[family_index].members.push(member);
                            nodes[member_index].parents.push(family);
                        }
                    }
                }
                3 if !live_families.is_empty() && !live_indices.is_empty() => {
                    let family_index = live_families[rng.index(live_families.len())];
                    let member_index = live_indices[rng.index(live_indices.len())];
                    let family = nodes[family_index].id;
                    let member = nodes[member_index].id;
                    let expected = nodes[family_index].members.contains(&member);
                    let removed = store.remove_member(family, member).unwrap_or_else(|error| {
                        panic!("seed={seed} step={step}: remove_member failed: {error}")
                    });
                    assert_eq!(removed, expected, "seed={seed} step={step}: remove result");
                    if expected {
                        nodes[family_index]
                            .members
                            .retain(|candidate| *candidate != member);
                        nodes[member_index]
                            .parents
                            .retain(|candidate| *candidate != family);
                    }
                }
                4 if !live_indices.is_empty() => {
                    let index = live_indices[rng.index(live_indices.len())];
                    let id = nodes[index].id;
                    let key = format!("key-{}", rng.next() % 12);
                    let owner = nodes.iter().position(|node| {
                        node.live && node.source.as_deref() == Some(key.as_str()) && node.id != id
                    });
                    let before = nodes[index].source.clone();
                    let result = store
                        .set_source_identity(id, Some(SourceIdentity::ExplicitKey(key.clone())));
                    if owner.is_some() {
                        assert!(
                            matches!(result, Err(SemanticStoreError::DuplicateSourceIdentity(_))),
                            "seed={seed} step={step}: duplicate source identity was accepted"
                        );
                        assert_eq!(nodes[index].source, before);
                    } else {
                        result.unwrap_or_else(|error| {
                            panic!(
                                "seed={seed} step={step}: unique source identity failed: {error}"
                            )
                        });
                        nodes[index].source = Some(key);
                    }
                }
                5 if !live_indices.is_empty() => {
                    let index = live_indices[rng.index(live_indices.len())];
                    let id = nodes[index].id;
                    store.set_source_identity(id, None).unwrap();
                    nodes[index].source = None;
                }
                6 if live_indices.len() > 2 => {
                    let index = live_indices[rng.index(live_indices.len())];
                    let id = nodes[index].id;
                    store.remove_node(id).unwrap_or_else(|error| {
                        panic!("seed={seed} step={step}: remove_node failed: {error}")
                    });
                    remove_from_model(&mut nodes, id);
                }
                _ => {}
            }

            assert_store_matches_model(&store, &nodes, seed, step);
        }
    }
}

fn generated_property_writes(
    seed: u64,
    object_ids: &[SemanticNodeId],
) -> Vec<(SemanticNodeId, SemanticObjectProperty, SemanticSignalValue)> {
    let mut rng = Rng::new(seed);
    let mut writes = Vec::new();
    for &object in object_ids {
        // One write per object/property is the shared transaction contract.
        writes.extend([
            (
                object,
                SemanticObjectProperty::Translation,
                SemanticVec3::new(rng.scalar().into(), rng.scalar().into(), 0.0).into(),
            ),
            (
                object,
                SemanticObjectProperty::RotationZ,
                f64::from(rng.scalar()).into(),
            ),
            (
                object,
                SemanticObjectProperty::Scale,
                SemanticVec3::new(
                    f64::from(rng.scalar().abs()) + 0.1,
                    f64::from(rng.scalar().abs()) + 0.1,
                    1.0,
                )
                .into(),
            ),
            (
                object,
                SemanticObjectProperty::ObjectOpacity,
                ((rng.next() % 1001) as f64 / 1000.0).into(),
            ),
        ]);
    }
    for index in (1..writes.len()).rev() {
        let other = rng.index(index + 1);
        writes.swap(index, other);
    }
    writes
}

#[test]
fn generated_property_transactions_match_sequential_application_and_rollback() {
    for seed in 1_u64..=48 {
        let mut base = SemanticStore::new();
        let ids = (0..12)
            .map(|index| {
                base.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                    radius: index as f32 + 1.0,
                }))
            })
            .collect::<Vec<_>>();
        let writes = generated_property_writes(seed, &ids);
        let mut sequential = base.clone();
        let mut transaction = SemanticMutationTransaction::new();
        for (object, property, value) in &writes {
            let mut single = SemanticMutationTransaction::new();
            single.set_property(*object, *property, value.clone());
            single.apply(&mut sequential).unwrap();
            transaction.set_property(*object, *property, value.clone());
        }
        let mut transactional = base.clone();
        transaction.apply(&mut transactional).unwrap();
        for &id in &ids {
            assert_eq!(
                transactional.semantic_object_state_checked(id).unwrap(),
                sequential.semantic_object_state_checked(id).unwrap(),
                "seed={seed}: atomic and sequential publication diverged"
            );
        }
        assert_eq!(transactional.last_mutation_stats().slots_written, ids.len());

        let stale = SemanticNodeId::new(u32::MAX - seed as u32, 0);
        for failure_position in [0, 1, writes.len() / 2, writes.len()] {
            let mut rejected = base.clone();
            let before_revision = rejected.scene_revision();
            let mut transaction = SemanticMutationTransaction::new();
            for (index, (object, property, value)) in writes.iter().enumerate() {
                if index == failure_position {
                    transaction.set_property(stale, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
                }
                transaction.set_property(*object, *property, value.clone());
            }
            if failure_position == writes.len() {
                transaction.set_property(stale, SemanticObjectProperty::ObjectOpacity, 0.5_f64);
            }
            assert!(
                transaction.apply(&mut rejected).is_err(),
                "seed={seed}: invalid write committed"
            );
            assert_eq!(rejected.scene_revision(), before_revision);
            assert_eq!(rejected.len(), base.len());
            for &id in &ids {
                assert_eq!(
                    rejected.semantic_object_state_checked(id).unwrap(),
                    base.semantic_object_state_checked(id).unwrap(),
                    "seed={seed}: failure_position={failure_position}: partial property rollback"
                );
            }
        }

        let mut structural = base.clone();
        let before_revision = structural.scene_revision();
        let mut transaction = SemanticMutationTransaction::new();
        transaction
            .add_node(SemanticNodeCreation::object(object()))
            .remove_node(stale);
        assert!(transaction.apply(&mut structural).is_err());
        assert_eq!(structural.scene_revision(), before_revision);
        assert_eq!(structural.len(), base.len());
        for &id in &ids {
            assert_eq!(
                structural.semantic_object_state_checked(id).unwrap(),
                base.semantic_object_state_checked(id).unwrap()
            );
        }
        // A rejected creation must not consume a generational identity.
        assert_eq!(
            structural.insert_semantic_object(object()),
            base.insert_semantic_object(object())
        );
    }
}

fn reference_map(steps: &[CompositionTimeMapStep], input: f32) -> (f32, bool, bool) {
    let mut alpha = input.clamp(0.0, 1.0);
    let mut begun = true;
    let mut finished = input >= 1.0;
    for step in steps {
        if !begun {
            break;
        }
        let warped = f64::from(step.rate_func.evaluate(alpha));
        if warped < step.start {
            alpha = 0.0;
            begun = false;
            finished = false;
            break;
        }
        alpha = ((warped - step.start) / step.duration).clamp(0.0, 1.0) as f32;
        begun = true;
        finished = warped > step.start + step.duration;
    }
    (alpha, begun, finished)
}

#[test]
fn generated_nested_composition_maps_match_simple_reference_evaluator() {
    let rates = [
        RateFunction::Linear,
        RateFunction::Smooth,
        RateFunction::RushInto,
        RateFunction::RushFrom,
        RateFunction::ThereAndBack,
        RateFunction::EaseInOutCubic,
    ];

    for seed in 1_u64..=64 {
        let mut rng = Rng::new(seed);
        let depth = 1 + rng.index(5);
        let mut steps = Vec::with_capacity(depth);
        for _ in 0..depth {
            let start = (rng.next() % 700) as f64 / 1000.0;
            let max_duration = (1.0 - start).max(0.001);
            let fraction = 0.1 + (rng.next() % 901) as f64 / 1000.0 * 0.9;
            let duration = (max_duration * fraction).max(0.000_001);
            steps.push(CompositionTimeMapStep::new(
                start,
                duration,
                rates[rng.index(rates.len())],
            ));
        }
        let map = CompositionTimeMap::from_steps(steps.clone());
        map.validate()
            .unwrap_or_else(|error| panic!("seed={seed}: invalid generated map: {error}"));

        for sample_index in 0..=200 {
            let alpha = sample_index as f32 / 200.0;
            let expected = reference_map(&steps, alpha);
            let actual = map.evaluate(alpha);
            assert!(
                (actual.alpha - expected.0).abs() <= 1e-6,
                "seed={seed} alpha={alpha}: mapped alpha {} != {}",
                actual.alpha,
                expected.0
            );
            assert_eq!(actual.begun, expected.1, "seed={seed} alpha={alpha}: begun");
            assert_eq!(
                actual.finished, expected.2,
                "seed={seed} alpha={alpha}: finished"
            );
        }
    }
}

#[test]
fn source_identity_uniqueness_survives_reassignment_and_slot_reuse() {
    for seed in 1_u64..=64 {
        let mut rng = Rng::new(seed);
        let mut store = SemanticStore::new();
        let mut live = Vec::new();
        let mut expected_owner: HashMap<String, SemanticNodeId> = HashMap::new();

        for _ in 0..40_u64 {
            live.push(store.insert_semantic_object(object()));
        }

        for step in 0..300 {
            if live.is_empty() {
                live.push(store.insert_semantic_object(object()));
            }
            let index = rng.index(live.len());
            let id = live[index];
            if rng.next().is_multiple_of(5) {
                let old = store.node(id).unwrap().source_identity().cloned();
                store.set_source_identity(id, None).unwrap();
                if let Some(SourceIdentity::ExplicitKey(key)) = old {
                    expected_owner.remove(&key);
                }
            } else {
                let key = format!("stable-key-{}", rng.next() % 24);
                match expected_owner.get(&key).copied() {
                    Some(owner) if owner != id => {
                        assert!(matches!(
                            store.set_source_identity(
                                id,
                                Some(SourceIdentity::ExplicitKey(key.clone()))
                            ),
                            Err(SemanticStoreError::DuplicateSourceIdentity(_))
                        ));
                    }
                    _ => {
                        if let Some(SourceIdentity::ExplicitKey(old)) =
                            store.node(id).unwrap().source_identity().cloned()
                        {
                            expected_owner.remove(&old);
                        }
                        store
                            .set_source_identity(id, Some(SourceIdentity::ExplicitKey(key.clone())))
                            .unwrap();
                        expected_owner.insert(key, id);
                    }
                }
            }

            if step % 17 == 0 && live.len() > 4 {
                let removed_index = rng.index(live.len());
                let removed = live.swap_remove(removed_index);
                if let Some(SourceIdentity::ExplicitKey(key)) =
                    store.node(removed).unwrap().source_identity().cloned()
                {
                    expected_owner.remove(&key);
                }
                store.remove_node(removed).unwrap();
                assert!(store.node(removed).is_none(), "seed={seed} step={step}");
                let replacement = store.insert_semantic_object(object());
                assert_eq!(
                    replacement.slot(),
                    removed.slot(),
                    "seed={seed} step={step}"
                );
                assert_ne!(
                    replacement.generation(),
                    removed.generation(),
                    "seed={seed} step={step}"
                );
                live.push(replacement);
            }

            for (key, owner) in &expected_owner {
                assert_eq!(
                    store.node_for_source(&SourceIdentity::ExplicitKey(key.clone())),
                    Some(*owner),
                    "seed={seed} step={step}: source lookup"
                );
            }
        }
    }
}

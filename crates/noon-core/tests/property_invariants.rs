use std::collections::{HashMap, HashSet};

use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, GeometryRef, MutationTransaction, ObjectDefinition,
    ObjectId, RateFunction, SceneDefinition, ScenePatch, SemanticNodeId, SemanticStore,
    SemanticStoreError, SourceIdentity, Style, Transform2D, Vec2,
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

fn object(id: u64) -> ObjectDefinition {
    ObjectDefinition::new(ObjectId::new(id), GeometryRef::circle(1.0))
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
            (expected, actual) => panic!(
                "seed={seed} step={step}: source mismatch for {:?}: model={expected:?} store={actual:?}",
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
        let mut next_object = 0_u64;

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
                    let family = rng.next() % 4 == 0;
                    let id = if family {
                        store.insert_family()
                    } else {
                        let id = store.insert_object(object(next_object));
                        next_object += 1;
                        id
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

fn generated_property_patches(seed: u64, object_ids: &[ObjectId]) -> Vec<ScenePatch> {
    let mut rng = Rng::new(seed);
    let mut patches = Vec::new();
    for _ in 0..96 {
        let object = object_ids[rng.index(object_ids.len())];
        if rng.next() % 2 == 0 {
            patches.push(ScenePatch::SetTransform {
                object,
                transform: Transform2D {
                    translation: Vec2::new(rng.scalar(), rng.scalar()),
                    rotation: rng.scalar(),
                    scale: Vec2::new(rng.scalar().abs() + 0.1, rng.scalar().abs() + 0.1),
                },
            });
        } else {
            patches.push(ScenePatch::SetStyle {
                object,
                style: Style {
                    opacity: ((rng.next() % 1001) as f32) / 1000.0,
                    stroke_width: ((rng.next() % 500) as f32) / 100.0,
                    ..Style::default()
                },
            });
        }
    }
    patches
}

#[test]
fn generated_property_transactions_match_sequential_application_and_rollback() {
    for seed in 1_u64..=48 {
        let mut base = SceneDefinition::new();
        let ids = (0..12)
            .map(|index| {
                if index % 2 == 0 {
                    base.add(GeometryRef::circle(index as f32 + 1.0))
                } else {
                    base.add(GeometryRef::rectangle(index as f32 + 1.0, 2.0))
                }
            })
            .collect::<Vec<_>>();
        let patches = generated_property_patches(seed, &ids);

        let mut sequential = base.clone();
        for patch in &patches {
            sequential.apply_patch(patch.clone()).unwrap();
        }

        let mut transactional = base.clone();
        transactional
            .apply_transaction(&MutationTransaction::from_mutations(patches.clone()))
            .unwrap();
        assert_eq!(
            transactional, sequential,
            "seed={seed}: property fast path diverged from sequential semantics"
        );

        for failure_position in [0, 1, patches.len() / 2, patches.len()] {
            let mut mutations = patches[..failure_position].to_vec();
            mutations.push(ScenePatch::SetStyle {
                object: ObjectId::new(u64::MAX - seed),
                style: Style::default(),
            });
            mutations.extend_from_slice(&patches[failure_position..]);

            let mut rejected = base.clone();
            let before = rejected.clone();
            assert!(
                rejected
                    .apply_transaction(&MutationTransaction::from_mutations(mutations))
                    .is_err(),
                "seed={seed}: invalid property transaction unexpectedly committed"
            );
            assert_eq!(
                rejected, before,
                "seed={seed}: failure_position={failure_position}: property rollback was partial"
            );
        }

        let mut structural = base.clone();
        let before = structural.clone();
        let transaction = MutationTransaction::from_mutations([
            ScenePatch::CreateObject(ObjectDefinition::new(
                ObjectId::new(10_000 + seed),
                GeometryRef::circle(1.0),
            )),
            ScenePatch::RemoveObject(ObjectId::new(u64::MAX - seed)),
        ]);
        assert!(structural.apply_transaction(&transaction).is_err());
        assert_eq!(
            structural, before,
            "seed={seed}: conservative structural transaction failed to roll back"
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

        for object_id in 0..40_u64 {
            live.push(store.insert_object(object(object_id)));
        }

        for step in 0..300 {
            if live.is_empty() {
                live.push(store.insert_object(object(10_000 + step)));
            }
            let index = rng.index(live.len());
            let id = live[index];
            if rng.next() % 5 == 0 {
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
                let removed = live.swap_remove(rng.index(live.len()));
                if let Some(SourceIdentity::ExplicitKey(key)) =
                    store.node(removed).unwrap().source_identity().cloned()
                {
                    expected_owner.remove(&key);
                }
                store.remove_node(removed).unwrap();
                assert!(store.node(removed).is_none(), "seed={seed} step={step}");
                let replacement = store.insert_object(object(20_000 + step));
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

from pathlib import Path


def r1(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one match, found {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


p = "crates/noon-compile/src/semantic_lowering/animation_schedule.rs"

# Published projection: separate family-transform lane with no ObjectId.
r1(p,
'''    leaves: Vec<SemanticScheduledAnimationLeaf>,
    scalar_leaves: Vec<SemanticScheduledScalarLeaf>,
}''',
'''    leaves: Vec<SemanticScheduledAnimationLeaf>,
    scalar_leaves: Vec<SemanticScheduledScalarLeaf>,
    family_transforms: Vec<SemanticScheduledFamilyTransform>,
}''')
r1(p,
'''    pub fn scalar_leaves(&self) -> &[SemanticScheduledScalarLeaf] {
        &self.scalar_leaves
    }

    pub fn len(&self) -> usize {
        self.leaves.len() + self.scalar_leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty() && self.scalar_leaves.is_empty()
    }''',
'''    pub fn scalar_leaves(&self) -> &[SemanticScheduledScalarLeaf] {
        &self.scalar_leaves
    }

    pub fn family_transforms(&self) -> &[SemanticScheduledFamilyTransform] {
        &self.family_transforms
    }

    pub fn len(&self) -> usize {
        self.leaves.len() + self.scalar_leaves.len() + self.family_transforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
            && self.scalar_leaves.is_empty()
            && self.family_transforms.is_empty()
    }''')
r1(p,
'''pub struct SemanticScheduledScalarLeaf {
    pub animation: SemanticNodeId,
    pub signal: SemanticNodeId,
    pub target: f64,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

/// Compiler scheduling result for an animation graph held by one prepared semantic transaction.''',
'''pub struct SemanticScheduledScalarLeaf {
    pub animation: SemanticNodeId,
    pub signal: SemanticNodeId,
    pub target: f64,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

/// One scheduled family Transform. It carries shared timing and authored family
/// references, but deliberately no stable execution object identity.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticScheduledFamilyTransform {
    pub finish_time_map: CompositionTimeMap,
    pub animation: SemanticNodeId,
    pub source: SemanticNodeId,
    pub target_state: SemanticNodeId,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

/// Compiler scheduling result for an animation graph held by one prepared semantic transaction.''')

# Prepared projection mirrors the same identity-free lane.
r1(p,
'''    leaves: Vec<PreparedSemanticScheduledAnimationLeaf>,
    scalar_leaves: Vec<PreparedSemanticScheduledScalarLeaf>,
}''',
'''    leaves: Vec<PreparedSemanticScheduledAnimationLeaf>,
    scalar_leaves: Vec<PreparedSemanticScheduledScalarLeaf>,
    family_transforms: Vec<PreparedSemanticScheduledFamilyTransform>,
}''')
r1(p,
'''    pub fn scalar_leaves(&self) -> &[PreparedSemanticScheduledScalarLeaf] {
        &self.scalar_leaves
    }

    pub fn len(&self) -> usize {
        self.leaves.len() + self.scalar_leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty() && self.scalar_leaves.is_empty()
    }''',
'''    pub fn scalar_leaves(&self) -> &[PreparedSemanticScheduledScalarLeaf] {
        &self.scalar_leaves
    }

    pub fn family_transforms(&self) -> &[PreparedSemanticScheduledFamilyTransform] {
        &self.family_transforms
    }

    pub fn len(&self) -> usize {
        self.leaves.len() + self.scalar_leaves.len() + self.family_transforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
            && self.scalar_leaves.is_empty()
            && self.family_transforms.is_empty()
    }''')
r1(p,
'''pub struct PreparedSemanticScheduledScalarLeaf {
    pub animation: SemanticTransactionNodeRef,
    pub signal: SemanticNodeId,
    pub target: f64,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticAnimationLookupError''',
'''pub struct PreparedSemanticScheduledScalarLeaf {
    pub animation: SemanticTransactionNodeRef,
    pub signal: SemanticNodeId,
    pub target: f64,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

/// Transaction-local scheduled family Transform with no execution object identity.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedSemanticScheduledFamilyTransform {
    pub finish_time_map: CompositionTimeMap,
    pub animation: SemanticTransactionNodeRef,
    pub source: SemanticTransactionNodeRef,
    pub target_state: SemanticTransactionNodeRef,
    pub timing: TrackTiming,
    pub time_map: CompositionTimeMap,
    pub options: ResolvedAnimationOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedSemanticAnimationLookupError''')

# Published lowering collects the third internal leaf kind separately.
r1(p,
'''    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    for leaf in projection.leaves {''',
'''    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    let mut family_transforms = Vec::new();
    for leaf in projection.leaves {''')
r1(p,
'''            ScheduledAnimationLeaf::Scalar {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            } => scalar_leaves.push(SemanticScheduledScalarLeaf {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            }),
        }
    }
    Ok(SemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves,
        scalar_leaves,
    })''',
'''            ScheduledAnimationLeaf::Scalar {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            } => scalar_leaves.push(SemanticScheduledScalarLeaf {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            }),
            ScheduledAnimationLeaf::FamilyTransform {
                finish_time_map,
                animation,
                source,
                target_state,
                timing,
                time_map,
                options,
            } => family_transforms.push(SemanticScheduledFamilyTransform {
                finish_time_map,
                animation,
                source,
                target_state,
                timing,
                time_map,
                options,
            }),
        }
    }
    Ok(SemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves,
        scalar_leaves,
        family_transforms,
    })''')

# Prepared lowering has the same local declarations; target the occurrence after its function header.
text = Path(p).read_text()
needle = '''pub fn lower_prepared_semantic_animation_schedule('''
pos = text.index(needle)
tail = text[pos:]
old = '''    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    for leaf in projection.leaves {'''
if tail.count(old) != 1:
    raise RuntimeError(f"prepared lowering declaration match count {tail.count(old)}")
tail = tail.replace(old, '''    let mut leaves = Vec::new();
    let mut scalar_leaves = Vec::new();
    let mut family_transforms = Vec::new();
    for leaf in projection.leaves {''', 1)
Path(p).write_text(text[:pos] + tail)
r1(p,
'''            ScheduledAnimationLeaf::Scalar {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            } => scalar_leaves.push(PreparedSemanticScheduledScalarLeaf {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            }),
        }
    }
    Ok(PreparedSemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves,
        scalar_leaves,
    })''',
'''            ScheduledAnimationLeaf::Scalar {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            } => scalar_leaves.push(PreparedSemanticScheduledScalarLeaf {
                animation,
                signal,
                target,
                timing,
                time_map,
                options,
            }),
            ScheduledAnimationLeaf::FamilyTransform {
                finish_time_map,
                animation,
                source,
                target_state,
                timing,
                time_map,
                options,
            } => family_transforms.push(PreparedSemanticScheduledFamilyTransform {
                finish_time_map,
                animation,
                source,
                target_state,
                timing,
                time_map,
                options,
            }),
        }
    }
    Ok(PreparedSemanticAnimationScheduleProjection {
        root,
        start_time: projection.start_time,
        run_time: projection.run_time,
        leaves,
        scalar_leaves,
        family_transforms,
    })''')

# Generic declaration and scheduler leaf/plan kinds.
r1(p,
'''enum AnimationDeclarationIntent<R> {
    TransformTo {''',
'''enum AnimationDeclarationIntent<R> {
    FamilyTransformTo {
        source: R,
        target_state: R,
    },
    TransformTo {''')
r1(p,
'''enum ScheduledAnimationLeaf<R> {
    Object {''',
'''enum ScheduledAnimationLeaf<R> {
    FamilyTransform {
        finish_time_map: CompositionTimeMap,
        animation: R,
        source: R,
        target_state: R,
        timing: TrackTiming,
        time_map: CompositionTimeMap,
        options: ResolvedAnimationOptions,
    },
    Object {''')
r1(p,
'''enum PlannedAnimationKind<R> {
    Wait,
    Leaf {''',
'''enum PlannedAnimationKind<R> {
    Wait,
    FamilyTransform {
        source: R,
        target_state: R,
        options: ResolvedAnimationOptions,
    },
    Leaf {''')

# Published lookup reads family endpoints as families, not object target states.
r1(p,
'''            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
                interpolation,
''',
'''            SemanticAnimationIntent::FamilyTransformTo {
                source,
                target_state,
            } => {
                self.store
                    .semantic_family_members_checked(*source)
                    .map_err(SemanticAnimationError::Target)?;
                self.store
                    .semantic_family_members_checked(*target_state)
                    .map_err(SemanticAnimationError::Target)?;
                AnimationDeclarationIntent::FamilyTransformTo {
                    source: *source,
                    target_state: *target_state,
                }
            }
            SemanticAnimationIntent::TransformTo {
                target,
                target_state,
                interpolation,
''')

# Prepared lookup: published-existing and pending transaction forms.
r1(p,
'''                    SemanticAnimationIntent::TransformTo {
                        target,
                        target_state,
                        interpolation,
''',
'''                    SemanticAnimationIntent::FamilyTransformTo {
                        source,
                        target_state,
                    } => AnimationDeclarationIntent::FamilyTransformTo {
                        source: (*source).into(),
                        target_state: (*target_state).into(),
                    },
                    SemanticAnimationIntent::TransformTo {
                        target,
                        target_state,
                        interpolation,
''')
r1(p,
'''                    SemanticTransactionAnimationIntent::TransformTo {
                        target,
                        target_state,
                        interpolation,
''',
'''                    SemanticTransactionAnimationIntent::FamilyTransformTo {
                        source,
                        target_state,
                    } => AnimationDeclarationIntent::FamilyTransformTo {
                        source: *source,
                        target_state: *target_state,
                    },
                    SemanticTransactionAnimationIntent::TransformTo {
                        target,
                        target_state,
                        interpolation,
''')
r1(p,
'''        match &intent {
            AnimationDeclarationIntent::TransformTo {''',
'''        match &intent {
            AnimationDeclarationIntent::FamilyTransformTo { .. } => {}
            AnimationDeclarationIntent::TransformTo {''')

# Shared planner resolves options but never requests execution_object_id for family Transform.
r1(p,
'''    match state.intent {
        AnimationDeclarationIntent::TransformTo {''',
'''    match state.intent {
        AnimationDeclarationIntent::FamilyTransformTo {
            source,
            target_state,
        } => {
            let options =
                resolve_animation_options(AnimationDefaults::MANIM, state.options, play_options)
                    .map_err(|error| AnimationSchedulePlanError::Options { animation, error })?;
            Ok(PlannedAnimation {
                animation,
                run_time: options.run_time,
                kind: PlannedAnimationKind::FamilyTransform {
                    source,
                    target_state,
                    options,
                },
            })
        }
        AnimationDeclarationIntent::TransformTo {''')

# Shared composition traversal emits timing/time-map into the identity-free lane.
r1(p,
'''    match &plan.kind {
        PlannedAnimationKind::Wait => {}
        PlannedAnimationKind::Leaf {''',
'''    match &plan.kind {
        PlannedAnimationKind::Wait => {}
        PlannedAnimationKind::FamilyTransform {
            source,
            target_state,
            options,
        } => leaves.push(ScheduledAnimationLeaf::FamilyTransform {
            finish_time_map: finish_time_map.clone(),
            animation: plan.animation,
            source: *source,
            target_state: *target_state,
            timing: TrackTiming::new(root_start_time, root_run_time, options.rate_func),
            time_map: CompositionTimeMap::from_steps(steps.clone()),
            options: *options,
        }),
        PlannedAnimationKind::Leaf {''')

# Focused integration tests exercise direct and nested timing without ObjectId assignment.
Path("crates/noon-compile/tests/family_transform_schedule.rs").write_text(r'''use noon_compile::{
    lower_prepared_semantic_animation_schedule, lower_semantic_animation_schedule,
    SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, RateFunction, SemanticMutationTransaction, SemanticObjectState,
    SemanticStore, StoredGeometry,
};

fn object(store: &mut SemanticStore, radius: f32) -> noon_core::SemanticNodeId {
    store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
        radius,
    }))
}

fn family(
    store: &mut SemanticStore,
    members: &[noon_core::SemanticNodeId],
) -> noon_core::SemanticNodeId {
    let family = store.insert_family();
    for &member in members {
        store.add_member(family, member).unwrap();
    }
    family
}

fn index(store: &SemanticStore) -> SemanticExecutionIndex {
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(store).unwrap();
    index
}

#[test]
fn published_family_transform_owns_timing_without_execution_object_identity() {
    let mut store = SemanticStore::new();
    let s0 = object(&mut store, 1.0);
    let s1 = object(&mut store, 0.7);
    let t0 = object(&mut store, 0.4);
    let t1 = object(&mut store, 0.5);
    let t2 = object(&mut store, 0.6);
    let source = family(&mut store, &[s0, s1]);
    let target = family(&mut store, &[t0, t1, t2]);
    store.attach_to_scene(source).unwrap();
    let animation = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new()
                .run_time(2.5)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let source_before = store.semantic_family_members_checked(source).unwrap().to_vec();
    let target_before = store.semantic_family_members_checked(target).unwrap().to_vec();
    let revision = store.scene_revision();

    let schedule = lower_semantic_animation_schedule(
        &store,
        &index(&store),
        animation,
        3.0,
        AnimationOptions::new(),
    )
    .unwrap();

    assert!(schedule.leaves().is_empty());
    assert!(schedule.scalar_leaves().is_empty());
    assert_eq!(schedule.family_transforms().len(), 1);
    assert_eq!(schedule.len(), 1);
    let family = &schedule.family_transforms()[0];
    assert_eq!(family.animation, animation);
    assert_eq!(family.source, source);
    assert_eq!(family.target_state, target);
    assert_eq!(family.timing.start_time, 3.0);
    assert_eq!(family.timing.duration, 2.5);
    assert_eq!(family.options.rate_func, RateFunction::Linear);
    assert_eq!(store.scene_revision(), revision);
    assert_eq!(store.semantic_family_members_checked(source).unwrap(), source_before);
    assert_eq!(store.semantic_family_members_checked(target).unwrap(), target_before);
}

#[test]
fn prepared_family_transform_uses_same_scheduler_without_object_id_lookup() {
    let mut store = SemanticStore::new();
    let source_leaf = object(&mut store, 1.0);
    let target_leaf = object(&mut store, 2.0);
    let source = family(&mut store, &[source_leaf]);
    let target = family(&mut store, &[target_leaf]);
    store.attach_to_scene(source).unwrap();
    let index = index(&store);

    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(1.25)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &index,
        animation,
        4.0,
        AnimationOptions::new(),
    )
    .unwrap();

    assert!(schedule.leaves().is_empty());
    assert!(schedule.scalar_leaves().is_empty());
    assert_eq!(schedule.family_transforms().len(), 1);
    let family = &schedule.family_transforms()[0];
    assert_eq!(family.source.existing(), Some(source));
    assert_eq!(family.target_state.existing(), Some(target));
    assert_eq!(family.timing.start_time, 4.0);
    assert_eq!(family.timing.duration, 1.25);
}

#[test]
fn family_transform_in_sequence_uses_existing_composition_time_map() {
    let mut store = SemanticStore::new();
    let s = object(&mut store, 1.0);
    let t = object(&mut store, 2.0);
    let source = family(&mut store, &[s]);
    let target = family(&mut store, &[t]);
    store.attach_to_scene(source).unwrap();

    let transform = store
        .insert_semantic_family_transform_animation(
            source,
            target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let wait = store
        .insert_semantic_wait_animation(AnimationOptions::new().run_time(1.0))
        .unwrap();
    let sequence = store
        .insert_semantic_animation_composition(
            noon_core::SemanticAnimationCompositionKind::Sequence,
            vec![transform, wait],
            AnimationOptions::new().rate_func(RateFunction::Linear),
        )
        .unwrap();

    let schedule = lower_semantic_animation_schedule(
        &store,
        &index(&store),
        sequence,
        5.0,
        AnimationOptions::new(),
    )
    .unwrap();
    assert_eq!(schedule.family_transforms().len(), 1);
    let family = &schedule.family_transforms()[0];
    assert!(!family.time_map.is_identity());
    assert_eq!(family.timing.start_time, 5.0);
    assert_eq!(family.timing.duration, schedule.run_time());
}
''')

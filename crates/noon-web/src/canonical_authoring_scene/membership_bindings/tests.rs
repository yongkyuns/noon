use super::*;
use noon_core::{AnimationOptions, RateFunction, SemanticAnimationCompositionKind, Vec2, VectorPath};

struct Fixture {
    context: CanonicalAuthoringScene,
    source: noon::MobjectFamily,
    source_leaf: noon::Mobject,
    target: noon::MobjectFamily,
    target_leaf: noon::Mobject,
}

fn path_object(context: &CanonicalAuthoringScene, x: f64) -> noon::Mobject {
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, -1.0))
        .line_to(Vec2::new(1.0, -0.5))
        .line_to(Vec2::new(-0.25, 1.0))
        .close();
    let mut object = noon::Mobject::from_manim_geometry(
        std::rc::Rc::clone(context.scene.integration_store()),
        noon::ManimGeometryOptions::path(path).unwrap(),
    )
    .unwrap();
    object.shift(x, 0.0).unwrap();
    object
}

fn fixture() -> Fixture {
    let mut context = CanonicalAuthoringScene::default();
    let source_leaf = path_object(&context, -2.0);
    let target_leaf = path_object(&context, 4.0);
    let source = context.scene.family(&[(&source_leaf).into()]).unwrap();
    let target = context.scene.family(&[(&target_leaf).into()]).unwrap();
    context
        .edit_membership(SceneMembershipBatch {
            kind: SceneMembershipBatchKind::Add,
            members: vec![OwnedSceneMembershipMember::Family(source.clone())],
            bindings: vec![(ObjectId::new(0), source_leaf.clone())],
        })
        .unwrap();
    Fixture { context, source, source_leaf, target, target_leaf }
}

fn associations(bindings: &[(u64, &noon::Mobject)]) -> SceneMembershipBatch {
    SceneMembershipBatch {
        kind: SceneMembershipBatchKind::Add,
        members: Vec::new(),
        bindings: bindings.iter().map(|(id, handle)| (ObjectId::new(*id), (*handle).clone())).collect(),
    }
}

fn begin_matching(f: &mut Fixture) -> f64 {
    f.context.begin_ordinary_mixed_composition(
        SemanticAnimationCompositionKind::Parallel,
        &[OrdinaryCompositionChild::MatchingFamilyTransformTo {
            source: f.source.clone(),
            target_state: f.target.clone(),
            options: AnimationOptions::new().run_time(1.0).rate_func(RateFunction::Linear),
        }],
        AnimationOptions::new(),
        AnimationOptions::new(),
    ).unwrap()
}

fn complete(f: &mut Fixture, end: f64) {
    let player = f.context.active_live_player().unwrap();
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
}

#[test]
fn matching_completion_associates_original_target_without_republishing_and_can_readd_source() {
    let mut f = fixture();
    let end = begin_matching(&mut f);
    let player = f.context.take_execution_player(end, 41).unwrap();
    let identity = player.ownership_identity();
    assert!(f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).is_err());
    let mut player = player;
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
    f.context.return_execution_player(player).unwrap();
    assert!(!f.context.contains_mobject(&f.source_leaf).unwrap());
    assert!(f.context.contains_mobject(&f.target_leaf).unwrap());
    assert!(f.context.mobject_layout(&f.target_leaf).is_err());
    let revision = f.context.scene.revision();
    let publication = f.context.active_live_player().unwrap().live_effective(&f.target_leaf).unwrap().publication;
    let roots = f.context.root_membership_keys().unwrap();

    f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).unwrap();
    assert_eq!(f.context.mobject_layout(&f.target_leaf).unwrap(), (4.0, 0.0, 2.0, 2.0));
    assert_eq!(f.context.identities.get(&f.target_leaf.node_id()), Some(&ObjectId::new(1)));
    assert_eq!(f.context.scene.revision(), revision);
    assert_eq!(f.context.root_membership_keys().unwrap(), roots);
    let player = f.context.active_live_player().unwrap();
    assert_eq!(player.ownership_identity(), identity);
    assert_eq!(player.live_effective(&f.target_leaf).unwrap().publication, publication);
    // Retrying the same association is idempotent; no second wrapper identity.
    f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).unwrap();

    f.context.ordinary_play_mixed_composition(
        SemanticAnimationCompositionKind::Parallel,
        &[OrdinaryCompositionChild::FamilyIndicate {
            target: f.target.clone(),
            indication: noon::IndicateOptions::default(),
            options: AnimationOptions::new().run_time(1.0).rate_func(RateFunction::ThereAndBack),
        }],
        AnimationOptions::new(), AnimationOptions::new(),
    ).unwrap();
    assert_eq!(f.context.mobject_layout(&f.target_leaf).unwrap(), (4.0, 0.0, 2.0, 2.0));
    f.context.edit_membership(SceneMembershipBatch {
        kind: SceneMembershipBatchKind::Add,
        members: vec![OwnedSceneMembershipMember::Family(f.source)],
        bindings: vec![(ObjectId::new(0), f.source_leaf.clone())],
    }).unwrap();
    assert!(f.context.contains_mobject(&f.source_leaf).unwrap());
    assert_eq!(f.context.identities.get(&f.source_leaf.node_id()), Some(&ObjectId::new(0)));
    assert_eq!(f.context.active_live_player().unwrap().ownership_identity(), identity);
}

#[test]
fn pending_and_rejected_completion_cannot_associate_a_target() {
    let mut f = fixture();
    assert!(f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).is_err());
    let end = begin_matching(&mut f);
    let revision = f.context.scene.revision();
    let bindings = f.context.bindings.clone();
    let identities = f.context.identities.clone();
    assert!(f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).is_err());
    let player = f.context.active_live_player().unwrap();
    player.live_advance_segment_to(end / 2.0).unwrap();
    assert!(player.live_complete_segment().is_err());
    assert!(f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).is_err());
    assert_eq!(f.context.scene.revision(), revision);
    assert_eq!(f.context.bindings, bindings);
    assert_eq!(f.context.identities, identities);
    complete(&mut f, end);
    f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).unwrap();
}

#[test]
fn invalid_association_batches_leave_all_bindings_unchanged() {
    let mut f = fixture();
    let end = begin_matching(&mut f);
    complete(&mut f, end);
    let foreign = CanonicalAuthoringScene::default();
    let foreign_leaf = path_object(&foreign, 1.0);
    let bindings = f.context.bindings.clone();
    let identities = f.context.identities.clone();
    let revision = f.context.scene.revision();
    // A valid first reservation cannot leak when a later reservation fails.
    for batch in [
        associations(&[(1, &f.target_leaf), (2, &f.source_leaf)]),
        associations(&[(1, &f.target_leaf), (2, &foreign_leaf)]),
        associations(&[(1, &f.target_leaf), (1, &f.target_leaf)]),
        associations(&[(0, &f.target_leaf)]),
    ] {
        assert!(f.context.associate_published_bindings(batch).is_err());
        assert_eq!(f.context.bindings, bindings);
        assert_eq!(f.context.identities, identities);
        assert_eq!(f.context.scene.revision(), revision);
    }
    let mut edit = associations(&[(1, &f.target_leaf)]);
    edit.members.push(OwnedSceneMembershipMember::Family(f.target.clone()));
    assert!(f.context.associate_published_bindings(edit).is_err());
    let mut remove = associations(&[(1, &f.target_leaf)]);
    remove.kind = SceneMembershipBatchKind::Remove;
    assert!(f.context.associate_published_bindings(remove).is_err());
    assert_eq!(f.context.bindings, bindings);
    assert_eq!(f.context.identities, identities);
}

#[test]
fn stale_returned_runtime_cannot_associate_authored_only_members() {
    let mut f = fixture();
    let end = begin_matching(&mut f);
    complete(&mut f, end);
    let player = f.context.take_execution_player(end, 41).unwrap();
    let identity = player.ownership_identity();
    f.context.return_execution_player(player).unwrap();
    f.target_leaf.shift(1.0, 0.0).unwrap();
    let revision = f.context.scene.revision();
    assert!(f.context.associate_published_bindings(associations(&[(1, &f.target_leaf)])).is_err());
    assert!(!f.context.identities.contains_key(&f.target_leaf.node_id()));
    assert_eq!(f.context.scene.revision(), revision);
    assert_eq!(f.context.active_live_player().unwrap().ownership_identity(), identity);
}

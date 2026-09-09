use noon::ManimNextToArgs;
use noon::{
    FamilyLayoutTarget as AuthoredTarget, LayoutAnchor, LiveLayoutTarget, Scene, SemanticVec3,
};

fn args() -> ManimNextToArgs {
    ManimNextToArgs {
        direction: (1.0, 0.0),
        buff: 0.25,
        aligned_edge: (0.0, 0.0),
        mask: (1.0, 0.0),
    }
}

#[test]
fn indexed_alignment_resolves_current_direct_members_and_moves_entire_family() {
    let scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(2.0, 3.0).unwrap();
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let mut near = scene.square(2.0).unwrap();
    near.shift(6.0, 0.0).unwrap();
    let mut far = scene.square(2.0).unwrap();
    far.shift(10.0, 0.0).unwrap();
    let nested = scene.family(&[(&near).into()]).unwrap();
    let target = scene.family(&[(&far).into(), (&nested).into()]).unwrap();
    let source = LayoutAnchor::from(&family);
    let aligner = source.clone().member(-1);
    let destination = LayoutAnchor::from(&target).member(-1);
    source
        .next_to_aligned(AuthoredTarget::Anchor(&destination), &aligner, args())
        .unwrap();
    assert_eq!(first.center().unwrap(), (5.75, 0.0));
    assert_eq!(second.center().unwrap(), (7.75, 3.0));
    target.remove((&far).into()).unwrap();
    target.add((&far).into()).unwrap();
    source
        .next_to_aligned(AuthoredTarget::Anchor(&destination), &aligner, args())
        .unwrap();
    assert_eq!(first.center().unwrap(), (9.75, 0.0));
    assert_eq!(second.center().unwrap(), (11.75, 3.0));
}

#[test]
fn invalid_alignment_does_not_publish_or_partially_move_members() {
    let scene = Scene::new();
    let object = scene.square(1.0).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    let source = LayoutAnchor::from(&family);
    let foreign_scene = Scene::new();
    let foreign = LayoutAnchor::from(&foreign_scene.square(1.0).unwrap());
    let before = scene.integration_store().borrow().scene_revision();
    for invalid in [
        source.clone().member(-2),
        source.clone().member(1),
        foreign,
        LayoutAnchor::from(&object).member(0),
    ] {
        assert!(source
            .next_to_aligned(AuthoredTarget::Point(9.0, 0.0), &invalid, args())
            .is_err());
        assert_eq!(scene.integration_store().borrow().scene_revision(), before);
        assert_eq!(object.center().unwrap(), (0.0, 0.0));
    }
}

#[test]
fn live_alignment_uses_effective_target_bounds_and_one_local_publication() {
    let mut scene = Scene::new();
    let first = scene.square(1.0).unwrap();
    let mut second = scene.square(1.0).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let source_family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let mut target = scene.square(2.0).unwrap();
    target.shift(6.0, 0.0).unwrap();
    let target_family = scene.family(&[(&target).into()]).unwrap();
    let tracker = scene.value_tracker(0.0).unwrap();
    let position = scene
        .position_from_tracker(
            &tracker,
            SemanticVec3::new(1.0, 0.0, 0.0),
            SemanticVec3::new(6.0, 0.0, 0.0),
        )
        .unwrap();
    scene.bind_position(&target, &position).unwrap();
    for object in [&first, &second, &target] {
        scene.add(object).unwrap();
    }
    let mut execution = scene.execution_session().unwrap();
    let mut live = scene.live(&mut execution);
    live.set_value(&tracker, 2.0).unwrap();
    let source = LayoutAnchor::from(&source_family);
    let aligner = source.clone().member(-1);
    let destination = LayoutAnchor::from(&target_family).member(0);
    let before = scene.integration_store().borrow().scene_revision();
    let result = live
        .next_layout_to_aligned(
            &source,
            LiveLayoutTarget::Anchor(&destination),
            &aligner,
            args(),
        )
        .unwrap();
    assert_eq!(result.impacts().len(), 2);
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        before.checked_next().unwrap()
    );
    assert_eq!(live.effective_layout(&first).unwrap().center, (7.75, 0.0));
    assert_eq!(live.effective_layout(&second).unwrap().center, (9.75, 0.0));
    assert_eq!(target.center().unwrap(), (6.0, 0.0));
    let before = scene.integration_store().borrow().scene_revision();
    assert!(live
        .next_layout_to_aligned(
            &source,
            LiveLayoutTarget::Anchor(&destination.clone().member(1)),
            &aligner,
            args()
        )
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert_eq!(live.effective_layout(&first).unwrap().center, (7.75, 0.0));
}

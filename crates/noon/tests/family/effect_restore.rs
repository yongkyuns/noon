//! Appearance restoration through the ordinary shared state-replacement transaction.
use noon::{
    effects::{Blur, EffectUpdate, Glow, Pixels, Shadow},
    Scene, SemanticNodeId,
};
use noon_core::{EffectDefinition, SemanticAnimationEffectSnapshot};

fn snapshot(scene: &Scene, node: SemanticNodeId) -> Vec<SemanticAnimationEffectSnapshot> {
    scene
        .integration_store()
        .borrow()
        .animation_effect_snapshot(node)
        .unwrap()
}

fn appearance(
    snapshot: &[SemanticAnimationEffectSnapshot],
) -> Vec<(noon_core::SemanticEffectName, EffectDefinition)> {
    snapshot
        .iter()
        .map(|effect| (effect.name.clone(), effect.definition))
        .collect()
}

#[test]
fn object_restore_recovers_order_with_fresh_bindings_and_preserves_the_saved_copy() {
    let mut scene = Scene::new();
    let mut source = scene.square(0.8).unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "first",
        )
        .unwrap();
    source
        .add_effect(
            Glow::new(EffectUpdate::default().intensity(0.25)).unwrap(),
            "glow",
        )
        .unwrap();
    source
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(1.0))).unwrap(),
            "last",
        )
        .unwrap();
    let saved = source.target_editor().unwrap();
    let saved_bindings = snapshot(&scene, saved.node_id());
    let original = snapshot(&scene, source.node_id());
    source
        .set_effect("first", EffectUpdate::default().radius(Pixels(7.0)))
        .unwrap();
    source.remove_effect("glow").unwrap();
    source.add_effect(Shadow::default(), "new-shadow").unwrap();
    source.shift(2.0, 1.0).unwrap();
    let retired = snapshot(&scene, source.node_id());
    let owner = source.node_id();
    let revision = scene.revision();

    source.become_handle(&saved, Default::default()).unwrap();

    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(source.node_id(), owner);
    assert_eq!(
        source.state().unwrap().transform,
        saved.state().unwrap().transform
    );
    let restored = snapshot(&scene, owner);
    assert_eq!(appearance(&restored), appearance(&saved_bindings));
    assert_eq!(snapshot(&scene, saved.node_id()), saved_bindings);
    for old in original.iter().chain(&retired) {
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_effect_state(old.attachment)
            .is_err());
        assert!(!restored
            .iter()
            .any(|effect| effect.attachment == old.attachment));
    }
    for effect in &restored {
        assert!(!saved_bindings
            .iter()
            .any(|saved| saved.attachment == effect.attachment));
    }
    let same = source.clone();
    let revision = scene.revision();
    source.become_handle(&same, Default::default()).unwrap();
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        snapshot(&scene, owner),
        restored,
        "self replacement preserves every generation"
    );
}

#[test]
fn unequal_nested_family_become_copies_composed_and_leaf_appearance_without_importing_ids() {
    let mut scene = Scene::new();
    let mut old_leaf = scene.circle(0.3).unwrap();
    old_leaf
        .set_glow(EffectUpdate::default().intensity(0.1))
        .unwrap();
    let mut source = scene.family(&[(&old_leaf).into()]).unwrap();
    source.add_effect(Blur::default(), "old").unwrap();
    let mut a = scene.square(0.5).unwrap();
    a.add_effect(Shadow::default(), "depth").unwrap();
    let b = scene.circle(0.2).unwrap();
    let c = scene.square(0.3).unwrap();
    let alias = scene.family(&[(&a).into()]).unwrap();
    let mut inner = scene
        .family(&[(&a).into(), (&b).into(), (&alias).into()])
        .unwrap();
    inner.add_effect(Blur::default(), "soften").unwrap();
    let mut target = scene.family(&[(&inner).into(), (&c).into()]).unwrap();
    target.add_effect(Glow::default(), "glow").unwrap();
    target.set_object_opacity(0.625).unwrap();
    let target_nodes = scene
        .integration_store()
        .borrow()
        .ordered_authoring_nodes(target.node_id())
        .unwrap();
    let target_bindings: Vec<_> = target_nodes
        .iter()
        .map(|&node| snapshot(&scene, node))
        .collect();
    let retired_root = snapshot(&scene, source.node_id());
    let root = source.node_id();

    source.become_family(&target, Default::default()).unwrap();

    assert_eq!(source.node_id(), root);
    assert_eq!(source.own_style().unwrap().object_opacity, 0.625);
    let receiver_nodes = scene
        .integration_store()
        .borrow()
        .ordered_authoring_nodes(root)
        .unwrap();
    assert_eq!(receiver_nodes.len(), target_nodes.len());
    assert!(receiver_nodes
        .iter()
        .all(|node| !target_nodes.contains(node)));
    for ((&receiver, &target_node), expected) in receiver_nodes
        .iter()
        .zip(&target_nodes)
        .zip(&target_bindings)
    {
        let actual = snapshot(&scene, receiver);
        assert_eq!(appearance(&actual), appearance(expected));
        assert_eq!(snapshot(&scene, target_node), *expected);
        assert!(actual.iter().all(|effect| !expected
            .iter()
            .any(|old| old.attachment == effect.attachment)));
    }
    let inner = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(root)
        .unwrap()[0];
    let members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(inner)
        .unwrap();
    let alias_members = scene
        .integration_store()
        .borrow()
        .semantic_family_members_checked(members[2])
        .unwrap();
    assert_eq!(
        members[0], alias_members[0],
        "target aliases converge on one receiver-owned node"
    );
    for effect in retired_root {
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_effect_state(effect.attachment)
            .is_err());
    }
    let old_receiver: Vec<_> = receiver_nodes
        .iter()
        .flat_map(|&node| snapshot(&scene, node))
        .collect();
    source.become_family(&target, Default::default()).unwrap();
    for effect in old_receiver {
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_effect_state(effect.attachment)
            .is_err());
    }
}

mod live;

#[test]
fn replacement_preserves_the_private_visibility_role_beside_a_named_mask() {
    use noon::effects::NoiseDissolve;
    let mut scene = Scene::new();
    let mut source = scene.square(1.0).unwrap();
    source
        .add_effect(
            NoiseDissolve::new(7, 0.2)
                .unwrap()
                .with_progress(0.25)
                .unwrap(),
            "visibility",
        )
        .unwrap();
    let mut visibility = noon_core::SemanticMutationTransaction::new();
    let token = visibility.create_visibility_effect(
        source.node_id(),
        NoiseDissolve::new(11, 0.4)
            .unwrap()
            .with_progress(0.75)
            .unwrap(),
    );
    let old = visibility
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap()
        .resolve(token)
        .unwrap();
    let saved = source.target_editor().unwrap();
    let expected = snapshot(&scene, saved.node_id());
    let mut retire = noon_core::SemanticMutationTransaction::new();
    retire.remove_node(old);
    retire
        .apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    source.set_glow(EffectUpdate::default()).unwrap();
    source.become_handle(&saved, Default::default()).unwrap();
    let restored = snapshot(&scene, source.node_id());
    assert_eq!(appearance(&restored), appearance(&expected));
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].name.authored(), Some("visibility"));
    assert!(restored[1].name.is_visibility());
    assert_ne!(restored[1].attachment, old);
    scene.add(&source).unwrap();
    let session = scene.execution_session().unwrap();
    let owner = session.execution_object_id(source.node_id()).unwrap();
    let row = session
        .frame()
        .objects
        .iter()
        .find(|row| row.id == owner)
        .unwrap();
    assert_eq!(
        row.effects
            .as_ref()
            .unwrap()
            .visibility()
            .unwrap()
            .attachment,
        restored[1].attachment
    );
    assert_eq!(
        row.effects
            .as_ref()
            .unwrap()
            .visibility()
            .unwrap()
            .definition,
        expected[1].definition
    );
}

#[test]
fn overlapping_family_targets_follow_ordered_reads_unless_matching_captures_first() {
    for matching in [false, true] {
        for effect_on_first in [false, true] {
            let mut scene = Scene::new();
            let mut a = scene.square(0.5).unwrap();
            let mut b = scene.circle(0.3).unwrap();
            if effect_on_first {
                a.add_effect(Glow::default(), "halo").unwrap();
            } else {
                b.add_effect(Glow::default(), "halo").unwrap();
            }
            let source = scene.family(&[(&a).into(), (&b).into()]).unwrap();
            let target = scene.family(&[(&b).into(), (&a).into()]).unwrap();
            let before_a = snapshot(&scene, a.node_id());
            let before_b = snapshot(&scene, b.node_id());
            source
                .become_family(
                    &target,
                    noon::ManimBecomeOptions {
                        match_center: matching,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert_eq!(
                appearance(&snapshot(&scene, a.node_id())),
                appearance(&before_b)
            );
            assert_eq!(
                appearance(&snapshot(&scene, b.node_id())),
                appearance(if matching { &before_a } else { &before_b })
            );
            for old in before_a.iter().chain(&before_b) {
                assert!(scene
                    .integration_store()
                    .borrow()
                    .semantic_effect_state(old.attachment)
                    .is_err());
            }
        }
    }
}

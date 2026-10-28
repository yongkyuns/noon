use noon::{effects::EffectUpdate, ManimBecomeOptions, Scene};

fn assert_same_visual(actual: &noon::SemanticObjectState, target: &noon::SemanticObjectState) {
    assert_eq!(actual.content, target.content);
    assert_eq!(actual.transform, target.transform);
    assert_eq!(actual.style, target.style);
}

#[test]
fn unrelated_glow_does_not_block_plain_state_replacement() {
    let mut scene = Scene::new();
    let mut remote = scene.circle(0.2).unwrap();
    remote
        .set_glow(EffectUpdate::default().intensity(0.8))
        .unwrap();
    let generation = remote.get_effect("glow").unwrap().node_id();
    let mut receiver = scene.square(1.0).unwrap();
    let mut donor = scene.rectangle(2.0, 1.0).unwrap();
    donor.shift(2.0, 0.5).unwrap();

    let receiver_id = receiver.node_id();
    receiver
        .become_handle(&donor, ManimBecomeOptions::default())
        .unwrap();
    assert_eq!(receiver.node_id(), receiver_id);
    assert_same_visual(&receiver.state().unwrap(), &donor.state().unwrap());
    assert_eq!(remote.get_effect("glow").unwrap().node_id(), generation);
}

#[test]
fn local_effects_copy_independently_and_retire_when_the_donor_has_none() {
    let mut scene = Scene::new();
    let mut receiver = scene.square(1.0).unwrap();
    let mut donor = scene.circle(0.25).unwrap();
    donor
        .set_glow(EffectUpdate::default().intensity(0.8))
        .unwrap();
    let donor_effect = donor.get_effect("glow").unwrap();
    let receiver_id = receiver.node_id();
    receiver
        .become_handle(&donor, ManimBecomeOptions::default())
        .unwrap();
    assert_eq!(receiver.node_id(), receiver_id);
    assert_same_visual(&receiver.state().unwrap(), &donor.state().unwrap());
    let copied = receiver.get_effect("glow").unwrap();
    assert_ne!(copied.node_id(), donor_effect.node_id());
    assert_eq!(
        copied.authored_definition().unwrap(),
        donor_effect.authored_definition().unwrap()
    );

    donor.remove_glow().unwrap();
    assert!(donor_effect.authored_definition().is_err());
    assert!(
        copied.authored_definition().is_ok(),
        "donor retirement must not retire the independent receiver binding"
    );
    receiver
        .become_handle(&donor, ManimBecomeOptions::default())
        .unwrap();
    assert!(receiver.get_effect("glow").is_err());
    assert!(copied.authored_definition().is_err());

    receiver.set_glow(EffectUpdate::default()).unwrap();
    let prior = receiver.get_effect("glow").unwrap();
    let before_state = receiver.state().unwrap();
    let before_revision = scene.revision();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.square(2.0).unwrap();
    assert!(receiver
        .become_handle(&foreign, ManimBecomeOptions::default())
        .is_err());
    assert_eq!(receiver.state().unwrap(), before_state);
    assert_eq!(scene.revision(), before_revision);
    assert_eq!(
        receiver.get_effect("glow").unwrap().node_id(),
        prior.node_id()
    );
    assert!(prior.authored_definition().is_ok());
}

#[test]
fn family_replacement_copies_local_glow_without_touching_unrelated_bindings() {
    let mut scene = Scene::new();
    let mut remote = scene.circle(0.4).unwrap();
    remote
        .set_glow(EffectUpdate::default().intensity(1.2))
        .unwrap();
    let generation = remote.get_effect("glow").unwrap().node_id();

    let mut first = scene.square(0.8).unwrap();
    first.shift(-2.0, 0.0).unwrap();
    let mut second = scene.rectangle(1.0, 0.5).unwrap();
    second.shift(2.0, 0.0).unwrap();
    let source = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let mut target_a = scene.circle(0.5).unwrap();
    target_a.shift(-1.0, 1.0).unwrap();
    let mut target_b = scene.circle(0.8).unwrap();
    target_b.shift(1.0, 1.0).unwrap();
    let target = scene
        .family(&[(&target_a).into(), (&target_b).into()])
        .unwrap();

    source.become_family(&target, Default::default()).unwrap();
    assert_same_visual(&first.state().unwrap(), &target_a.state().unwrap());
    assert_same_visual(&second.state().unwrap(), &target_b.state().unwrap());
    assert_eq!(remote.get_effect("glow").unwrap().node_id(), generation);

    target_a.set_glow(EffectUpdate::default()).unwrap();
    let target_effect = target_a.get_effect("glow").unwrap();
    let revision = scene.revision();
    source.become_family(&target, Default::default()).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_same_visual(&first.state().unwrap(), &target_a.state().unwrap());
    assert_same_visual(&second.state().unwrap(), &target_b.state().unwrap());
    let copied = first.get_effect("glow").unwrap();
    assert_ne!(copied.node_id(), target_effect.node_id());
    assert_eq!(
        copied.authored_definition().unwrap(),
        target_effect.authored_definition().unwrap()
    );
    assert_eq!(remote.get_effect("glow").unwrap().node_id(), generation);

    let before = [first.state().unwrap(), second.state().unwrap()];
    let revision = scene.revision();
    let mut foreign_scene = Scene::new();
    let foreign = foreign_scene.family(&[]).unwrap();
    assert!(source.become_family(&foreign, Default::default()).is_err());
    assert_eq!([first.state().unwrap(), second.state().unwrap()], before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(
        first.get_effect("glow").unwrap().node_id(),
        copied.node_id()
    );
    assert_eq!(remote.get_effect("glow").unwrap().node_id(), generation);
}

#[test]
fn live_become_ignores_glow_on_unrelated_detached_source() {
    let mut scene = Scene::new();
    let mut remote = scene.circle(0.4).unwrap();
    remote.set_glow(EffectUpdate::default()).unwrap();
    let source = scene.rectangle(1.0, 1.0).unwrap();
    let donor = scene.square(2.0).unwrap();
    scene.add(&source).unwrap();

    let mut session = scene.execution_session().unwrap();
    scene
        .live(&mut session)
        .become_mobject(&source, &donor, ManimBecomeOptions::default())
        .unwrap();
    assert_same_visual(&source.state().unwrap(), &donor.state().unwrap());
    assert!(remote.get_effect("glow").is_ok());
}

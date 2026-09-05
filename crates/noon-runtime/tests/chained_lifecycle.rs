use noon_compile::CompiledScene;
use noon_core::{Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming};
use noon_runtime::SceneInstance;

fn chained_scene() -> CompiledScene {
    let mut scene = SceneDefinition::new();
    let first = scene.add(GeometryRef::circle(1.0));
    let second = scene.add(GeometryRef::circle(2.0));
    let third = scene.add(GeometryRef::circle(3.0));

    let first_snapshot = ObjectSnapshot::from(scene.object(first).expect("first exists"));
    let second_snapshot = ObjectSnapshot::from(scene.object(second).expect("second exists"));
    let third_snapshot = ObjectSnapshot::from(scene.object(third).expect("third exists"));

    scene
        .animate_transform(
            first,
            first_snapshot,
            second_snapshot.clone(),
            TrackTiming::new(0.0, 1.0, Easing::Linear),
        )
        .expect("first replacement transform is valid");
    scene
        .set_presence_at(first, true, false, 1.0)
        .expect("first hide is valid");
    scene
        .set_presence_at(second, false, true, 1.0)
        .expect("second show is valid");

    scene
        .animate_transform(
            second,
            second_snapshot,
            third_snapshot,
            TrackTiming::new(1.0, 1.0, Easing::Linear),
        )
        .expect("second replacement transform is valid");
    scene
        .set_presence_at(second, true, false, 2.0)
        .expect("second hide is valid");
    scene
        .set_presence_at(third, false, true, 2.0)
        .expect("third show is valid");

    CompiledScene::compile(&scene).expect("chained lifecycle scene must compile")
}

#[test]
fn chained_replacements_have_exact_presence_handoffs() {
    let mut instance = SceneInstance::new(chained_scene());

    let before_first = instance.seek(0.5).expect("valid time");
    assert!(before_first.is_present(0));
    assert!(!before_first.is_present(1));
    assert!(!before_first.is_present(2));

    let first_handoff = instance.seek(1.0).expect("valid time");
    assert!(!first_handoff.is_present(0));
    assert!(first_handoff.is_present(1));
    assert!(!first_handoff.is_present(2));
    assert_eq!(
        first_handoff.objects[1].geometry(),
        Some(&GeometryRef::circle(2.0))
    );

    let middle = instance.seek(1.5).expect("valid time");
    assert!(!middle.is_present(0));
    assert!(middle.is_present(1));
    assert!(!middle.is_present(2));
    assert_eq!(
        middle.objects[1].geometry(),
        Some(&GeometryRef::circle(2.5))
    );

    let second_handoff = instance.seek(2.0).expect("valid time");
    assert!(!second_handoff.is_present(0));
    assert!(!second_handoff.is_present(1));
    assert!(second_handoff.is_present(2));
    assert_eq!(
        second_handoff.objects[2].geometry(),
        Some(&GeometryRef::circle(3.0))
    );
}

#[test]
fn chained_replacements_direct_seek_matches_forward_and_rewind() {
    let compiled = chained_scene();
    let mut sequential = SceneInstance::new(compiled.clone());
    let mut direct = SceneInstance::new(compiled);

    for time in [0.25, 0.5, 1.0, 1.5, 2.0] {
        sequential.advance_to(time).expect("valid forward time");
    }
    direct.seek(2.0).expect("valid direct seek");
    assert_eq!(sequential.frame(), direct.frame());

    direct.seek(1.0).expect("valid rewind");
    assert!(!direct.frame().is_present(0));
    assert!(direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));

    direct.seek(0.5).expect("valid rewind before first handoff");
    assert!(direct.frame().is_present(0));
    assert!(!direct.frame().is_present(1));
    assert!(!direct.frame().is_present(2));
}

use noon::{Scene, SectionType};

#[test]
fn sections_follow_the_scene_clock_and_preserve_same_time_declarations() {
    let mut scene = Scene::new();
    scene.next_section("intro", SectionType::Normal).unwrap();
    scene.next_section("alternate", SectionType::Skip).unwrap();
    scene.wait(1.25).unwrap();
    scene.next_section("body", SectionType::Normal).unwrap();

    assert_eq!(scene.sections().len(), 3);
    assert_eq!(scene.sections()[0].name, "intro");
    assert_eq!(scene.sections()[0].time, 0.0);
    assert_eq!(scene.sections()[1].name, "alternate");
    assert_eq!(scene.sections()[1].section_type, SectionType::Skip);
    assert_eq!(scene.sections()[1].time, 0.0);
    assert_eq!(scene.sections()[2].time, 1.25);
    assert_eq!(scene.time(), 1.25);
}

#[test]
fn invalid_section_name_is_atomic_and_does_not_advance_time() {
    let mut scene = Scene::new();
    scene.wait(0.5).unwrap();
    let revision = scene.revision();
    assert!(scene
        .next_section("bad\nname", SectionType::Normal)
        .is_err());
    assert!(scene.sections().is_empty());
    assert_eq!(scene.time(), 0.5);
    assert_eq!(scene.revision(), revision);
}

#[test]
fn section_declarations_do_not_mutate_semantic_or_execution_state() {
    let mut scene = Scene::new();
    let square = scene.square(1.0).unwrap();
    scene.add(&square).unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let revision = scene.revision();
    let context = session.publication_context();
    let frame = format!("{:?}", session.frame());

    scene.next_section("live", SectionType::Normal).unwrap();

    assert_eq!(scene.revision(), revision);
    assert_eq!(session.publication_context(), context);
    assert_eq!(format!("{:?}", session.frame()), frame);
    assert!(session.take_frame_changes().is_empty());
}

#[test]
fn invalid_section_at_future_time_does_not_advance_cursor() {
    let mut scene = Scene::new();
    scene.wait(0.5).unwrap();
    let revision = scene.revision();
    assert!(scene
        .next_section_at("bad\nname", SectionType::Normal, 2.0)
        .is_err());
    assert!(scene.sections().is_empty());
    assert_eq!(scene.time(), 0.5);
    assert_eq!(scene.revision(), revision);
}

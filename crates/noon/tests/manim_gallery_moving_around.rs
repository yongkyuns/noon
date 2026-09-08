use noon::{example_scenes::moving_around, Color, LiveProgramStatus, RustHostCallbackTable, Vec2};

#[test]
fn moving_around_captures_each_completed_target_and_keeps_identity() {
    let mut program = moving_around::program().unwrap();
    let mut callbacks = RustHostCallbackTable::new();
    let mut identity = None;
    for end in 1..=4 {
        assert!(matches!(
            program.resume().unwrap(),
            LiveProgramStatus::Awaiting(_)
        ));
        let state = program.drive_to(&mut callbacks, f64::from(end)).unwrap();
        let frame = program.session().frame();
        assert_eq!(frame.objects.len(), 1);
        let object = &frame.objects[0];
        assert_eq!(*identity.get_or_insert(object.id), object.id);
        assert_eq!(object.transform.translation, Vec2::new(-1.0, 0.0));
        if end >= 2 {
            assert_eq!(object.style.fill, Some(Color::ORANGE));
        }
        if end >= 3 {
            assert_eq!(object.transform.scale, Vec2::new(0.3, 0.3));
        }
        if end == 4 {
            assert!((object.transform.rotation - 0.4).abs() < 1e-6);
        }
        if let LiveProgramStatus::PublicationPending(expected) = state {
            let publication = program.take_renderer_publication().context();
            assert_eq!(publication, expected);
            program.admit_publication(publication).unwrap();
        }
    }
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

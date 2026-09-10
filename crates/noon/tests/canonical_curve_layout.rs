#[test]
fn paired_canonical_curve_layout_keeps_typed_geometry_and_seek_stable() {
    let mut session = noon::example_scenes::canonical_curve_layout::session().unwrap();
    assert_eq!(session.frame().objects.len(), 4);
    session.seek(0.).unwrap();
    let before = session.frame().objects.clone();
    session.seek(0.2).unwrap();
    assert_eq!(session.frame().objects, before);
}

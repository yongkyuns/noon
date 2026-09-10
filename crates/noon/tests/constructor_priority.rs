use noon::{ManimGeometryOptions, Scene};

#[test]
fn inert_geometry_priority_is_finite_high_precision_and_published_once() {
    let mut scene = Scene::new();
    let mut options = ManimGeometryOptions::square(1.).unwrap();
    let revision = scene.revision();
    let z = f64::from(f32::MAX) * 2.;
    options.set_z_index(z).unwrap();
    assert!(options.set_z_index(f64::NAN).is_err());
    assert_eq!(scene.revision(), revision);
    let object = scene.geometry(options.clone()).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(object.z_index().unwrap(), z);
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.revision();
    let live_object = scene
        .live(&mut session)
        .create_manim_geometry(options)
        .unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(live_object.z_index().unwrap(), z);
    assert_eq!(session.frame().objects.len(), 1);
}

#[test]
fn family_constructor_priority_is_root_only_and_failure_does_not_allocate() {
    let mut scene = Scene::new();
    let a = scene.square(1.).unwrap();
    a.set_z_index(2.5).unwrap();
    let revision = scene.revision();
    let family = scene.family_with_z_index(&[(&a).into()], -3.25).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(family.z_index().unwrap(), -3.25);
    assert_eq!(a.z_index().unwrap(), 2.5);
    let revision = scene.revision();
    let count = scene.integration_store().borrow().len();
    assert!(scene.family_with_z_index(&[(&a).into()], f64::NAN).is_err());
    assert_eq!(scene.integration_store().borrow().len(), count);
    assert_eq!(scene.revision(), revision);
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.revision();
    assert!(scene
        .live(&mut session)
        .family_with_z_index(&[(&a).into()], f64::INFINITY)
        .is_err());
    assert_eq!(scene.revision(), revision);
    let empty = scene
        .live(&mut session)
        .family_with_z_index(&[], 7.5)
        .unwrap();
    assert_eq!(empty.z_index().unwrap(), 7.5);
    assert_eq!(a.z_index().unwrap(), 2.5);
}

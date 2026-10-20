use noon::{ManimGeometryOptions, Scene, SemanticSheenDirection};
use noon_core::{SemanticVec3, VectorPath};

#[test]
fn sheen_direction_constructor_metadata_survives_detached_and_live_publication() {
    let expected = SemanticSheenDirection::new(SemanticVec3::new(0.25, -0.5, 0.0), 2);
    let mut options = ManimGeometryOptions::path(VectorPath::new()).unwrap();
    options.set_sheen_direction(0.25, -0.5, 0.0, 2).unwrap();

    let mut scene = Scene::new();
    let detached = scene.geometry(options.clone()).unwrap();
    assert_eq!(detached.sheen_direction().unwrap(), expected);

    let mut session = scene.execution_session().unwrap();
    let live = scene
        .live(&mut session)
        .create_manim_geometry(options)
        .unwrap();
    assert_eq!(live.sheen_direction().unwrap(), expected);
}

#[test]
fn invalid_sheen_direction_constructor_metadata_is_rejected_without_changing_default() {
    let mut options = ManimGeometryOptions::path(VectorPath::new()).unwrap();
    assert!(options.set_sheen_direction(0.25, -0.5, 1.0, 2).is_err());
    assert!(options.set_sheen_direction(0.25, -0.5, 0.0, 4).is_err());
    let mut scene = Scene::new();
    assert_eq!(
        scene.geometry(options).unwrap().sheen_direction().unwrap(),
        SemanticSheenDirection::DEFAULT
    );

    let mut analytic = ManimGeometryOptions::circle(1.0).unwrap();
    assert!(analytic.set_sheen_direction(f64::NAN, 0.0, 0.0, 3).is_err());
    assert_eq!(
        analytic.inline_state().unwrap().unwrap().sheen_direction,
        SemanticSheenDirection::DEFAULT
    );
}

use super::*;
use crate::Scene;

const PREFIX: [SemanticVec3; 4] = [
    SemanticVec3::new(0.125, 0.25, 0.123456789012345),
    SemanticVec3::new(0.5, 0.75, 1.25),
    SemanticVec3::new(1.0, 1.5, 2.25),
    SemanticVec3::new(2.0, 3.0, 4.25),
];
const SUFFIX: [SemanticVec3; 2] = [
    SemanticVec3::new(4.0, 5.0, 6.125),
    SemanticVec3::new(7.0, 8.0, 9.25),
];

#[test]
fn raw_xyz_appends_preserve_world_prefix_and_identity_in_all_execution_modes() {
    for mode in 0..2 {
        let mut scene = Scene::new();
        let mut object = scene
            .path(
                VectorPath::from_manim_points_3d(PREFIX.to_vec()),
                Default::default(),
            )
            .unwrap();
        scene.add(&object).unwrap();
        // Exercise Z affine baking as well as the existing world XY path edit.
        let mut transform = object.state().unwrap().transform;
        transform.translation = SemanticVec3::new(1.5, -2.5, 3.0);
        transform.scale.z = 2.0;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(object.node_id(), transform);
        transaction
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let before = object.path_query().unwrap();
        let mut expected = before.points_3d().unwrap();
        assert_eq!(expected[3], SemanticVec3::new(3.5, 0.5, 11.5));
        expected.extend(SUFFIX);
        let id = object.node_id();
        let nodes = scene.integration_store().borrow().len();
        match mode {
            0 => object.append_points_3d(&SUFFIX).unwrap(),
            _ => {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
                scene.append_points_3d(&object, &SUFFIX).unwrap();
            }
        }
        let actual = if let Some(execution) = scene.running_execution() {
            crate::path_queries::effective_path_query(scene.integration_store(), execution, &object)
                .unwrap()
        } else {
            object.path_query().unwrap()
        };
        assert_eq!(actual.points_3d().unwrap(), expected, "mode {mode}");
        assert_eq!(object.node_id(), id);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert_eq!(object.state().unwrap().transform.translation.z, 0.0);
        assert_eq!(object.state().unwrap().transform.scale.z, 1.0);
        assert_eq!(before.points_3d().unwrap().len(), 4);
    }
}

#[test]
fn planar_raw_convenience_preserves_an_existing_three_axis_prefix() {
    let mut scene = Scene::new();
    let mut object = scene
        .path(
            VectorPath::from_manim_points_3d(PREFIX.to_vec()),
            Default::default(),
        )
        .unwrap();
    object.append_points_f64(&[(1.25, -0.5)]).unwrap();
    let actual = object.path_query().unwrap().points_3d().unwrap();
    assert_eq!(&actual[..4], &PREFIX);
    assert_eq!(actual[4], SemanticVec3::new(1.25, -0.5, 0.0));
}

#[test]
fn invalid_xyz_inputs_do_not_publish_or_admit_resources() {
    let mut scene = Scene::new();
    let object = scene
        .path(
            VectorPath::from_manim_points_3d(PREFIX.to_vec()),
            Default::default(),
        )
        .unwrap();
    scene.add(&object).unwrap();
    let mut running = scene.into_execution().unwrap();
    let context = running.session().publication_context();
    let revision = running.scene().revision();
    let resources = running
        .scene()
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    for points in [
        &[SemanticVec3::new(1.0, 2.0, f64::NAN)][..],
        &[SemanticVec3::new(1.0, 2.0, f64::INFINITY)][..],
    ] {
        for replacing in [false, true] {
            let result = if replacing {
                running.scene_mut().set_points_3d(&object, points)
            } else {
                running.scene_mut().append_points_3d(&object, points)
            };
            assert!(result.is_err());
        }
    }
    assert_eq!(object.path_query().unwrap().points_3d().unwrap(), PREFIX);
    assert_eq!(running.session().publication_context(), context);
    assert_eq!(running.scene().revision(), revision);
    assert_eq!(
        running
            .scene()
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

#[test]
fn appending_raw_coordinates_keeps_the_existing_morph_target() {
    let target = VectorPath::from_manim_points_3d(SUFFIX.to_vec());
    let path = VectorPath::from_manim_points_3d(PREFIX.to_vec()).with_morph_target(target.clone());
    let appended = append(&SUFFIX, || Ok(path.clone())).unwrap().unwrap();
    assert_eq!(appended.morph_target(), Some(&target));
    assert_eq!(&appended.manim_points().unwrap()[..4], &PREFIX);
}

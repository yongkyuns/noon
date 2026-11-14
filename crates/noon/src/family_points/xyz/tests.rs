use super::*;
use crate::{Color, Scene};

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
fn root_xyz_appends_preserve_children_identity_and_presentation_in_all_hosts() {
    for mode in 0..2 {
        for materialized in [false, true] {
            for attached in [false, true] {
                for drawing in [false, true] {
                    let mut scene = Scene::new();
                    let child = scene.square(0.5).unwrap();
                    let family = scene
                        .family_with_presentation(&[(&child).into()], 2.0, Color::GREEN, drawing)
                        .unwrap();
                    if materialized {
                        family.set_points_3d(&PREFIX).unwrap();
                    }
                    family.shift(1.25, -0.75).unwrap();
                    if attached {
                        scene.add_many(&[(&family).into()]).unwrap();
                    }
                    let id = family.node_id();
                    let count = scene.integration_store().borrow().len();
                    let before_child = child.state().unwrap();
                    let before = family.own_path_query().unwrap();
                    let mut expected = before.points_3d().unwrap();
                    expected.extend(SUFFIX);
                    let observed = if mode == 0 {
                        family.append_points_3d(&SUFFIX).unwrap();
                        family.own_path_query().unwrap()
                    } else {
                        let mut running = scene.into_execution().unwrap();
                        running
                            .scene_mut()
                            .append_family_points_3d(&family, &SUFFIX)
                            .unwrap();
                        running
                            .scene()
                            .effective_family_own_path_query(&family)
                            .unwrap()
                    };
                    let points = observed.points_3d().unwrap();
                    assert_eq!(points, expected, "mode={mode}, materialized={materialized}, attached={attached}, drawing={drawing}");
                    assert_eq!(family.node_id(), id);
                    assert_eq!(family.integration_store().borrow().len(), count);
                    assert_eq!(child.state().unwrap(), before_child);
                    assert_eq!(family.member_count().unwrap(), 1);
                    assert_eq!(family.draw_enabled().unwrap(), drawing);
                    assert_eq!(family.own_color().unwrap(), Color::GREEN);
                    assert_eq!(before.point_count(), if materialized { 4 } else { 0 });
                }
            }
        }
    }
}

#[test]
fn replacing_xyz_root_points_updates_only_its_existing_payload() {
    for mode in 0..2 {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        family.set_points_3d(&PREFIX).unwrap();
        family.shift(1.25, -0.75).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let before_child = child.state().unwrap();
        let actual = if mode == 0 {
            scene.set_family_points_3d(&family, &SUFFIX).unwrap();
            family.own_path_query().unwrap()
        } else {
            let execution = scene.execution_session().unwrap();
            scene.install_execution(execution);
            scene.set_family_points_3d(&family, &SUFFIX).unwrap();
            scene.effective_family_own_path_query(&family).unwrap()
        };
        assert_eq!(actual.points_3d().unwrap(), SUFFIX);
        assert_eq!(child.state().unwrap(), before_child);
    }
}

#[test]
fn invalid_xyz_and_stale_or_foreign_roots_cannot_publish_or_materialize() {
    let mut scene = Scene::new();
    let family = scene.family(&[]).unwrap();
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let revision = scene.revision();
    let publication = scene.owned_execution().publication_context();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    let invalid = [SemanticVec3::new(1.0, 2.0, f64::NAN)];
    assert!(scene.set_family_points_3d(&family, &invalid).is_err());
    assert!(scene.append_family_points_3d(&family, &invalid).is_err());
    let foreign = Scene::new().family(&[]).unwrap();
    assert!(scene.append_family_points_3d(&foreign, &[]).is_err());
    assert!(family.own_object().unwrap().is_none());
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.owned_execution().publication_context(), publication);
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
    let stale = MobjectFamily::create(Rc::clone(scene.integration_store()), &[]).unwrap();
    for points in [&SUFFIX[..], &[][..]] {
        assert!(scene.append_family_points_3d(&stale, points).is_err());
    }
    assert!(stale.own_object().unwrap().is_none());
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len(),
        resources
    );
}

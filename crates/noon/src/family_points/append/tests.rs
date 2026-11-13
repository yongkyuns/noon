use super::*;
use crate::{Color, Scene};

const PREFIX: [(f64, f64); 4] = [
    (0.123456789012345, 0.234567890123456),
    (0.345678901234567, 0.456789012345678),
    (0.567890123456789, 0.678901234567890),
    (0.789012345678901, 0.890123456789012),
];
const SUFFIX: [(f64, f64); 2] = [
    (1.23456789012345, -0.456789012345678),
    (2.34567890123456, 0.567890123456789),
];

#[test]
fn root_appends_preserve_precision_identity_children_and_presentation_in_all_hosts() {
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
                        family.set_points_f64(&PREFIX).unwrap();
                    }
                    family.shift(1.25, -0.75).unwrap();
                    if attached {
                        scene.add_many(&[(&family).into()]).unwrap();
                    }
                    let identity = family.node_id();
                    let nodes = scene.integration_store().borrow().len();
                    let child_before = child.state().unwrap();
                    let before = family.own_path_query().unwrap();
                    let mut expected = before.points().unwrap();
                    expected.extend(SUFFIX);
                    let observed = if mode == 0 {
                        family.append_points_f64(&SUFFIX).unwrap();
                        family.own_path_query().unwrap()
                    } else {
                        let mut running = scene.into_execution().unwrap();
                        running
                            .scene_mut()
                            .append_family_points_f64(&family, &SUFFIX)
                            .unwrap();
                        running
                            .scene()
                            .effective_family_own_path_query(&family)
                            .unwrap()
                    };
                    let points = observed.points().unwrap();
                    assert_eq!(points, expected, "mode={mode}, materialized={materialized}, attached={attached}, drawing={drawing}");
                    assert_eq!(family.node_id(), identity);
                    assert_eq!(family.integration_store().borrow().len(), nodes);
                    assert_eq!(family.member_count().unwrap(), 1);
                    assert_eq!(child.state().unwrap(), child_before);
                    assert_eq!(family.draw_enabled().unwrap(), drawing);
                    assert_eq!(family.own_color().unwrap(), Color::GREEN);
                    assert_eq!(
                        before.points().unwrap().len(),
                        if materialized { 4 } else { 0 }
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_root_appends_preserve_payload_publication_and_resource_ownership() {
    for materialized in [false, true] {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene
            .family_with_presentation(&[(&child).into()], 2.0, Color::GREEN, true)
            .unwrap();
        if materialized {
            family.set_points_f64(&PREFIX).unwrap();
        }
        scene.add_many(&[(&family).into()]).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let revision = scene.revision();
        let publication = scene.owned_execution().publication_context();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let before = family.own_path_query().unwrap().points().unwrap();
        assert!(scene
            .append_family_points_f64(&family, &[(f64::NAN, 0.0)],)
            .is_err());
        let foreign = Scene::new().family(&[]).unwrap();
        for points in [&SUFFIX[..], &[][..]] {
            assert!(scene.append_family_points_f64(&foreign, points,).is_err());
        }
        assert_eq!(family.own_path_query().unwrap().points().unwrap(), before);
        assert_eq!(family.own_object().unwrap().is_some(), materialized);
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
        let empty = MobjectFamily::create(Rc::clone(scene.integration_store()), &[]).unwrap();
        for points in [&SUFFIX[..], &[][..]] {
            assert!(scene.append_family_points_f64(&empty, points,).is_err());
        }
        assert!(empty.own_object().unwrap().is_none());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
    }
}

#[test]
fn empty_root_appends_do_not_materialize_payload_or_advance_publication() {
    let mut scene = Scene::new();
    let family = scene.family(&[]).unwrap();
    let revision = scene.revision();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    family.append_points_f64(&[]).unwrap();
    assert!(family.own_object().unwrap().is_none());
    assert_eq!(scene.revision(), revision);
    let execution = scene.execution_session().unwrap();
    scene.install_execution(execution);
    let publication = scene.owned_execution().publication_context();
    scene.append_family_points_f64(&family, &[]).unwrap();
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
}

#[test]
fn deleted_family_handles_cannot_mutate_replacement_identity() {
    for materialized in [false, true] {
        let mut scene = Scene::new();
        let family = scene.family(&[]).unwrap();
        if materialized {
            family.set_points_f64(&PREFIX).unwrap();
        }
        scene.add_many(&[(&family).into()]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let mut removal = noon_core::SemanticMutationTransaction::new();
        removal.remove_node(family.node_id());
        execution
            .apply_semantic_transaction_at_root(
                &mut scene.integration_store().borrow_mut(),
                scene.root(),
                removal,
            )
            .unwrap();
        let replacement = scene.family(&[]).unwrap();
        assert_ne!(family.node_id(), replacement.node_id());
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let revision = scene.revision();
        let publication = scene.owned_execution().publication_context();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        for points in [&SUFFIX[..], &[][..]] {
            assert!(family.append_points_f64(points).is_err());
            assert!(scene.append_family_points_f64(&family, points,).is_err());
        }
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
        assert!(replacement.own_object().unwrap().is_none());
    }
}

#[test]
fn materialized_root_appends_dirty_only_the_addressed_row() {
    let mut scene = Scene::new();
    let children = (0..4096)
        .map(|_| scene.square(0.25).unwrap())
        .collect::<Vec<_>>();
    let family = scene.family(&[(&children[2048]).into()]).unwrap();
    family.set_points_f64(&PREFIX).unwrap();
    scene
        .add_many(
            &children
                .iter()
                .map(crate::MobjectTarget::from)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut running = scene.into_execution().unwrap();
    let before = running.session().frame().objects.clone();
    running.session_mut().take_frame_changes();
    running
        .scene_mut()
        .append_family_points_f64(&family, &SUFFIX)
        .unwrap();
    let changes = running.session_mut().take_frame_changes();
    assert_eq!(changes.object_indices().len(), 1);
    let changed = changes.object_indices()[0];
    assert_eq!(running.session().frame().objects.len(), before.len());
    for (index, (before, after)) in before
        .iter()
        .zip(running.session().frame().objects.iter())
        .enumerate()
    {
        if index != changed {
            assert_eq!(before, after, "unrelated row {index}");
        }
    }
    assert!(running.session_mut().take_frame_changes().is_empty());
}

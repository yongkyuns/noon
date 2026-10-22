use super::*;
use crate::{LayoutDimension, MobjectTarget, Scene};
use noon_core::{SemanticOrientation, VectorPath};

fn points(value: &serde_json::Value) -> Vec<SemanticVec3> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            SemanticVec3::new(
                row[0].as_f64().unwrap(),
                row[1].as_f64().unwrap(),
                row[2].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn depth_xyz_affines_match_all_ce_pivots_in_authored_owned_and_borrowed_hosts() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/depth-affine.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 75);
    for mode in 0..3 {
        for case in fixture["cases"].as_array().unwrap() {
            let mut scene = Scene::new();
            let mut leaves = case["prefixXYZ"]
                .as_array()
                .unwrap()
                .iter()
                .map(|input| {
                    scene
                        .path(
                            VectorPath::from_manim_points_3d(points(input)),
                            Default::default(),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let family = if case["spec"]["family"] == true {
                let nested = scene
                    .family(&[(&leaves[0]).into(), (&leaves[1]).into()])
                    .unwrap();
                Some(
                    scene
                        .family(&[(&leaves[0]).into(), (&nested).into()])
                        .unwrap(),
                )
            } else {
                None
            };
            let target = family
                .as_ref()
                .map(MobjectTarget::from)
                .unwrap_or_else(|| (&leaves[0]).into());
            let anchor = family
                .as_ref()
                .map(LayoutAnchor::from)
                .unwrap_or_else(|| LayoutAnchor::from(&leaves[0]));
            scene.add_many(&[target]).unwrap();
            let unrelated = scene.square(1.).unwrap();
            scene.add(&unrelated).unwrap();
            let unrelated_before = unrelated.state().unwrap();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let ids = leaves.iter().map(Mobject::node_id).collect::<Vec<_>>();
            let pivot = match case["spec"]["pivot"].as_str().unwrap() {
                "center" => ManimRotationPivot::Center,
                "point" => ManimRotationPivot::Point3d(SemanticVec3::new(8., -9., 4.25)),
                "min" => ManimRotationPivot::Edge3d(SemanticVec3::new(0., 0., -1.)),
                "max" => ManimRotationPivot::Edge3d(SemanticVec3::new(0., 0., 1.)),
                "zero_edge" => ManimRotationPivot::Edge3d(SemanticVec3::ZERO),
                other => panic!("unknown pivot {other}"),
            };
            let delta = SemanticVec3::new(1.5, -2.5, 3.125);
            let factor = case["spec"]["factor"].as_f64().unwrap();
            let mut borrowed = None;
            if mode == 0 {
                if let Some(family) = &family {
                    family.shift_3d(delta).unwrap();
                } else {
                    leaves[0].shift_3d(delta).unwrap();
                }
                anchor
                    .stretch(factor, LayoutDimension::Depth, pivot)
                    .unwrap();
            } else {
                if mode == 1 {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                } else {
                    borrowed = Some(scene.execution_session().unwrap());
                }
                let mut live = if let Some(execution) = borrowed.as_mut() {
                    scene.live(execution)
                } else {
                    scene.owned_live()
                };
                if let Some(family) = &family {
                    live.shift_family_3d(family, delta).unwrap();
                } else {
                    live.shift_3d(&leaves[0], delta).unwrap();
                }
                live.stretch(&anchor, factor, LayoutDimension::Depth, pivot)
                    .unwrap();
            }
            let context = format!("mode={mode} spec={}", case["spec"]);
            for (index, leaf) in leaves.iter().enumerate() {
                let query = if let Some(execution) =
                    borrowed.as_ref().or_else(|| scene.running_execution())
                {
                    crate::path_queries::effective_path_query(
                        scene.integration_store(),
                        execution,
                        leaf,
                    )
                    .unwrap()
                } else {
                    leaf.path_query().unwrap()
                };
                let actual = query.points_3d().unwrap();
                let expected = points(&case["pointsXYZ"][index]);
                assert_eq!(actual.len(), expected.len(), "{context}");
                for (actual, expected) in actual.iter().zip(expected) {
                    for (a, b) in [
                        (actual.x, expected.x),
                        (actual.y, expected.y),
                        (actual.z, expected.z),
                    ] {
                        assert!(
                            (a - b).abs() <= 1e-13 * (1. + b.abs()),
                            "{context}: {a} != {b}"
                        );
                    }
                }
                assert!(matches!(
                    leaf.state().unwrap().transform.orientation,
                    SemanticOrientation::Planar(_)
                ));
                assert_eq!(leaf.node_id(), ids[index]);
            }
            assert_eq!(unrelated.state().unwrap(), unrelated_before);
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
}

#[test]
fn depth_xyz_invalid_family_edits_preserve_the_entire_publication() {
    for mode in 0..3 {
        let mut scene = Scene::new();
        let a = scene.square(1.).unwrap();
        let b = scene.square(2.).unwrap();
        let family = scene
            .family(&[(&a).into(), (&b).into(), (&a).into()])
            .unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        if mode == 1 {
            let owned = scene.execution_session().unwrap();
            scene.install_execution(owned);
        }
        for (factor, pivot) in [
            (f64::NAN, ManimRotationPivot::Center),
            (
                f64::INFINITY,
                ManimRotationPivot::Point3d(SemanticVec3::ZERO),
            ),
            (
                2.,
                ManimRotationPivot::Point3d(SemanticVec3::new(0., 0., f64::NAN)),
            ),
            (
                2.,
                ManimRotationPivot::Edge3d(SemanticVec3::new(0., 0., f64::INFINITY)),
            ),
        ] {
            let before = [a.state().unwrap(), b.state().unwrap()];
            let revision = scene.revision();
            let publication = if mode == 1 {
                scene.running_execution().unwrap().publication_context()
            } else {
                execution.publication_context()
            };
            let anchor = LayoutAnchor::from(&family);
            let rejected = if mode == 0 {
                anchor
                    .stretch(factor, LayoutDimension::Depth, pivot)
                    .is_err()
            } else if mode == 1 {
                scene
                    .owned_live()
                    .stretch(&anchor, factor, LayoutDimension::Depth, pivot)
                    .is_err()
            } else {
                scene
                    .live(&mut execution)
                    .stretch(&anchor, factor, LayoutDimension::Depth, pivot)
                    .is_err()
            };
            assert!(rejected);
            assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
            assert_eq!(scene.revision(), revision);
            assert_eq!(
                if mode == 1 {
                    scene.running_execution().unwrap().publication_context()
                } else {
                    execution.publication_context()
                },
                publication
            );
        }
    }
}

#[test]
fn depth_xyz_invalid_shifts_roll_back_all_aliased_leaves() {
    for live in [false, true] {
        let mut scene = Scene::new();
        let a = scene.square(1.).unwrap();
        let b = scene.square(2.).unwrap();
        let family = scene
            .family(&[(&a).into(), (&b).into(), (&a).into()])
            .unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let before = [a.state().unwrap(), b.state().unwrap()];
        let revision = scene.revision();
        let publication = execution.publication_context();
        for z in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let delta = SemanticVec3::new(1., 2., z);
            assert!(if live {
                scene
                    .live(&mut execution)
                    .shift_family_3d(&family, delta)
                    .is_err()
            } else {
                family.shift_3d(delta).is_err()
            });
            assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
            assert_eq!(scene.revision(), revision);
            assert_eq!(execution.publication_context(), publication);
        }
    }
}

#[test]
fn inert_depth_scale_preserves_real_controls_tails_and_morph_targets_in_all_hosts() {
    for host in 0..3 {
        for profile in 0..5 {
            let mut controls = vec![
                SemanticVec3::new(-1., 0., 0.),
                SemanticVec3::new(0., 1., 0.),
                SemanticVec3::new(0., -1., 0.),
                SemanticVec3::new(1., 0., 0.),
            ];
            if profile == 1 {
                controls[1].z = 2.;
            } else if profile == 2 {
                controls.push(SemanticVec3::new(2., 0., 4.));
            }
            let mut path = VectorPath::from_manim_points_3d(controls.clone());
            if profile == 3 {
                let mut target = controls.clone();
                target[1].z = 2.;
                path = path.with_morph_target(VectorPath::from_manim_points_3d(target));
            } else if profile == 4 {
                path = VectorPath::new()
                    .move_to(noon_core::Vec2::new(-1., 0.))
                    .line_to(noon_core::Vec2::new(1., 0.));
            }
            let mut scene = Scene::new();
            let object = scene.path(path, Default::default()).unwrap();
            scene.add(&object).unwrap();
            let anchor = LayoutAnchor::from(&object);
            let mut borrowed = None;
            if host == 1 {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
            } else if host == 2 {
                borrowed = Some(scene.execution_session().unwrap());
            }
            let before = object.state().unwrap();
            let revision = scene.revision();
            if host == 0 {
                anchor
                    .stretch(
                        3.,
                        LayoutDimension::Depth,
                        ManimRotationPivot::Point3d(SemanticVec3::ZERO),
                    )
                    .unwrap();
            } else {
                let mut live = if let Some(execution) = borrowed.as_mut() {
                    scene.live(execution)
                } else {
                    scene.owned_live()
                };
                live.stretch(
                    &anchor,
                    3.,
                    LayoutDimension::Depth,
                    ManimRotationPivot::Point3d(SemanticVec3::ZERO),
                )
                .unwrap();
            }
            let state = object.state().unwrap();
            let has_depth = matches!(profile, 1..=3);
            if has_depth {
                assert_eq!(state.transform.scale.z, 3., "host={host},profile={profile}");
                assert_eq!(scene.revision(), revision.checked_next().unwrap());
            } else {
                assert_eq!(state, before, "host={host},profile={profile}");
                assert_eq!(scene.revision(), revision);
            }
            let query =
                if let Some(execution) = borrowed.as_ref().or_else(|| scene.running_execution()) {
                    crate::path_queries::effective_path_query(
                        scene.integration_store(),
                        execution,
                        &object,
                    )
                    .unwrap()
                } else {
                    object.path_query().unwrap()
                };
            let points = query.points_3d().unwrap();
            if profile != 4 {
                let expected = controls
                    .iter()
                    .map(|point| SemanticVec3::new(point.x, point.y, point.z * 3.))
                    .collect::<Vec<_>>();
                assert_eq!(points, expected, "host={host},profile={profile}");
            } else {
                assert!(points.iter().all(|point| point.z == 0.));
            }
            if profile == 3 {
                let store = scene.integration_store().borrow();
                let path = crate::path_queries::content_path(&store, state.content).unwrap();
                let target = crate::PathQuery::prepare(
                    path.morph_target().unwrap(),
                    state.transform.as_planar().unwrap(),
                )
                .unwrap();
                assert_eq!(target.points_3d().unwrap()[1].z, 6.);
            }
        }
    }
}

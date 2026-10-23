use super::*;
use crate::{ManimRotationPivot as Pivot, MobjectTarget, Scene, SemanticVec3};
use noon_core::VectorPath;

fn points(value: &serde_json::Value) -> Vec<SemanticVec3> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            SemanticVec3::new(
                p[0].as_f64().unwrap(),
                p[1].as_f64().unwrap(),
                p[2].as_f64().unwrap(),
            )
        })
        .collect()
}

fn close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= 2e-13 * (1.0 + expected.abs()),
        "{context}: {actual} != {expected}"
    );
}

#[test]
fn fitting_xyz_matches_actual_ce_in_authored_owned_and_borrowed_execution() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/fitting-xyz.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 720);
    for mode in 0..3 {
        for case in fixture["cases"].as_array().unwrap() {
            for matching in [false, true] {
                if matching && case["spec"]["length"] != 7.0 {
                    continue;
                }
                let mut scene = Scene::new();
                let leaves = case["prefixXYZ"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        scene
                            .path(
                                VectorPath::from_manim_points_3d(points(p)),
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
                let target: MobjectTarget<'_> = family
                    .as_ref()
                    .map(MobjectTarget::from)
                    .unwrap_or_else(|| (&leaves[0]).into());
                let anchor = family
                    .as_ref()
                    .map(LayoutAnchor::from)
                    .unwrap_or_else(|| LayoutAnchor::from(&leaves[0]));
                scene.add_many(&[target]).unwrap();
                // Every coordinate extent is exactly seven. Matching this
                // independent target must agree with the pinned length=7 fit.
                let peer = scene
                    .path(
                        VectorPath::from_manim_points_3d(vec![
                            SemanticVec3::ZERO,
                            SemanticVec3::new(1., 2., 3.),
                            SemanticVec3::new(4., 5., 6.),
                            SemanticVec3::new(7., 7., 7.),
                        ]),
                        Default::default(),
                    )
                    .unwrap();
                scene.add(&peer).unwrap();
                let peer_before = peer.state().unwrap();
                let peer_anchor = LayoutAnchor::from(&peer);
                let context = format!("mode={mode}, matching={matching}, spec={}", case["spec"]);
                for (dim, expected) in case["before"]["lengths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    close(
                        anchor
                            .dimension_length((dim as u32).try_into().unwrap())
                            .unwrap(),
                        expected.as_f64().unwrap(),
                        &context,
                    );
                    close(
                        target.length_over_dim(dim).unwrap(),
                        expected.as_f64().unwrap(),
                        &context,
                    );
                }
                let dimension = (case["spec"]["dimension"].as_u64().unwrap() as u32)
                    .try_into()
                    .unwrap();
                let stretch = case["spec"]["stretch"].as_bool().unwrap();
                let pivot = match case["spec"]["pivot"].as_str().unwrap() {
                    "center" => Pivot::Center,
                    "point" => Pivot::Point3d(SemanticVec3::new(8., -9., 4.25)),
                    "min" => Pivot::Edge3d(SemanticVec3::new(0., 0., -1.)),
                    "max" => Pivot::Edge3d(SemanticVec3::new(0., 0., 1.)),
                    _ => unreachable!(),
                };
                let length = case["spec"]["length"].as_f64().unwrap();
                let before = leaves
                    .iter()
                    .map(|p| p.state().unwrap())
                    .collect::<Vec<_>>();
                let revision = scene.revision();
                let resources = scene
                    .integration_store()
                    .borrow()
                    .geometry_resources()
                    .len();
                let mut borrowed = None;
                let rejected = if mode == 0 {
                    let result = if matching {
                        anchor.match_dim_size_with_pivot(&peer_anchor, dimension, stretch, pivot)
                    } else {
                        anchor.rescale_to_fit_with_pivot(length, dimension, stretch, pivot)
                    };
                    if let Err(error) = &result {
                        assert!(
                            matches!(
                                error,
                                AuthoringError::PathQuery(
                                    noon_geometry::PathProportionError::UnpairedAnchors { .. }
                                )
                            ),
                            "{context}: {error}"
                        );
                    }
                    result.is_err()
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
                    let result = if matching {
                        live.match_dim_size_with_pivot(
                            &anchor,
                            &peer_anchor,
                            dimension,
                            stretch,
                            pivot,
                        )
                    } else {
                        live.rescale_to_fit_with_pivot(&anchor, length, dimension, stretch, pivot)
                    };
                    result.is_err()
                };
                assert_eq!(rejected, case["outcome"]["kind"] == "raised", "{context}");
                if rejected {
                    assert_eq!(
                        leaves
                            .iter()
                            .map(|p| p.state().unwrap())
                            .collect::<Vec<_>>(),
                        before,
                        "{context}"
                    );
                    assert_eq!(scene.revision(), revision, "{context}");
                }
                let execution = borrowed.as_ref().or_else(|| scene.running_execution());
                for (index, leaf) in leaves.iter().enumerate() {
                    let query = if let Some(execution) = execution {
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
                    for (a, b) in actual.iter().zip(expected) {
                        for (a, b) in [(a.x, b.x), (a.y, b.y), (a.z, b.z)] {
                            close(a, b, &context);
                        }
                    }
                }
                for (dim, expected) in case["after"]["lengths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                {
                    let dim = (dim as u32).try_into().unwrap();
                    let actual = if let Some(execution) = borrowed.as_mut() {
                        scene
                            .live(execution)
                            .dimension_length(&anchor, dim)
                            .unwrap()
                    } else if mode == 1 {
                        scene.owned_live().dimension_length(&anchor, dim).unwrap()
                    } else {
                        anchor.dimension_length(dim).unwrap()
                    };
                    close(actual, expected.as_f64().unwrap(), &context);
                }
                assert_eq!(peer.state().unwrap(), peer_before, "{context}");
                assert_eq!(
                    scene
                        .integration_store()
                        .borrow()
                        .geometry_resources()
                        .len(),
                    resources,
                    "{context}"
                );
            }
        }
    }
}

mod rotated;

use super::*;
use crate::{MobjectTarget, Scene};
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

fn compare(
    scene: &Scene,
    borrowed: Option<&crate::ExecutionSession>,
    leaves: &[Mobject],
    expected: &serde_json::Value,
    context: &str,
) {
    for (leaf, expected) in leaves.iter().zip(expected.as_array().unwrap()) {
        let query = if let Some(execution) = borrowed.or_else(|| scene.running_execution()) {
            crate::path_queries::effective_path_query(scene.integration_store(), execution, leaf)
                .unwrap()
        } else {
            leaf.path_query().unwrap()
        };
        let actual = query.points_3d().unwrap();
        let expected = points(expected);
        assert_eq!(actual.len(), expected.len(), "{context}");
        for (a, b) in actual.iter().zip(expected) {
            for (a, b) in [(a.x, b.x), (a.y, b.y), (a.z, b.z)] {
                assert!(
                    (a - b).abs() <= 2e-13 * (1. + b.abs()),
                    "{context}: {a} != {b}"
                );
            }
        }
    }
}

#[test]
fn generic_scale_xyz_matches_ce_in_all_native_hosts() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/scale-xyz.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 288);
    for mode in 0..3 {
        for case in fixture["cases"].as_array().unwrap() {
            let mut scene = Scene::new();
            let mut leaves = Vec::new();
            for (index, input) in case["initialXYZ"].as_array().unwrap().iter().enumerate() {
                let mut object = if case["spec"]["shape"] == "line" {
                    if index == 0 {
                        scene.line((-0.25, 1.5), (3.25, -0.5)).unwrap()
                    } else {
                        scene.line((2.5, 1.25), (4.25, -1.75)).unwrap()
                    }
                } else {
                    scene
                        .path(
                            VectorPath::from_manim_points_3d(points(input)),
                            Default::default(),
                        )
                        .unwrap()
                };
                object.set_stroke_width(0.).unwrap();
                leaves.push(object);
            }
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
            let anchor = family
                .as_ref()
                .map(LayoutAnchor::from)
                .unwrap_or_else(|| LayoutAnchor::from(&leaves[0]));
            let target = family
                .as_ref()
                .map(MobjectTarget::from)
                .unwrap_or_else(|| (&leaves[0]).into());
            scene.add_many(&[target]).unwrap();
            let unrelated = scene.square(1.).unwrap();
            scene.add(&unrelated).unwrap();
            let untouched = unrelated.state().unwrap();
            let ids = leaves.iter().map(Mobject::node_id).collect::<Vec<_>>();
            let mut borrowed = None;
            if mode == 1 {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
            } else if mode == 2 {
                borrowed = Some(scene.execution_session().unwrap());
            }
            let delta = SemanticVec3::new(1.5, -2.5, 3.125);
            let angle = case["spec"]["angle"].as_f64().unwrap();
            if mode == 0 {
                if let Some(family) = &family {
                    family.shift_3d(delta).unwrap();
                } else {
                    leaves[0].shift_3d(delta).unwrap();
                }
                if angle != 0. {
                    anchor
                        .rotate(angle, ManimRotationPivot::Point(0.25, -0.75))
                        .unwrap();
                }
            } else {
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
                if angle != 0. {
                    live.rotate_layout(&anchor, angle, ManimRotationPivot::Point(0.25, -0.75))
                        .unwrap();
                }
            }
            let context = format!("mode={mode},spec={}", case["spec"]);
            compare(
                &scene,
                borrowed.as_ref(),
                &leaves,
                &case["beforeXYZ"],
                &context,
            );
            let pivot = match case["spec"]["pivot"].as_str().unwrap() {
                "center" => ManimRotationPivot::Center,
                "point" => ManimRotationPivot::Point3d(SemanticVec3::new(
                    0.1234567890123,
                    -0.2345678901234,
                    4.25,
                )),
                "edge" => ManimRotationPivot::Edge3d(SemanticVec3::new(1., -1., 1.)),
                _ => unreachable!(),
            };
            let value = &case["spec"]["factor"];
            let factors = if let Some(factor) = value.as_f64() {
                SemanticVec3::new(factor, factor, factor)
            } else {
                points(&serde_json::json!([value]))[0]
            };
            let before = leaves
                .iter()
                .map(|leaf| leaf.state().unwrap())
                .collect::<Vec<_>>();
            let revision = scene.revision();
            let resources = scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len();
            let publication = borrowed
                .as_ref()
                .or_else(|| scene.running_execution())
                .map(|execution| execution.publication_context());
            let nodes = scene.integration_store().borrow().len();
            let rejected = if mode == 0 {
                anchor.scale_3d(factors, pivot).is_err()
            } else {
                let mut live = if let Some(execution) = borrowed.as_mut() {
                    scene.live(execution)
                } else {
                    scene.owned_live()
                };
                live.scale_layout_3d(&anchor, factors, pivot).is_err()
            };
            assert_eq!(rejected, case["outcome"]["kind"] == "raised", "{context}");
            if rejected {
                assert_eq!(
                    leaves
                        .iter()
                        .map(|leaf| leaf.state().unwrap())
                        .collect::<Vec<_>>(),
                    before,
                    "{context}"
                );
                assert_eq!(scene.revision(), revision, "{context}");
                assert_eq!(
                    scene
                        .integration_store()
                        .borrow()
                        .geometry_resources()
                        .len(),
                    resources,
                    "{context}"
                );
                assert_eq!(
                    borrowed
                        .as_ref()
                        .or_else(|| scene.running_execution())
                        .map(|execution| execution.publication_context()),
                    publication,
                    "{context}"
                );
            }
            compare(
                &scene,
                borrowed.as_ref(),
                &leaves,
                &case["pointsXYZ"],
                &context,
            );
            assert_eq!(scene.integration_store().borrow().len(), nodes);
            assert_eq!(leaves.iter().map(Mobject::node_id).collect::<Vec<_>>(), ids);
            assert_eq!(unrelated.state().unwrap(), untouched);
        }
    }
}

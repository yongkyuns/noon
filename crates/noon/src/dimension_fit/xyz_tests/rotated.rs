use super::*;

fn observe(
    scene: &Scene,
    execution: Option<&crate::ExecutionSession>,
    object: &crate::Mobject,
) -> Vec<SemanticVec3> {
    if let Some(execution) = execution {
        crate::path_queries::effective_path_query(scene.integration_store(), execution, object)
            .unwrap()
    } else {
        object.path_query().unwrap()
    }
    .points_3d()
    .unwrap()
}

fn compare(actual: Vec<SemanticVec3>, expected: &serde_json::Value, context: &str) {
    let expected = points(expected);
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (a, b) in actual.iter().zip(expected) {
        for (a, b) in [(a.x, b.x), (a.y, b.y), (a.z, b.z)] {
            close(a, b, context);
        }
    }
}

#[test]
fn rotated_xyz_fitting_matches_ce_through_all_native_hosts() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/rotated-xyz.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 648);
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
            let ids = leaves
                .iter()
                .map(crate::Mobject::node_id)
                .collect::<Vec<_>>();
            let mut borrowed = None;
            if mode == 1 {
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
            } else if mode == 2 {
                borrowed = Some(scene.execution_session().unwrap());
            }
            let angle = case["spec"]["angle"].as_f64().unwrap();
            let delta = SemanticVec3::new(1.5, -2.5, 3.125);
            if mode == 0 {
                anchor
                    .stretch(
                        2.,
                        LayoutDimension::Depth,
                        Pivot::Point3d(SemanticVec3::ZERO),
                    )
                    .unwrap();
                if let Some(family) = &family {
                    family.shift_3d(delta).unwrap();
                } else {
                    leaves[0].shift_3d(delta).unwrap();
                }
                anchor.rotate(angle, Pivot::Point(0.25, -0.75)).unwrap();
            } else {
                let mut live = if let Some(execution) = borrowed.as_mut() {
                    scene.live(execution)
                } else {
                    scene.owned_live()
                };
                live.stretch(
                    &anchor,
                    2.,
                    LayoutDimension::Depth,
                    Pivot::Point3d(SemanticVec3::ZERO),
                )
                .unwrap();
                if let Some(family) = &family {
                    live.shift_family_3d(family, delta).unwrap();
                } else {
                    live.shift_3d(&leaves[0], delta).unwrap();
                }
                live.rotate_layout(&anchor, angle, Pivot::Point(0.25, -0.75))
                    .unwrap();
            }
            let context = format!("mode={mode},spec={}", case["spec"]);
            for (index, leaf) in leaves.iter().enumerate() {
                compare(
                    observe(
                        &scene,
                        borrowed.as_ref().or_else(|| scene.running_execution()),
                        leaf,
                    ),
                    &case["beforeXYZ"][index],
                    &context,
                );
            }
            let dimension = (case["spec"]["dimension"].as_u64().unwrap() as u32)
                .try_into()
                .unwrap();
            let stretch = case["spec"]["stretch"].as_bool().unwrap();
            let length = case["spec"]["length"].as_f64().unwrap();
            let pivot = match case["spec"]["pivot"].as_str().unwrap() {
                "center" => Pivot::Center,
                "point" => {
                    Pivot::Point3d(SemanticVec3::new(0.1234567890123, -0.2345678901234, 4.25))
                }
                "edge" => Pivot::Edge3d(SemanticVec3::new(1., -1., 1.)),
                _ => unreachable!(),
            };
            let nodes = scene.integration_store().borrow().len();
            if mode == 0 {
                anchor
                    .rescale_to_fit_with_pivot(length, dimension, stretch, pivot)
                    .unwrap();
            } else {
                let mut live = if let Some(execution) = borrowed.as_mut() {
                    scene.live(execution)
                } else {
                    scene.owned_live()
                };
                live.rescale_to_fit_with_pivot(&anchor, length, dimension, stretch, pivot)
                    .unwrap();
            }
            for (index, leaf) in leaves.iter().enumerate() {
                compare(
                    observe(
                        &scene,
                        borrowed.as_ref().or_else(|| scene.running_execution()),
                        leaf,
                    ),
                    &case["pointsXYZ"][index],
                    &context,
                );
                assert_eq!(leaf.node_id(), ids[index]);
            }
            assert_eq!(scene.integration_store().borrow().len(), nodes);
            assert_eq!(unrelated.state().unwrap(), untouched);
        }
    }
}

#[test]
fn rotated_xyz_planar_command_baking_matches_actual_ce_precision() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../fixtures/manim-v0.21/rotated-xyz.json"
    ))
    .unwrap();
    let reference = &fixture["planarCommandControl"];
    for mode in 0..3 {
        let mut scene = Scene::new();
        let mut line = scene.line((-0.25, 1.5), (3.25, -0.5)).unwrap();
        line.set_stroke_width(0.).unwrap();
        line.shift(
            reference["shift"][0].as_f64().unwrap(),
            reference["shift"][1].as_f64().unwrap(),
        )
        .unwrap();
        line.rotate_with_pivot(0.37, Pivot::Point(0., 0.)).unwrap();
        scene.add(&line).unwrap();
        let anchor = LayoutAnchor::from(&line);
        let mut borrowed = None;
        if mode == 1 {
            let execution = scene.execution_session().unwrap();
            scene.install_execution(execution);
        } else if mode == 2 {
            borrowed = Some(scene.execution_session().unwrap());
        }
        let context = format!("planar command mode={mode}");
        compare(
            observe(
                &scene,
                borrowed.as_ref().or_else(|| scene.running_execution()),
                &line,
            ),
            &reference["beforeXYZ"],
            &context,
        );
        let pivot = Pivot::Point3d(points(&serde_json::json!([reference["pivot"]]))[0]);
        let length = reference["length"].as_f64().unwrap();
        if mode == 0 {
            anchor
                .rescale_to_fit_with_pivot(length, LayoutDimension::Width, true, pivot)
                .unwrap();
        } else {
            let mut live = if let Some(execution) = borrowed.as_mut() {
                scene.live(execution)
            } else {
                scene.owned_live()
            };
            live.rescale_to_fit_with_pivot(&anchor, length, LayoutDimension::Width, true, pivot)
                .unwrap();
        }
        compare(
            observe(
                &scene,
                borrowed.as_ref().or_else(|| scene.running_execution()),
                &line,
            ),
            &reference["pointsXYZ"],
            &context,
        );
    }
}

#[test]
fn rotated_xyz_world_replacement_publishes_baked_depth_once() {
    for command_only in [false, true] {
        let mut scene = Scene::new();
        let mut object = if command_only {
            scene.line((-0.25, 1.5), (3.25, -0.5)).unwrap()
        } else {
            scene
                .path(
                    VectorPath::from_manim_points_3d(vec![
                        SemanticVec3::new(0.125, 0.25, 1.125),
                        SemanticVec3::new(0.5, 0.75, 40.25),
                        SemanticVec3::new(1.0, 1.5, -30.5),
                        SemanticVec3::new(2.0, 3.0, 7.375),
                    ]),
                    Default::default(),
                )
                .unwrap()
        };
        object.set_stroke_width(0.0).unwrap();
        LayoutAnchor::from(&object)
            .stretch(
                2.0,
                LayoutDimension::Depth,
                Pivot::Point3d(SemanticVec3::ZERO),
            )
            .unwrap();
        object
            .shift_3d(SemanticVec3::new(1.5, -2.5, 3.125))
            .unwrap();
        object
            .rotate_with_pivot(0.37, Pivot::Point(0.25, -0.75))
            .unwrap();
        let before = object.path_query().unwrap().points_3d().unwrap();
        let captured = object.state().unwrap();
        let id = object.node_id();
        let prepared = {
            let store = scene.integration_store().borrow();
            let (replacement, path) = crate::family_affine::world_scaled_replacement(
                &store,
                &captured,
                1.2345678901234,
                -0.8765432109876,
                (0.125, -0.25),
                (2.75, 1.625),
            )
            .unwrap();
            assert_eq!(replacement.transform.translation, SemanticVec3::ZERO);
            assert_eq!(
                replacement.transform.scale,
                SemanticVec3::new(1.0, 1.0, 1.0)
            );
            assert!(path.manim_points().is_some());
            crate::path_editing::PreparedPathEdits::prepare(&store, [(id, replacement, path)])
                .unwrap()
        };
        prepared
            .publish(
                &mut scene.integration_store().borrow_mut(),
                |store, transaction| {
                    transaction
                        .apply(store)
                        .map(|_| ())
                        .map_err(AuthoringError::from)
                },
            )
            .unwrap();
        assert_eq!(object.node_id(), id);
        let after = object.path_query().unwrap().points_3d().unwrap();
        assert_eq!(after.len(), before.len());
        for (before, after) in before.iter().zip(after) {
            assert_eq!(before.z, after.z);
        }
    }
}

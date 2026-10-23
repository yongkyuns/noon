#![cfg(all(feature = "native-text", feature = "bundled-fonts"))]
//! Actual pinned ManimCE coordinates, including retained native Text and parts.
use noon::{
    CompositeEntryHandle, ImageMobjectOptions, LayoutAnchor, Scene, SemanticVec3, Table,
    TableOptions, Text,
};
use serde_json::Value;

fn anchor(root: &CompositeEntryHandle) -> LayoutAnchor {
    match root {
        CompositeEntryHandle::Mobject(value) => value.into(),
        CompositeEntryHandle::Family(value) => value.into(),
    }
}

fn prepare(scene: &mut Scene, kind: &str) -> CompositeEntryHandle {
    let image = |scene: &mut Scene| {
        let mut options = ImageMobjectOptions::rgba8(4, 2, vec![255; 32]).unwrap();
        options.set_height(2.).unwrap();
        scene.image(options).unwrap()
    };
    let mut root = match kind {
        "ImageMobject" => CompositeEntryHandle::Mobject(image(scene)),
        "TableEntry" => Table::from_rows_with_options(
            scene,
            [["Parity"]],
            TableOptions {
                include_outer_lines: true,
                ..Default::default()
            },
        )
        .unwrap()
        .get_entries()
        .unwrap()
        .remove(0),
        "Text" => CompositeEntryHandle::Mobject(
            scene
                .text(
                    Text::new("Parity")
                        .with_font("DejaVu Sans Mono")
                        .with_font_size(24.),
                )
                .unwrap(),
        ),
        "Group" | "VGroup" => {
            let mut text = scene
                .text(
                    Text::new("Parity")
                        .with_font("DejaVu Sans Mono")
                        .with_font_size(24.),
                )
                .unwrap();
            text.shift_3d(SemanticVec3::new(-3., 0., 1.125)).unwrap();
            let mut square = scene.square(2.).unwrap();
            square.shift_3d(SemanticVec3::new(2., 0., -3.5)).unwrap();
            let nested = scene.family(&[(&text).into(), (&square).into()]).unwrap();
            let family = if kind == "Group" {
                let mut picture = image(scene);
                picture.shift_3d(SemanticVec3::new(0., 2., 4.25)).unwrap();
                scene
                    .family(&[(&nested).into(), (&text).into(), (&picture).into()])
                    .unwrap()
            } else {
                scene.family(&[(&nested).into(), (&text).into()]).unwrap()
            };
            CompositeEntryHandle::Family(family)
        }
        _ => panic!("unexpected retained content profile"),
    };
    match &mut root {
        CompositeEntryHandle::Mobject(value) => {
            value.shift_3d(SemanticVec3::new(1.5, -2.5, 2.375)).unwrap();
        }
        CompositeEntryHandle::Family(value) => {
            value.shift_3d(SemanticVec3::new(1.5, -2.5, 2.375)).unwrap();
        }
    }
    root
}

fn direction(query: &Value) -> SemanticVec3 {
    if let Some(value) = query["args"].as_array().and_then(|args| args.first()) {
        return SemanticVec3::new(
            value[0].as_f64().unwrap(),
            value[1].as_f64().unwrap(),
            value[2].as_f64().unwrap(),
        );
    }
    match query["method"].as_str().unwrap() {
        "get_center" => SemanticVec3::ZERO,
        "get_left" => SemanticVec3::new(-1., 0., 0.),
        "get_right" => SemanticVec3::new(1., 0., 0.),
        "get_top" => SemanticVec3::new(0., 1., 0.),
        "get_bottom" => SemanticVec3::new(0., -1., 0.),
        "get_zenith" => SemanticVec3::new(0., 0., 1.),
        "get_nadir" => SemanticVec3::new(0., 0., -1.),
        _ => panic!("unexpected retained query"),
    }
}

#[test]
fn retained_content_precision_matches_all_pinned_coordinates_without_query_publication() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/text/retained-content-xyz-manim-v0.21.0.json"
    ))
    .unwrap();
    let mut observations = 0;
    let mut residuals = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let kind = case["kind"].as_str().unwrap();
        let mut scene = Scene::new();
        let root = prepare(&mut scene, kind);
        let source = anchor(&root);
        let mut execution = None;
        for (phase_index, phase) in case["phases"].as_array().unwrap().iter().enumerate() {
            if phase_index == 1 {
                scene.add_many(&[root.as_target()]).unwrap();
            }
            if phase_index == 2 {
                execution = Some(scene.execution_session().unwrap());
            }
            if phase_index == 3 {
                let mut live = scene.live(execution.as_mut().unwrap());
                match &root {
                    CompositeEntryHandle::Mobject(value) => {
                        live.shift_3d(value, SemanticVec3::new(0.5, 0.25, 1.125))
                            .unwrap();
                    }
                    CompositeEntryHandle::Family(value) => {
                        live.shift_family_3d(value, SemanticVec3::new(0.5, 0.25, 1.125))
                            .unwrap();
                    }
                }
            }
            if phase_index == 4 {
                scene
                    .live(execution.as_mut().unwrap())
                    .remove_many(&[root.as_target()])
                    .unwrap();
            }
            let revision = scene.revision();
            let publication = execution.as_ref().map(|value| value.publication_context());
            for (index, query) in phase["queries"].as_array().unwrap().iter().enumerate() {
                let actual = if let Some(execution) = execution.as_mut() {
                    scene
                        .live(execution)
                        .layout_critical_point_3d(&source, direction(query))
                        .unwrap()
                } else {
                    source.critical_point_3d(direction(query)).unwrap()
                };
                let actual = [actual.x, actual.y, actual.z];
                let expected: [f64; 3] =
                    std::array::from_fn(|axis| query["output"]["value"][axis].as_f64().unwrap());
                let matches = actual
                    .iter()
                    .zip(expected)
                    .all(|(a, e)| (a - e).abs() <= 1e-12 * (1. + e.abs()));
                observations += 1;
                if !matches {
                    residuals.push((
                        kind.to_owned(),
                        phase["phase"].as_str().unwrap().to_owned(),
                        index,
                        actual,
                        expected,
                    ));
                }
            }
            assert_eq!(
                scene.revision(),
                revision,
                "authored queries must not mutate"
            );
            assert_eq!(
                execution.as_ref().map(|value| value.publication_context()),
                publication,
                "queries must not publish"
            );
        }
    }
    assert_eq!(observations, 475);
    for row in &residuals {
        eprintln!("retained content mismatch: {row:?}");
    }
    assert!(
        residuals.is_empty(),
        "{} of475 pinned coordinates differ",
        residuals.len()
    );
}

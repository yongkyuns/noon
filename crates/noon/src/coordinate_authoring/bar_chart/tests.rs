use super::*;

fn options() -> ManimBarChartOptions {
    ManimBarChartOptions::new(vec![-2.0, 0.0, 3.0], [-4.0, 4.0, 1.0], 6.0, 4.0)
}

#[test]
fn bars_share_one_axes_frame_and_support_signed_and_zero_values() {
    let mut scene = Scene::new();
    let chart = scene.bar_chart(&options()).unwrap();
    let nodes = direct_bar_nodes(chart.bars()).unwrap();
    let store = scene.integration_store().borrow();
    let states: Vec<_> = nodes
        .iter()
        .map(|node| store.semantic_object_state_checked(*node).unwrap().clone())
        .collect();
    assert_eq!(states.len(), 3);
    for state in &states {
        assert_eq!(state.style.stroke_width, 0.03);
        assert_eq!(
            state.style.stroke_width_mode,
            noon_core::StrokeWidthMode::ScreenSpace
        );
    }
    assert!(states[0].transform.translation.y < states[1].transform.translation.y);
    assert_eq!(states[1].transform.scale.y, 0.0);
    assert!(states[2].transform.translation.y > states[1].transform.translation.y);
}

#[test]
fn updates_are_atomic_and_touch_only_requested_bars_when_color_is_unchanged() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    let nodes = direct_bar_nodes(chart.bars()).unwrap();
    let before: Vec<_> = nodes
        .iter()
        .map(|node| {
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(*node)
                .unwrap()
                .clone()
        })
        .collect();
    chart
        .change_bar_values(&mut scene, &[1.0, -1.0], false)
        .unwrap();
    let updated_nodes = direct_bar_nodes(chart.bars()).unwrap();
    assert_ne!(updated_nodes[1], nodes[1]);
    let after: Vec<_> = updated_nodes
        .iter()
        .map(|node| {
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(*node)
                .unwrap()
                .clone()
        })
        .collect();
    assert_ne!(before[0].transform, after[0].transform);
    assert_ne!(before[1].transform, after[1].transform);
    assert_eq!(before[2], after[2]);
    assert_eq!(before[0].style, after[0].style);
    assert_eq!(before[1].style, after[1].style);

    let snapshot = after.clone();
    assert!(chart
        .change_bar_values(&mut scene, &[f64::NAN], false)
        .is_err());
    let rolled_back: Vec<_> = updated_nodes
        .iter()
        .map(|node| {
            scene
                .integration_store()
                .borrow()
                .semantic_object_state_checked(*node)
                .unwrap()
                .clone()
        })
        .collect();
    assert_eq!(snapshot, rolled_back);
}

#[test]
fn update_uses_the_current_transformed_axes_frame() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    chart.family().shift(2.0, -1.0).unwrap();
    chart.change_bar_values(&mut scene, &[2.0], false).unwrap();
    let node = direct_bar_nodes(chart.bars()).unwrap()[0];
    let state = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(node)
        .unwrap()
        .clone();
    let expected = chart
        .axes()
        .authored_frame()
        .unwrap()
        .coords_to_point(0.5, 1.0)
        .unwrap();
    assert!((state.transform.translation.x - expected[0]).abs() < 1e-9);
    assert!((state.transform.translation.y - expected[1]).abs() < 1e-9);
}

#[test]
fn updates_preserve_a_ninety_degree_axes_basis() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    chart
        .family()
        .rotate(
            std::f64::consts::FRAC_PI_2,
            crate::ManimRotationPivot::Center,
        )
        .unwrap();
    chart.change_bar_values(&mut scene, &[2.0], false).unwrap();

    let node = direct_bar_nodes(chart.bars()).unwrap()[0];
    let state = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(node)
        .unwrap()
        .clone();
    assert!((state.transform.rotation_z - std::f64::consts::FRAC_PI_2).abs() < 1.0e-9);
    assert!(state.transform.scale.x > 0.0);
    assert!(state.transform.scale.y > 0.0);
}

#[test]
fn short_update_leaves_a_large_chart_suffix_unchanged() {
    let mut scene = Scene::new();
    let values = (0..2_000).map(|index| index as f64).collect();
    let mut chart = scene
        .bar_chart(&ManimBarChartOptions::new(
            values,
            [0.0, 2_000.0, 1.0],
            12.0,
            4.0,
        ))
        .unwrap();
    let nodes = direct_bar_nodes(chart.bars()).unwrap();
    let suffix = nodes[1_999];
    let before = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(suffix)
        .unwrap()
        .clone();
    chart
        .change_bar_values(&mut scene, &[123.0], false)
        .unwrap();
    assert_eq!(
        scene
            .integration_store()
            .borrow()
            .semantic_object_state_checked(suffix)
            .unwrap(),
        &before
    );
}

#[test]
fn skewed_axes_do_not_replace_nonzero_bar_geometry() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    chart
        .axes()
        .y_axis()
        .unwrap()
        .family()
        .rotate(0.1, crate::ManimRotationPivot::Center)
        .unwrap();
    let node = direct_bar_nodes(chart.bars()).unwrap()[0];
    let before = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(node)
        .unwrap()
        .clone();
    chart.change_bar_values(&mut scene, &[2.0], false).unwrap();
    let after = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(node)
        .unwrap()
        .clone();
    assert_eq!(after.content, before.content);
    assert_eq!(after.transform.scale, before.transform.scale);
    assert_eq!(bar_metadata(&after).unwrap().value, 2.0);
}

#[test]
fn manim_defaults_match_the_compatibility_constructor_policy() {
    let options = ManimBarChartOptions::manim_defaults(vec![-2.0, 3.0], None, None, None).unwrap();
    assert_eq!(options.y_range, [-2.0, 3.0, 0.75]);
    assert_eq!(options.x_length, 2.0);
    assert_eq!(options.y_length, 4.0);

    let explicit = ManimBarChartOptions::manim_defaults(
        vec![1.0, 2.0],
        Some(&[-1.0, 5.0]),
        Some(7.0),
        Some(2.0),
    )
    .unwrap();
    assert_eq!(explicit.y_range, [-1.0, 5.0, 1.0]);
}

#[test]
fn copied_chart_reconstructs_semantics_from_its_family() {
    let mut scene = Scene::new();
    let chart = scene.bar_chart(&options()).unwrap();
    let copied = chart.family().copy_family().unwrap();
    let reconstructed = ManimBarChart::from_family(copied.root().clone()).unwrap();
    assert_eq!(reconstructed.values().unwrap(), vec![-2.0, 0.0, 3.0]);
    assert_ne!(reconstructed.family().node_id(), chart.family().node_id());
}
#[test]
fn updates_use_authored_values_and_restore_constructor_palette() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    let node = direct_bar_nodes(chart.bars()).unwrap()[0];
    let mut bar = crate::Mobject::from_node(Rc::clone(scene.integration_store()), node).unwrap();
    let original = bar.state().unwrap();
    bar.shift(1.0, 2.0).unwrap();
    bar.set_color(1.0, 0.0, 0.0, 1.0).unwrap();
    assert_eq!(chart.values().unwrap(), vec![-2.0, 0.0, 3.0]);
    let edge = bar.layout_bounds().unwrap().unwrap().max_y;
    chart.change_bar_values(&mut scene, &[4.0], true).unwrap();
    let after = bar.state().unwrap();
    let bounds = bar.layout_bounds().unwrap().unwrap();
    assert!((bounds.min_y - edge).abs() < 1e-8);
    assert!((bounds.height() - 2.0).abs() < 1e-8);
    assert_eq!(after.content, original.content);
    assert_eq!(after.style.fill, original.style.fill);
    assert_eq!(chart.values().unwrap(), vec![4.0, 0.0, 3.0]);
    let copy = chart.family().copy_family().unwrap();
    let copied = ManimBarChart::from_family(copy.root().clone()).unwrap();
    assert_eq!(copied.values().unwrap(), chart.values().unwrap());
}
#[test]
fn rotated_value_change_stretches_current_world_height_locally() {
    let mut scene = Scene::new();
    let mut chart = scene.bar_chart(&options()).unwrap();
    chart
        .family()
        .rotate(0.37, crate::ManimRotationPivot::Point(0.0, 0.0))
        .unwrap();
    let nodes = direct_bar_nodes(chart.bars()).unwrap();
    let bar = crate::Mobject::from_node(Rc::clone(scene.integration_store()), nodes[0]).unwrap();
    let before = bar.layout_bounds().unwrap().unwrap();
    let unchanged = scene
        .integration_store()
        .borrow()
        .semantic_object_state_checked(nodes[2])
        .unwrap()
        .clone();
    let resources = scene
        .integration_store()
        .borrow()
        .geometry_resources()
        .len();
    chart.change_bar_values(&mut scene, &[4.0], false).unwrap();
    let after = bar.layout_bounds().unwrap().unwrap();
    assert!((after.width() - before.width()).abs() < 1e-6);
    assert!((after.height() - before.height() * 2.0).abs() < 1e-6);
    assert!((after.min_y - before.max_y).abs() < 1e-6);
    let store = scene.integration_store().borrow();
    assert_eq!(store.geometry_resources().len(), resources + 1);
    assert_eq!(
        store.semantic_object_state_checked(nodes[2]).unwrap(),
        &unchanged
    );
}

#[cfg(all(test, feature = "native-text", feature = "latex"))]
mod labeled_tests {
    use super::*;

    struct FailingCompiler;

    impl crate::LatexBackend for FailingCompiler {
        fn identity(&self) -> &str {
            "bar-chart-label-rollback"
        }
        fn format(&self) -> crate::LatexFormat {
            crate::LatexFormat::Preloaded
        }
        fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
            Err("compiler unavailable".into())
        }
        fn font(&mut self, _: &str) -> Result<crate::DviFontResource, String> {
            unreachable!()
        }
    }

    #[test]
    fn named_labeled_construction_rolls_back_when_compilation_fails() {
        let scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let before = {
            let store = store.borrow();
            (
                store.len(),
                store.scene_revision(),
                store.text_resources().len(),
                store.geometry_resources().len(),
            )
        };
        let mut options = ManimBarChartOptions::new(vec![1.0], [0.0, 2.0, 1.0], 2.0, 2.0);
        options.bar_names = Some(vec!["one".into()]);
        assert!(ManimBarChart::create_with_axis_labels(
            Rc::clone(&store),
            &options,
            &crate::plot_presentation::NumberLabelOptions::default(),
            &mut FailingCompiler,
        )
        .is_err());
        let store = store.borrow();
        assert_eq!(
            before,
            (
                store.len(),
                store.scene_revision(),
                store.text_resources().len(),
                store.geometry_resources().len()
            )
        );
    }

    #[test]
    fn bare_constructor_rejects_names_before_publication() {
        let mut scene = Scene::new();
        let before = scene.integration_store().borrow().len();
        let mut options = ManimBarChartOptions::new(vec![1.0], [0.0, 2.0, 1.0], 2.0, 2.0);
        options.bar_names = Some(vec!["one".into()]);
        assert!(scene.bar_chart(&options).is_err());
        assert_eq!(scene.integration_store().borrow().len(), before);
    }
}

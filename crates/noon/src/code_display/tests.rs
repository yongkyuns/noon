use super::*;
use noon_core::{SemanticObjectState, StoredGeometry, TextResource};

fn plain() -> CodeOptions {
    CodeOptions {
        language: Some("text".into()),
        ..Default::default()
    }
}

fn resource(object: &Mobject) -> (SemanticObjectState, TextResource) {
    let state = object.state().unwrap();
    let resource = object
        .integration_store()
        .borrow()
        .text_resources()
        .get(state.content.text().expect("retained code text"))
        .unwrap()
        .clone();
    (state, resource)
}

fn sources(code: &Code) -> Vec<String> {
    code.line_sources()
        .unwrap()
        .iter()
        .map(|source| source.to_string())
        .collect()
}

fn members(family: &MobjectFamily) -> Vec<Mobject> {
    let store = family.integration_store();
    let ids = store
        .borrow()
        .semantic_family_members_checked(family.node_id())
        .unwrap();
    ids.into_iter()
        .map(|id| Mobject::from_node(Rc::clone(store), id).unwrap())
        .collect()
}

fn counts(scene: &Scene) -> (usize, usize, usize, usize) {
    let store = scene.integration_store().borrow();
    (
        store.len(),
        store.text_resources().len(),
        store.font_resources().len(),
        store.geometry_resources().len(),
    )
}

fn baseline(object: &Mobject) -> f64 {
    let (state, text) = resource(object);
    state.transform.translation.y + text.baseline * state.transform.scale.y
}

fn glyph_x(object: &Mobject, source_offset: usize) -> f64 {
    let (state, text) = resource(object);
    let (run, glyph) = text
        .runs
        .iter()
        .find_map(|run| {
            run.glyphs
                .iter()
                .find(|glyph| glyph.cluster.source_span.start as usize == source_offset)
                .map(|glyph| (run, glyph))
        })
        .expect("the requested source column has a glyph");
    state.transform.translation.x
        + run.transform.transform_point(glyph.origin).x * state.transform.scale.x
}

fn logical_right(object: &Mobject) -> f64 {
    let (state, text) = resource(object);
    let advance: f64 = text
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.advance.x)
        .sum();
    state.transform.translation.x
        + (text.runs.first().map_or(0.0, |run| run.transform.tx) + advance)
            * state.transform.scale.x
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-6,
        "expected {expected}, observed {actual}"
    );
}

#[test]
fn tabs_expand_to_columns_and_every_source_line_survives() {
    assert_eq!(
        normalize_source("\r\n  x\ty\r\t\tx\n\n", 4).unwrap(),
        "\n  x y\n        x\n\n"
    );
    assert_eq!(normalize_source("é\tx", 4).unwrap(), "é   x");
    assert_eq!(normalize_source("a\tb\t", 0).unwrap(), "ab");

    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "\n\tx\n  \n", plain()).unwrap();
    assert_eq!(sources(&code), ["", "    x", "  ", ""]);
    assert!(matches!(
        code.line(4),
        Err(CodeAuthoringError::LineOutOfRange { index: 4, count: 4 })
    ));
}

#[test]
fn nested_indentation_moves_glyphs_by_full_monospace_columns() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "x\n    x\n\t\tx", plain()).unwrap();
    let (_, text) = resource(&code.line(1).unwrap());
    let space_advance = text.runs[0].glyphs[0].advance.x / 72.0;
    assert!(space_advance > 0.0);
    let first = glyph_x(&code.line(0).unwrap(), 0);
    close(
        glyph_x(&code.line(1).unwrap(), 4) - first,
        space_advance * 4.0,
    );
    close(
        glyph_x(&code.line(2).unwrap(), 8) - first,
        space_advance * 8.0,
    );
}

#[test]
fn blank_lines_reserve_baselines_without_fake_visible_glyphs() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "\nAAAA\n\npppp\n", plain()).unwrap();
    let lines: Vec<_> = (0..5).map(|index| code.line(index).unwrap()).collect();
    for pair in lines.windows(2) {
        close(baseline(&pair[0]) - baseline(&pair[1]), 0.375);
    }
    for index in [0, 2, 4] {
        let (_, text) = resource(&lines[index]);
        assert_eq!(text.source.as_ref(), "");
        assert_eq!(text.glyph_count(), 0);
        assert!(text.vector_items.is_empty());
    }
    let panel = code.background().layout_bounds().unwrap().unwrap();
    assert!(panel.max_y > baseline(&lines[0]));
    assert!(panel.min_y < baseline(&lines[4]));
}

#[test]
fn line_numbers_share_baselines_and_align_their_right_edge() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "AA\npp\n",
        CodeOptions {
            line_numbers_from: 9,
            ..plain()
        },
    )
    .unwrap();
    assert_eq!(
        code.line_number_sources()
            .unwrap()
            .iter()
            .map(|source| source.as_ref())
            .collect::<Vec<_>>(),
        ["9", "10", "11"]
    );
    let numbers = members(code.line_numbers().unwrap());
    for (index, number) in numbers.iter().enumerate() {
        close(baseline(number), baseline(&code.line(index).unwrap()));
        close(logical_right(number), logical_right(&numbers[0]));
    }
}

#[test]
fn numbers_are_optional_and_window_buttons_are_ordinary_geometry() {
    let mut scene = Scene::new();
    let numbered = Code::new(&mut scene, "x", plain()).unwrap();
    let bare = Code::new(
        &mut scene,
        "x",
        CodeOptions {
            add_line_numbers: false,
            ..plain()
        },
    )
    .unwrap();
    assert!(bare.line_numbers().is_none());
    assert!(bare.line_number_sources().unwrap().is_empty());
    let numbered_bounds = numbered.family().layout_bounds().unwrap().unwrap();
    let bare_bounds = bare.family().layout_bounds().unwrap().unwrap();
    assert!(
        numbered_bounds.max_x - numbered_bounds.min_x
            > bare_bounds.max_x - bare_bounds.min_x + 0.25
    );
    let window = Code::new(
        &mut scene,
        "x",
        CodeOptions {
            background: CodeBackground::Window,
            ..plain()
        },
    )
    .unwrap();
    let background = members(window.background());
    assert_eq!(background.len(), 4);
    for (index, color) in [0xff5f56, 0xffbd2e, 0x27c93f].into_iter().enumerate() {
        let state = background[index + 1].state().unwrap();
        assert_eq!(
            state.content.geometry(),
            Some(StoredGeometry::Circle { radius: 0.1 })
        );
        assert_eq!(
            state.style.fill,
            Some(noon_core::SemanticPaint::Solid(Color::from_hex(color)))
        );
    }
}

#[test]
fn zero_buff_window_keeps_its_buttons_inside_the_native_panel() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "",
        CodeOptions {
            add_line_numbers: false,
            background: CodeBackground::Window,
            background_config: CodeBackgroundConfig {
                buff: 0.0,
                corner_radius: 0.0,
                ..Default::default()
            },
            ..plain()
        },
    )
    .unwrap();
    let background = members(code.background());
    let panel = background[0].layout_bounds().unwrap().unwrap();
    for button in &background[1..] {
        let bounds = button.layout_bounds().unwrap().unwrap();
        assert!(panel.min_x <= bounds.min_x + 1.0e-6);
        assert!(panel.max_x >= bounds.max_x - 1.0e-6);
        assert!(panel.min_y <= bounds.min_y + 1.0e-6);
        assert!(panel.max_y >= bounds.max_y - 1.0e-6);
    }
}

#[test]
fn syntax_colors_reach_retained_text_and_utf8_selection() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "def café():\n    return \"é\" # greeting",
        CodeOptions {
            language: Some("python".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let first = code.line(0).unwrap();
    let (_, text) = resource(&first);
    assert_eq!(text.source.as_ref(), "def café():");
    let colors: Vec<_> = text.runs.iter().filter_map(|run| run.fill).collect();
    assert!(colors.windows(2).any(|pair| pair[0] != pair[1]));
    let part = first.text_source_parts_for("café").unwrap();
    assert_eq!(part.len(), 1);
    assert_eq!(part[0].source_span, noon_core::TextSourceSpan::new(4, 9));
    assert!(part[0].cluster_count >= 4);
}

#[test]
fn native_file_input_infers_its_language_and_io_failure_publishes_nothing() {
    let path = std::env::temp_dir().join(format!("noon-code-input-{}.py", std::process::id()));
    let source = "def café():\n    return 2";
    std::fs::write(&path, source).unwrap();
    let mut scene = Scene::new();
    let code = Code::from_file(&mut scene, &path, CodeOptions::default()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(sources(&code), ["def café():", "    return 2"]);
    let (_, text) = resource(&code.line(0).unwrap());
    assert!(text
        .runs
        .windows(2)
        .any(|pair| pair[0].fill != pair[1].fill));
    let before = counts(&scene);
    let revision = scene.revision();
    assert!(matches!(
        Code::from_file(&mut scene, &path, CodeOptions::default()),
        Err(CodeAuthoringError::Io(_))
    ));
    assert_eq!(counts(&scene), before);
    assert_eq!(scene.revision(), revision);
}

#[test]
fn invalid_construction_preserves_scene_and_all_resource_arenas() {
    let mut scene = Scene::new();
    let unrelated = scene.circle(0.5).unwrap();
    scene.add(&unrelated).unwrap();
    let state = unrelated.state().unwrap();
    let revision = scene.revision();
    let before = counts(&scene);
    let invalid = [
        CodeOptions {
            tab_width: usize::MAX,
            ..plain()
        },
        CodeOptions {
            language: Some("noon-missing-language".into()),
            ..plain()
        },
        CodeOptions {
            formatter_style: "noon-missing-style".into(),
            ..plain()
        },
        CodeOptions {
            paragraph_config: CodeParagraphConfig {
                font: "Noon Missing Font".into(),
                ..Default::default()
            },
            ..plain()
        },
        CodeOptions {
            paragraph_config: CodeParagraphConfig {
                font_size: f32::NAN,
                ..Default::default()
            },
            ..plain()
        },
        CodeOptions {
            background_config: CodeBackgroundConfig {
                fill_opacity: 2.0,
                ..Default::default()
            },
            ..plain()
        },
    ];
    for options in invalid {
        assert!(Code::new(&mut scene, "def untouched():\n    pass", options).is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(counts(&scene), before);
        assert_eq!(unrelated.state().unwrap(), state);
    }
}

#[test]
fn copied_code_rehydrates_independent_nodes_with_shared_text_resources() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\n    two", plain()).unwrap();
    let copied = code.family().copy_family().unwrap();
    let copy = Code::from_family(copied.root().clone()).unwrap();
    assert_eq!(sources(&copy), sources(&code));
    assert_ne!(copy.family().node_id(), code.family().node_id());
    for index in 0..2 {
        let original = code.line(index).unwrap();
        let cloned = copy.line(index).unwrap();
        assert_ne!(original.node_id(), cloned.node_id());
        assert_eq!(
            original.state().unwrap().content,
            cloned.state().unwrap().content
        );
    }
}

#[test]
fn ordinary_family_transforms_reuse_text_and_leave_unrelated_state_unchanged() {
    let mut scene = Scene::new();
    let unrelated = scene.circle(0.5).unwrap();
    let unrelated_state = unrelated.state().unwrap();
    let code = Code::new(&mut scene, "one\n    two", plain()).unwrap();
    let handles: Vec<_> = (0..2)
        .map(|index| code.line(index).unwrap().state().unwrap().content)
        .collect();
    let before = counts(&scene);
    code.family().shift(1.0, -2.0).unwrap();
    code.family().scale(1.5, 1.5).unwrap();
    code.family().set_opacity(0.5).unwrap();
    assert_eq!(counts(&scene), before);
    assert_eq!(unrelated.state().unwrap(), unrelated_state);
    for (index, handle) in handles.into_iter().enumerate() {
        assert_eq!(code.line(index).unwrap().state().unwrap().content, handle);
    }
    scene
        .add_many(&[code.family().into(), (&unrelated).into()])
        .unwrap();
    let session = scene.execution_session().unwrap();
    assert!(session
        .execution_object_id(code.line(0).unwrap().node_id())
        .is_some());
}

#[test]
fn stale_live_creation_rolls_back_resources_then_valid_creation_stays_detached() {
    let mut scene = Scene::new();
    let mut session = scene.execution_session().unwrap();
    scene.circle(0.1).unwrap();
    let revision = scene.revision();
    let before = counts(&scene);
    let mut live = crate::LiveSession::new(scene.integration_store(), scene.root(), &mut session);
    assert!(Code::in_live_session(&mut live, "one\n    two", plain()).is_err());
    assert_eq!(scene.revision(), revision);
    assert_eq!(counts(&scene), before);

    let mut session = scene.execution_session().unwrap();
    let code =
        Code::in_live_session(&mut scene.live(&mut session), "one\n    two", plain()).unwrap();
    let line = code.line(0).unwrap();
    assert!(session.execution_object_id(line.node_id()).is_none());
    scene
        .live(&mut session)
        .add_many(&[code.family().into()])
        .unwrap();
    assert!(session.execution_object_id(line.node_id()).is_some());
}

#[test]
fn zero_buff_panel_contains_custom_font_ink_overhang() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "jf",
        CodeOptions {
            add_line_numbers: false,
            background_config: CodeBackgroundConfig {
                buff: 0.0,
                corner_radius: 0.0,
                ..Default::default()
            },
            paragraph_config: CodeParagraphConfig {
                font: "Libertinus Serif".into(),
                ..Default::default()
            },
            ..plain()
        },
    )
    .unwrap();
    let panel = code.background().layout_bounds().unwrap().unwrap();
    let ink = code.line(0).unwrap().layout_bounds().unwrap().unwrap();
    assert!(panel.min_x <= ink.min_x + 1.0e-6);
    assert!(panel.max_x >= ink.max_x - 1.0e-6);
    assert!(panel.min_y <= ink.min_y + 1.0e-6);
    assert!(panel.max_y >= ink.max_y - 1.0e-6);
}

fn unpadded_highlight() -> CodeHighlightOptions {
    CodeHighlightOptions {
        padding: 0.0,
        ..Default::default()
    }
}

#[test]
fn logical_regions_cover_indentation_whitespace_blank_and_trailing_rows() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "\n    abc  \n  \n", plain()).unwrap();
    let before = counts(&scene);
    let regions: Vec<_> = (0..4)
        .map(|index| {
            code.get_highlighted_lines(index..index + 1, unpadded_highlight())
                .unwrap()
                .layout_bounds()
                .unwrap()
                .unwrap()
        })
        .collect();
    let baseline_step = baseline(&code.line(0).unwrap()) - baseline(&code.line(1).unwrap());
    assert!(regions[0].width() > 0.0 && regions[0].height() > 0.0);
    for pair in regions.windows(2) {
        close(pair[0].min_x, pair[1].min_x);
        close(pair[0].max_x, pair[1].max_x);
        close(pair[0].height(), pair[1].height());
        close(pair[0].max_y - pair[1].max_y, baseline_step);
    }
    let indented = code.line(1).unwrap();
    let ink = indented.layout_bounds().unwrap().unwrap();
    assert!(regions[1].min_x < ink.min_x);
    assert!(regions[1].max_x >= logical_right(&indented) - 1.0e-6);
    let range = code
        .get_highlighted_lines(0..4, unpadded_highlight())
        .unwrap()
        .layout_bounds()
        .unwrap()
        .unwrap();
    close(range.max_y, regions[0].max_y);
    close(range.min_y, regions[3].min_y);
    let numbered = code
        .get_highlighted_lines(
            1..2,
            CodeHighlightOptions {
                include_line_numbers: true,
                ..unpadded_highlight()
            },
        )
        .unwrap()
        .layout_bounds()
        .unwrap()
        .unwrap();
    let number_ink = members(code.line_numbers().unwrap())[1]
        .layout_bounds()
        .unwrap()
        .unwrap();
    assert!(numbered.min_x <= number_ink.min_x + 1.0e-6);
    assert!(numbered.min_x < regions[1].min_x);
    close(numbered.max_x, regions[1].max_x);
    close(numbered.min_y, regions[1].min_y);
    assert_eq!(sources(&code), ["", "    abc  ", "  ", ""]);
    let after = counts(&scene);
    assert_eq!((before.1, before.2, before.3), (after.1, after.2, after.3));
    assert!(members(code.highlights()).is_empty());
}

#[test]
fn unnumbered_empty_code_has_a_real_row_without_synthetic_ink() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "",
        CodeOptions {
            add_line_numbers: false,
            ..plain()
        },
    )
    .unwrap();
    let body = code
        .get_highlighted_lines(0..1, unpadded_highlight())
        .unwrap();
    let numbered = code
        .get_highlighted_lines(
            0..1,
            CodeHighlightOptions {
                include_line_numbers: true,
                ..unpadded_highlight()
            },
        )
        .unwrap();
    assert_eq!(
        body.state().unwrap().content,
        numbered.state().unwrap().content
    );
    assert_eq!(
        body.state().unwrap().transform,
        numbered.state().unwrap().transform
    );
    let bounds = body.layout_bounds().unwrap().unwrap();
    close(bounds.width(), 0.0);
    assert!(bounds.height() > 0.0);
    assert_eq!(resource(&code.line(0).unwrap()).1.glyph_count(), 0);
}

#[test]
fn invalid_regions_leave_nodes_resources_membership_and_revision_unchanged() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    let before = counts(&scene);
    let revision = scene.revision();
    for (start, end) in [(0, 0), (2, 1), (0, 3), (usize::MAX, usize::MAX)] {
        assert!(matches!(
            code.highlight_lines(start..end, CodeHighlightOptions::default()),
            Err(CodeAuthoringError::InvalidLineRange { .. })
        ));
    }
    for options in [
        CodeHighlightOptions {
            padding: -0.1,
            ..Default::default()
        },
        CodeHighlightOptions {
            padding: f64::NAN,
            ..Default::default()
        },
        CodeHighlightOptions {
            padding: f64::MAX,
            ..Default::default()
        },
        CodeHighlightOptions {
            fill_opacity: f64::INFINITY,
            ..Default::default()
        },
        CodeHighlightOptions {
            fill_opacity: 1.01,
            ..Default::default()
        },
        CodeHighlightOptions {
            color: Color {
                red: f32::NAN,
                ..Color::YELLOW
            },
            ..Default::default()
        },
    ] {
        assert!(code.highlight_lines(0..1, options).is_err());
    }
    assert_eq!(counts(&scene), before);
    assert_eq!(scene.revision(), revision);
    assert!(members(code.highlights()).is_empty());
}

#[test]
fn attached_regions_use_ordinary_painter_order_and_family_opacity() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\n\ntwo", plain()).unwrap();
    let options = CodeHighlightOptions::default();
    let detached = code.get_highlighted_lines(1..2, options).unwrap();
    let attached = code.highlight_lines(1..2, options).unwrap();
    assert_eq!(
        detached.state().unwrap().style,
        attached.state().unwrap().style
    );
    let order = scene
        .integration_store()
        .borrow()
        .ordered_leaf_nodes(code.family().node_id())
        .unwrap();
    let position = |node| {
        order
            .iter()
            .position(|candidate| *candidate == node)
            .unwrap()
    };
    let panel = code.layout().unwrap().nodes().panel;
    assert!(position(panel) < position(attached.node_id()));
    for line in members(code.code_lines())
        .into_iter()
        .chain(members(code.line_numbers().unwrap()))
    {
        assert!(position(attached.node_id()) < position(line.node_id()));
    }
    assert!(!order.contains(&detached.node_id()));
    scene.add_many(&[code.family().into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    live.set_family_opacity(code.family(), 0.0).unwrap();
    assert_eq!(live.effective(&attached).unwrap().fill_opacity(), 0.0);
    // New members retain their requested style, just like other ordinary
    // family additions. There is no inherited Code opacity controller.
    let later = code
        .highlight_lines_in_live_session(&mut live, 0..1, options)
        .unwrap();
    close(live.effective(&later).unwrap().fill_opacity(), 0.2);
    live.set_family_opacity(code.family(), 1.0).unwrap();
    assert_eq!(live.effective(&attached).unwrap().fill_opacity(), 1.0);
    assert_eq!(live.effective(&later).unwrap().fill_opacity(), 1.0);
    close(detached.fill_opacity().unwrap(), 0.2);
}

#[test]
fn regions_follow_panel_affines_and_copies_without_rebuilding_text() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "first\n    second\n", plain()).unwrap();
    let original = code
        .highlighted_state(None, 1..3, unpadded_highlight())
        .unwrap();
    let line_content = code.line(1).unwrap().state().unwrap().content;
    let before = counts(&scene);
    code.family().scale(-1.5, 0.75).unwrap();
    code.family()
        .rotate(0.4, crate::ManimRotationPivot::Point(0.0, 0.0))
        .unwrap();
    code.family().shift(1.25, -0.5).unwrap();
    let current = code.highlight_lines(1..3, unpadded_highlight()).unwrap();
    let layout = code.layout().unwrap();
    let panel =
        Mobject::from_node(Rc::clone(scene.integration_store()), layout.nodes().panel).unwrap();
    let pose = panel.state().unwrap().transform.as_planar().unwrap();
    let expected = pose.transform_xy(
        original.transform.translation.x,
        original.transform.translation.y,
    );
    let current_state = current.state().unwrap();
    close(current_state.transform.translation.x, expected.0);
    close(current_state.transform.translation.y, expected.1);
    assert_eq!(current_state.content, original.content);
    assert_eq!(current_state.transform.scale, pose.scale);
    close(
        current_state.transform.planar_rotation().unwrap(),
        pose.rotation_z,
    );
    assert_eq!(code.line(1).unwrap().state().unwrap().content, line_content);
    let after = counts(&scene);
    assert_eq!((before.1, before.2, before.3), (after.1, after.2, after.3));

    let copied = Code::from_family(code.family().copy_family().unwrap().root().clone()).unwrap();
    let copied_layout = copied.layout().unwrap();
    assert_eq!(copied_layout.metrics(), layout.metrics());
    assert_eq!(copied_layout.panel_geometry(), layout.panel_geometry());
    assert_ne!(copied_layout.nodes().panel, layout.nodes().panel);
    assert_ne!(copied.highlights().node_id(), code.highlights().node_id());
    let source_pose = current.state().unwrap().transform;
    copied.family().shift(3.0, 0.0).unwrap();
    let next = copied.highlight_lines(1..3, unpadded_highlight()).unwrap();
    close(
        next.state().unwrap().transform.translation.x,
        source_pose.translation.x + 3.0,
    );
    assert_eq!(current.state().unwrap().transform, source_pose);
    assert_eq!(members(code.highlights()).len(), 1);
    assert_eq!(members(copied.highlights()).len(), 2);
}

#[test]
fn row_edits_do_not_reflow_slots_and_replaced_panels_stay_invalid_after_copy() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    let expected = code
        .highlighted_state(None, 0..1, unpadded_highlight())
        .unwrap();
    code.line(0).unwrap().shift(4.0, 2.0).unwrap();
    assert_eq!(
        code.highlighted_state(None, 0..1, unpadded_highlight())
            .unwrap(),
        expected
    );
    let panel = code.layout().unwrap().nodes().panel;
    let mut panel = Mobject::from_node(Rc::clone(scene.integration_store()), panel).unwrap();
    let mut replacement = panel.state().unwrap();
    replacement.content = StoredGeometry::Rectangle {
        size: noon_core::Vec2::new(5.0, 2.0),
    }
    .into();
    panel.commit_state(replacement).unwrap();
    let before = counts(&scene);
    assert!(matches!(
        code.highlight_lines(0..1, unpadded_highlight()),
        Err(CodeAuthoringError::UnsupportedLineRegion(_))
    ));
    assert_eq!(counts(&scene), before);
    let copied = Code::from_family(code.family().copy_family().unwrap().root().clone()).unwrap();
    assert!(matches!(
        copied.highlight_lines(0..1, unpadded_highlight()),
        Err(CodeAuthoringError::UnsupportedLineRegion(_))
    ));
}

#[test]
fn baked_panel_deformation_invalidates_regions_and_copied_guards() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    let original_layout = code.layout().unwrap();
    code.background()
        .rotate(0.4, crate::ManimRotationPivot::Center)
        .unwrap();
    code.background().scale(2.0, 1.0).unwrap();
    let panel = Mobject::from_node(
        Rc::clone(scene.integration_store()),
        original_layout.nodes().panel,
    )
    .unwrap();
    assert_ne!(
        panel.state().unwrap().content.geometry(),
        Some(StoredGeometry::Resource(original_layout.panel_geometry()))
    );
    assert_eq!(
        code.layout().unwrap().panel_geometry(),
        original_layout.panel_geometry()
    );
    let before = counts(&scene);
    assert!(matches!(
        code.get_highlighted_lines(0..1, unpadded_highlight()),
        Err(CodeAuthoringError::UnsupportedLineRegion(_))
    ));
    assert_eq!(counts(&scene), before);
    let copy = Code::from_family(code.family().copy_family().unwrap().root().clone()).unwrap();
    assert_eq!(
        copy.layout().unwrap().panel_geometry(),
        original_layout.panel_geometry()
    );
    assert!(matches!(
        copy.get_highlighted_lines(0..1, unpadded_highlight()),
        Err(CodeAuthoringError::UnsupportedLineRegion(_))
    ));
}

#[test]
fn live_regions_observe_unfinished_affines_and_preserve_publication_gates() {
    use noon_core::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    let local = code
        .highlighted_state(None, 0..1, unpadded_highlight())
        .unwrap();
    let panel = Mobject::from_node(
        Rc::clone(scene.integration_store()),
        code.layout().unwrap().nodes().panel,
    )
    .unwrap();
    let mut target = panel.target_editor().unwrap();
    target.set_translation(4.0, -2.0).unwrap();
    target.set_scale(2.0, 0.5).unwrap();
    target.set_rotation(std::f64::consts::FRAC_PI_2).unwrap();
    scene.add_many(&[code.family().into()]).unwrap();
    let animation = scene
        .declare_transform_to(
            &panel,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&animation).unwrap();
    live.advance_segment_to(segment, 1.0).unwrap();
    let (pose, _) = live.effective_content_placement(&panel).unwrap();
    close(pose.translation.x, 2.0);
    close(pose.translation.y, -1.0);
    close(pose.scale.x, 1.5);
    close(pose.scale.y, 0.75);
    close(pose.rotation_z, std::f64::consts::FRAC_PI_4);
    let state = code
        .highlighted_state(Some(&live), 0..1, unpadded_highlight())
        .unwrap();
    let expected = pose.transform_xy(local.transform.translation.x, local.transform.translation.y);
    close(state.transform.translation.x, expected.0);
    close(state.transform.translation.y, expected.1);
    assert_eq!(state.transform.scale, pose.scale);
    assert_ne!(state.transform, local.transform);
    let before = counts(&scene);
    assert!(matches!(
        code.get_highlighted_lines_in_live_session(&mut live, 0..1, unpadded_highlight()),
        Err(CodeAuthoringError::LiveSession(
            LiveSessionError::Publication(
                crate::ExecutionSessionPublicationError::SegmentCompletionPending
            )
        ))
    ));
    assert_eq!(counts(&scene), before);
    live.advance_segment_to(segment, segment.end_time())
        .unwrap();
    live.complete_segment(segment).unwrap();
    let highlight = code
        .highlight_lines_in_live_session(&mut live, 0..1, unpadded_highlight())
        .unwrap();
    assert_eq!(
        live.effective(&highlight).unwrap().transform.rotation,
        std::f32::consts::FRAC_PI_2
    );
}

#[test]
fn live_regions_reject_effective_content_overrides_and_stale_publications() {
    use noon_core::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one", plain()).unwrap();
    let panel = Mobject::from_node(
        Rc::clone(scene.integration_store()),
        code.layout().unwrap().nodes().panel,
    )
    .unwrap();
    let mut session = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut session);
        live.declare_and_activate_create(
            &panel,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
        let before = counts(&scene);
        assert!(matches!(
            code.get_highlighted_lines_in_live_session(&mut live, 0..1, unpadded_highlight()),
            Err(CodeAuthoringError::UnsupportedLineRegion(_))
        ));
        assert_eq!(counts(&scene), before);
    }
    let mut current = scene.execution_session().unwrap();
    code.family().shift(1.0, 0.0).unwrap();
    let before = counts(&scene);
    assert!(matches!(
        code.get_highlighted_lines_in_live_session(
            &mut scene.live(&mut current),
            0..1,
            unpadded_highlight()
        ),
        Err(CodeAuthoringError::LiveSession(
            LiveSessionError::Publication(_)
        ))
    ));
    assert_eq!(counts(&scene), before);
}

#[test]
fn live_regions_reject_temporary_content_leases_without_authored_replacement() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one", plain()).unwrap();
    let layout = code.layout().unwrap();
    let panel =
        Mobject::from_node(Rc::clone(scene.integration_store()), layout.nodes().panel).unwrap();
    scene.add_many(&[code.family().into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let prepared = session
        .prepare_effective_content_replacement(
            panel.node_id(),
            noon_core::ObjectContentRef::Geometry(noon_core::GeometryRef::circle(3.0)),
            None,
            None,
        )
        .unwrap();
    let lease = session
        .commit_effective_content_replacement(prepared)
        .unwrap();
    assert_eq!(
        panel.state().unwrap().content.geometry(),
        Some(StoredGeometry::Resource(layout.panel_geometry()))
    );
    let before = counts(&scene);
    let revision = scene.revision();
    assert!(matches!(
        code.highlight_lines_in_live_session(
            &mut scene.live(&mut session),
            0..1,
            unpadded_highlight(),
        ),
        Err(CodeAuthoringError::UnsupportedLineRegion(_))
    ));
    assert_eq!(counts(&scene), before);
    assert_eq!(scene.revision(), revision);
    assert!(members(code.highlights()).is_empty());
    session.release_effective_content(lease).unwrap();
    code.highlight_lines_in_live_session(&mut scene.live(&mut session), 0..1, unpadded_highlight())
        .unwrap();
}

#[test]
fn live_regions_follow_effective_panel_priority_during_replay() {
    use crate::{AnimationCompositionRequest as Request, TransformToRequest};
    use noon_core::{AnimationOptions, RateFunction};
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", plain()).unwrap();
    code.family().set_z_index(2.0, true).unwrap();
    let panel = Mobject::from_node(
        Rc::clone(scene.integration_store()),
        code.layout().unwrap().nodes().panel,
    )
    .unwrap();
    let target = panel.target_editor().unwrap();
    target.set_z_index(-1.0).unwrap();
    scene.add_many(&[code.family().into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    {
        let mut live = scene.live(&mut session);
        let request = Request::TransformTo(
            TransformToRequest::new(
                &panel,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .method_target(),
        );
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
    }
    assert_eq!(panel.z_index().unwrap(), -1.0);
    session.seek(0.5).unwrap();
    let highlight = {
        let mut live = scene.live(&mut session);
        assert_eq!(live.effective(&panel).unwrap().z_index, 2.0);
        let highlight = code
            .highlight_lines_in_live_session(&mut live, 0..1, unpadded_highlight())
            .unwrap();
        assert_eq!(live.effective(&highlight).unwrap().z_index, 2.0);
        highlight
    };
    let painter_position = |object: &Mobject| {
        let id = session.execution_object_id(object.node_id()).unwrap();
        let slot = session
            .frame()
            .objects
            .iter()
            .position(|row| row.id == id)
            .unwrap();
        session
            .painter_order()
            .iter()
            .position(|index| *index as usize == slot)
            .unwrap()
    };
    assert!(painter_position(&panel) < painter_position(&highlight));
    for line in members(code.code_lines())
        .into_iter()
        .chain(members(code.line_numbers().unwrap()))
    {
        assert!(painter_position(&highlight) < painter_position(&line));
    }
}

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
    state.transform.translation.y + f64::from(text.baseline) * state.transform.scale.y
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
        + f64::from(run.transform.transform_point(glyph.origin).x) * state.transform.scale.x
}

fn logical_right(object: &Mobject) -> f64 {
    let (state, text) = resource(object);
    let advance: f32 = text
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.advance.x)
        .sum();
    state.transform.translation.x
        + f64::from(text.runs.first().map_or(0.0, |run| run.transform.tx) + advance)
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
    let space_advance = text.runs[0].glyphs[0].advance.x as f64 / 72.0;
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

use super::*;
use crate::ManimRotationPivot;
use noon_core::TextResourceLookup;

fn options(reserved: &str) -> CodeOptions {
    CodeOptions {
        language: Some("text".into()),
        reserved_source: Some(reserved.into()),
        ..Default::default()
    }
}

fn text(object: &Mobject) -> TextResource {
    let handle = object.state().unwrap().content.text().unwrap();
    object
        .integration_store()
        .borrow()
        .text_resources()
        .get(handle)
        .unwrap()
        .clone()
}

fn origin(object: &Mobject) -> (f64, f64) {
    let resource = text(object);
    object
        .state()
        .unwrap()
        .transform
        .as_planar()
        .unwrap()
        .transform_xy(
            resource.runs.first().map_or(0.0, |run| run.transform.tx),
            resource.baseline,
        )
}

fn close(left: (f64, f64), right: (f64, f64)) {
    assert!((left.0 - right.0).abs() < 1.0e-8, "{left:?} != {right:?}");
    assert!((left.1 - right.1).abs() < 1.0e-8, "{left:?} != {right:?}");
}

fn snapshot(scene: &Scene) -> (usize, usize, usize, usize, noon_core::SceneRevision) {
    let store = scene.integration_store().borrow();
    (
        store.len(),
        store.text_resources().len(),
        store.font_resources().len(),
        store.geometry_resources().len(),
        scene.revision(),
    )
}

#[test]
fn reservation_measures_without_displaying_or_admitting_placeholder_text() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "x\n  y\n", options("longer declaration\n  y\n")).unwrap();
    let target = Code::new(
        &mut scene,
        "longer declaration\n  y\n",
        CodeOptions {
            language: Some("text".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        code.layout().unwrap().metrics(),
        target.layout().unwrap().metrics()
    );
    assert_eq!(
        code.line_sources()
            .unwrap()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["x", "  y", ""]
    );
    close(
        origin(&code.line(0).unwrap()),
        origin(&target.line(0).unwrap()),
    );
}

#[test]
fn replacement_changes_one_row_and_keeps_allocated_geometry_and_other_identities() {
    let mut scene = Scene::new();
    let unrelated = scene.circle(0.4).unwrap();
    let unrelated_state = unrelated.state().unwrap();
    let code = Code::new(
        &mut scene,
        "short\n  untouched\n",
        options("much longer declaration\n  untouched\n"),
    )
    .unwrap();
    let focus = code
        .highlight_lines(0..1, CodeHighlightOptions::default())
        .unwrap();
    let focus_state = focus.state().unwrap();
    let first = code.line(0).unwrap();
    let first_state = first.state().unwrap();
    let before_origin = origin(&first);
    let other = code.line(1).unwrap();
    let other_state = other.state().unwrap();
    let layout = code.layout().unwrap();
    let family = code.family().node_id();
    let edit = code.replace_line(0, "much longer declaration").unwrap();
    assert_eq!(edit.previous.node_id(), first.node_id());
    assert_ne!(edit.previous.node_id(), edit.current.node_id());
    assert_eq!(edit.previous.state().unwrap(), first_state);
    assert_eq!(edit.previous.text_source().unwrap().as_ref(), "short");
    assert_eq!(
        code.line_source(0).unwrap().as_ref(),
        "much longer declaration"
    );
    assert_eq!(code.line(0).unwrap().node_id(), edit.current.node_id());
    close(origin(&edit.current), before_origin);
    assert_eq!(code.line(1).unwrap().node_id(), other.node_id());
    assert_eq!(other.state().unwrap(), other_state);
    assert_eq!(code.family().node_id(), family);
    assert_eq!(code.layout().unwrap(), layout);
    assert_eq!(focus.state().unwrap(), focus_state);
    assert_eq!(unrelated.state().unwrap(), unrelated_state);
    let repeat = code.replace_line(0, "much longer declaration").unwrap();
    assert_eq!(repeat.previous.node_id(), edit.current.node_id());
    assert_ne!(repeat.current.node_id(), repeat.previous.node_id());
    assert_eq!(text(&repeat.previous), text(&repeat.current));
}

#[test]
fn replacement_preserves_previous_rows_other_family_memberships() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "one\ntwo", options("longer\ntwo")).unwrap();
    let previous = code.line(0).unwrap();
    let alias = scene.family(&[(&previous).into()]).unwrap();
    let edit = code.replace_line(0, "longer").unwrap();
    let store = scene.integration_store().borrow();
    assert_eq!(
        store
            .semantic_family_member_at_checked(alias.node_id(), 0)
            .unwrap(),
        Some(previous.node_id())
    );
    assert_eq!(
        store
            .semantic_family_member_at_checked(code.code_lines().node_id(), 0)
            .unwrap(),
        Some(edit.current.node_id())
    );
    assert_eq!(previous.text_source().unwrap().as_ref(), "one");
}

#[test]
fn middle_and_last_replacements_keep_source_order_and_zero_scale() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "first\nmiddle\n",
        options("first\nmiddle variant\nlast"),
    )
    .unwrap();
    code.family().scale(0.0, 1.0).unwrap();
    let first = code.line(0).unwrap().node_id();
    let middle = code.replace_line(1, "middle variant").unwrap();
    let last = code.replace_line(2, "last").unwrap();
    assert_eq!(code.line(0).unwrap().node_id(), first);
    assert_eq!(code.line(1).unwrap().node_id(), middle.current.node_id());
    assert_eq!(code.line(2).unwrap().node_id(), last.current.node_id());
    assert_eq!(
        code.line_sources()
            .unwrap()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["first", "middle variant", "last"]
    );
    assert_eq!(middle.current.state().unwrap().transform.scale.x, 0.0);
    assert_eq!(last.current.layout_bounds().unwrap().unwrap().width(), 0.0);
}

#[test]
fn rejected_source_and_reservation_leave_nodes_resources_and_revision_unchanged() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "x\ny", options("longer\ny")).unwrap();
    let before = snapshot(&scene);
    let first = code.line(0).unwrap().node_id();
    for (index, source) in [(0, "this source is much too wide"), (0, "x\ny"), (2, "z")] {
        assert!(code.replace_line(index, source).is_err());
        assert_eq!(snapshot(&scene), before);
        assert_eq!(code.line(0).unwrap().node_id(), first);
        assert_eq!(code.line_source(0).unwrap().as_ref(), "x");
    }
    assert!(Code::new(&mut scene, "one", options("one\ntwo")).is_err());
    assert_eq!(snapshot(&scene), before);
}

#[test]
fn copied_reflected_rotated_frame_preserves_logical_origin_and_source_recipe() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "  x\ny", options("  wider source\ny")).unwrap();
    code.family().scale(-1.3, 0.8).unwrap();
    code.family()
        .rotate(0.3, ManimRotationPivot::Center)
        .unwrap();
    code.family().shift(2.0, -1.0).unwrap();
    code.family().set_opacity(0.4).unwrap();
    let copied = code.family().copy_family().unwrap();
    let copy = Code::from_family(copied.root().clone()).unwrap();
    let first = copy.line(0).unwrap();
    let previous_origin = origin(&first);
    let state = first.state().unwrap();
    let edit = copy.replace_line(0, "  wider source").unwrap();
    close(origin(&edit.current), previous_origin);
    assert_eq!(
        edit.current.state().unwrap().transform.scale,
        state.transform.scale
    );
    assert_eq!(
        edit.current.state().unwrap().transform.orientation,
        state.transform.orientation
    );
    assert_eq!(edit.current.state().unwrap().style, state.style);
    assert_eq!(code.line_source(0).unwrap().as_ref(), "  x");
    assert_eq!(copy.line_source(0).unwrap().as_ref(), "  wider source");
    assert!(Arc::ptr_eq(
        code.layout().unwrap().source_recipe().unwrap(),
        copy.layout().unwrap().source_recipe().unwrap()
    ));
}

#[test]
fn contextual_recoloring_keeps_other_row_layout_glyphs_and_identity() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "/*         \nint tile = 0;\n*/\nint out = tile;",
        CodeOptions {
            language: Some("cpp".into()),
            reserved_source: Some("//         \nint tile = 0;\n*/\nint out = tile;".into()),
            syntax_colors: CodeSyntaxColors {
                comment: Some(Color::GRAY),
                ..Default::default()
            },
            token_colors: [("tile".into(), Color::TEAL), ("out".into(), Color::GREEN)].into(),
            ..Default::default()
        },
    )
    .unwrap();
    let mut affected = code.line(1).unwrap();
    affected.shift(0.3, -0.2).unwrap();
    affected
        .rotate_with_pivot(0.2, ManimRotationPivot::Center)
        .unwrap();
    let prior = affected.state().unwrap();
    let prior_text = text(&affected);
    let untouched = code.line(3).unwrap();
    let untouched_state = untouched.state().unwrap();
    code.replace_line(0, "//         ").unwrap();
    let after = affected.state().unwrap();
    assert_eq!(code.line(1).unwrap().node_id(), affected.node_id());
    assert_eq!(after.transform, prior.transform);
    assert_eq!(after.style, prior.style);
    assert_ne!(after.content, prior.content);
    let after_text = text(&affected);
    assert_eq!(after_text.bounds, prior_text.bounds);
    assert_eq!(after_text.baseline, prior_text.baseline);
    assert_eq!(
        after_text
            .runs
            .iter()
            .flat_map(|run| &*run.glyphs)
            .collect::<Vec<_>>(),
        prior_text
            .runs
            .iter()
            .flat_map(|run| &*run.glyphs)
            .collect::<Vec<_>>()
    );
    assert!(after_text
        .runs
        .iter()
        .any(|run| run.fill == Some(Color::TEAL)));
    assert_eq!(untouched.state().unwrap(), untouched_state);
}

#[test]
fn inferred_language_survives_replacing_its_shebang() {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        "#!/usr/bin/env python\nvalue = True",
        CodeOptions {
            reserved_source: Some("# a Python listing  \nvalue = True".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let line = code.line(1).unwrap();
    let before = line.state().unwrap();
    assert_eq!(
        code.layout()
            .unwrap()
            .source_recipe()
            .unwrap()
            .language
            .as_deref(),
        Some("Python")
    );
    code.replace_line(0, "# a Python listing").unwrap();
    assert_eq!(line.state().unwrap(), before);
}

#[test]
fn live_edit_preserves_unrelated_execution_slot_and_rolls_back_pending_segment() {
    let mut scene = Scene::new();
    let code = Code::new(&mut scene, "x\ny", options("longer\ny")).unwrap();
    let unrelated = scene.circle(0.4).unwrap();
    scene
        .add_many(&[code.family().into(), (&unrelated).into()])
        .unwrap();
    let other = code.line(1).unwrap();
    let mut session = scene.execution_session().unwrap();
    let other_slot = session.execution_object_id(other.node_id());
    let unrelated_slot = session.execution_object_id(unrelated.node_id());
    {
        let mut live = scene.live(&mut session);
        let segment = live.wait_segment(0.5).unwrap();
        let before = snapshot(&scene);
        assert!(code
            .replace_line_in_live_session(&mut live, 0, "longer")
            .is_err());
        assert_eq!(snapshot(&scene), before);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        let edit = code
            .replace_line_in_live_session(&mut live, 0, "longer")
            .unwrap();
        assert_eq!(edit.current.text_source().unwrap().as_ref(), "longer");
    }
    assert_eq!(session.execution_object_id(other.node_id()), other_slot);
    assert_eq!(
        session.execution_object_id(unrelated.node_id()),
        unrelated_slot
    );
}

#[test]
fn pending_text_can_enter_an_existing_family_with_its_complete_resource_closure() {
    let mut scene = Scene::new();
    let detached = scene.text(crate::Text::new("pending text")).unwrap();
    let content = detached.state().unwrap().content;
    let family = scene.family(&[]).unwrap();
    let unrelated = scene.circle(0.4).unwrap();
    scene
        .add_many(&[(&family).into(), (&unrelated).into()])
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let unrelated_slot = session.execution_object_id(unrelated.node_id());
    let publication = session.publication_context();
    session.take_frame_changes();
    let mut transaction = SemanticMutationTransaction::new();
    let pending = transaction.create_node(SemanticNodeCreation::object(detached.state().unwrap()));
    transaction.add_member(family.node_id(), pending);
    let result = scene.live(&mut session).apply(transaction).unwrap();
    let node = result.resolve(pending).unwrap();
    assert_ne!(node, detached.node_id());
    assert!(session.execution_object_id(node).is_some());
    assert_eq!(
        session.execution_object_id(unrelated.node_id()),
        unrelated_slot
    );
    assert_eq!(session.execution_object_id(detached.node_id()), None);
    assert!(session
        .text_resources()
        .get(content.text().unwrap())
        .is_some());
    assert_ne!(session.publication_context(), publication);
    assert_eq!(session.take_frame_changes().added_indices().len(), 1);
}

#[test]
fn pending_text_with_foreign_resources_cannot_publish_membership_or_sibling_edits() {
    let mut scene = Scene::new();
    let unrelated = scene.circle(0.4).unwrap();
    scene.add(&unrelated).unwrap();
    let foreign = Scene::new()
        .text(crate::Text::new("foreign resource"))
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    session.take_frame_changes();
    let before = snapshot(&scene);
    let old_state = unrelated.state().unwrap();
    let old_frame = session.frame().clone();
    let publication = session.publication_context();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(
        unrelated.node_id(),
        noon_core::SemanticObjectProperty::Translation,
        noon_core::SemanticVec3::new(7.0, 3.0, 0.0),
    );
    let pending = transaction.create_node(SemanticNodeCreation::object(foreign.state().unwrap()));
    transaction.add_member(scene.root(), pending);
    assert!(scene.live(&mut session).apply(transaction).is_err());
    assert_eq!(snapshot(&scene), before);
    assert_eq!(unrelated.state().unwrap(), old_state);
    assert_eq!(session.frame(), &old_frame);
    assert_eq!(session.publication_context(), publication);
    assert!(session.take_frame_changes().is_empty());
}

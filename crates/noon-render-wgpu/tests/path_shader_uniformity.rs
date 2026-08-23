#[test]
fn reveal_derivative_runs_before_reveal_control_flow() {
    let shader = include_str!("../src/path.wgsl");
    let derivative = shader
        .find("let edge = max(fwidth(input.path_progress)")
        .expect("path shader must evaluate a reveal derivative");
    let hidden_branch = shader
        .find("if input.reveal <= 0.0")
        .expect("path shader must handle a fully hidden reveal");
    let complete_branch = shader
        .find("if input.reveal >= 1.0")
        .expect("path shader must handle a complete reveal");

    assert!(
        derivative < hidden_branch && derivative < complete_branch,
        "fragment derivatives must execute before reveal-dependent control flow"
    );
}

#[test]
fn fill_only_partial_reveal_derives_a_visible_outline() {
    let shader = include_str!("../src/path.wgsl");
    assert!(shader.contains(
        "let derive_creation_stroke = reveal < 1.0 && input.flags.x != 0u && input.flags.y == 0u;"
    ));
    assert!(shader.contains(
        "let enabled = select(authored_enabled, is_stroke, derive_creation_stroke);"
    ));
    assert!(shader.contains(
        "let color = select(authored_color, input.fill, derive_creation_stroke);"
    ));
}

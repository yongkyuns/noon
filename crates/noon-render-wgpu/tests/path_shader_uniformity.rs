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

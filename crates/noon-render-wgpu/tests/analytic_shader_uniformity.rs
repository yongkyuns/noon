const SHADER: &str = include_str!("../src/analytic.wgsl");

fn shader_function(name: &str) -> &'static str {
    let start = SHADER
        .find(&format!("fn {name}("))
        .expect("shader function");
    let function = &SHADER[start..];
    &function[..function.find("\n}").expect("function end")]
}

#[test]
fn circle_fill_derivative_runs_before_reveal_control_flow() {
    let circle = shader_function("fs_circle");
    let fill_coverage = circle
        .find("let fill_segment_coverage = fill_coverage * inside_coverage(fill_chord_side);")
        .expect("circle fill coverage must derive chord-edge antialiasing");

    for branch in ["if reveal >= 1.0", "if reveal <= 0.0"] {
        let branch = circle.find(branch).expect("circle reveal branch");
        assert!(
            fill_coverage < branch,
            "circle fill derivatives must run before reveal control flow"
        );
    }
}

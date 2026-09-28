const SHADER: &str = include_str!("../src/path.wgsl");

fn shader_function(name: &str) -> &'static str {
    let start = SHADER
        .find(&format!("fn {name}("))
        .expect("shader function");
    let function = &SHADER[start..];
    &function[..function.find("\n}").expect("function end")]
}

#[test]
fn reveal_derivative_runs_before_reveal_control_flow() {
    for entry in ["fs_path", "fs_path_compact"] {
        let fragment = shader_function(entry);
        let derivative = fragment
            .find("let edge = max(fwidth(input.path_progress)")
            .expect("path fragment must evaluate a reveal derivative");
        let reveal = fragment
            .find("revealed_path_color(")
            .expect("path fragment must apply shared reveal coverage");
        assert!(
            derivative < reveal,
            "derive coverage before reveal control flow"
        );
        if let Some(branch) = fragment.find("if ") {
            assert!(
                derivative < branch,
                "derive coverage before fragment branches"
            );
        }
    }
    let reveal = shader_function("revealed_path_color");
    assert!(
        !reveal.contains("fwidth("),
        "the conditional helper takes a precomputed derivative"
    );
    assert!(reveal.contains("if reveal <= 0.0"));
    assert!(reveal.contains("if reveal >= 1.0"));
}

#[test]
fn fill_only_partial_reveal_derives_a_visible_outline() {
    assert!(SHADER
        .contains("let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;"));
    assert!(
        SHADER.contains("let enabled = authored_enabled || (is_stroke && derive_creation_stroke);")
    );
    assert!(SHADER.contains("creation_outline_alpha = 1.0 - smoothstep(0.75, 1.0, reveal);"));
}

#[test]
fn partial_reveal_smoothly_fades_fill_instead_of_waiting_for_completion() {
    let reveal = shader_function("revealed_path_color");
    assert!(reveal.contains("if is_stroke < 0.5"));
    assert!(reveal.contains("return color * smoothstep(0.0, 1.0, reveal);"));
}

//! Allocated source rows, including a blank row, highlighted in a transformed Code.

use crate::{
    Code, CodeBackground, CodeHighlightOptions, CodeOptions, CodeParagraphConfig, ExecutionSession,
    ManimRotationPivot, Scene,
};

pub const SOURCE: &str =
    "__global__ void copy(float *out) {\n    int x = threadIdx.x;\n\n    out[x] = x;\n}";

/// The same scene without decorations is an independent raster control.
pub fn scene(highlight: bool) -> Result<Scene, String> {
    let mut scene = Scene::new();
    let code = Code::new(
        &mut scene,
        SOURCE,
        CodeOptions {
            language: Some("cuda".into()),
            formatter_style: "monokai".into(),
            background: CodeBackground::Window,
            line_numbers_from: 9,
            paragraph_config: CodeParagraphConfig {
                font_size: 20.0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    code.family()
        .scale(1.2, 1.2)
        .map_err(|error| error.to_string())?;
    code.family()
        .rotate(0.08, ManimRotationPivot::Center)
        .map_err(|error| error.to_string())?;
    if highlight {
        code.highlight_lines(2..3, CodeHighlightOptions::default())
            .map_err(|error| error.to_string())?;
        code.highlight_lines(
            3..5,
            CodeHighlightOptions {
                include_line_numbers: true,
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())?;
    }
    scene
        .add_many(&[code.family().into()])
        .map_err(|error| error.to_string())?;
    Ok(scene)
}

pub fn session(highlight: bool) -> Result<ExecutionSession, String> {
    scene(highlight)?
        .execution_session()
        .map_err(|error| error.to_string())
}

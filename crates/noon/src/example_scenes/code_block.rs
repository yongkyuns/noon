//! Native highlighted source listings shared by Rust and Python examples.

use crate::{
    Code, CodeBackground, CodeOptions, CodeParagraphConfig, ExecutionSession, Scene, Text,
};

pub const PYTHON_SOURCE: &str =
    "def squares(values):\n    return [\n        x * x for x in values\n    ]";
pub const CUDA_SOURCE: &str = "__global__ void transpose(float *out) {\n    __shared__ float tile[8][9];\n    // Padding keeps the bank stride coprime.\n    out[threadIdx.x] = tile[0][threadIdx.x];\n}";

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut title = scene
        .text(Text::new("Code is part of the scene").with_font_size(32.0))
        .map_err(|error| error.to_string())?;
    title.shift(0.0, 3.25).map_err(|error| error.to_string())?;
    let python = Code::new(
        &mut scene,
        PYTHON_SOURCE,
        CodeOptions {
            language: Some("python".into()),
            formatter_style: "monokai".into(),
            background: CodeBackground::Window,
            paragraph_config: CodeParagraphConfig {
                font_size: 20.0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    python
        .family()
        .shift(0.0, 1.25)
        .map_err(|error| error.to_string())?;
    let cuda = Code::new(
        &mut scene,
        CUDA_SOURCE,
        CodeOptions {
            language: Some("cuda".into()),
            formatter_style: "emacs".into(),
            paragraph_config: CodeParagraphConfig {
                font_size: 20.0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    cuda.family()
        .shift(0.0, -1.5)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            (&title).into(),
            python.family().into(),
            cuda.family().into(),
        ])
        .map_err(|error| error.to_string())?;
    scene.execution_session().map_err(|error| error.to_string())
}

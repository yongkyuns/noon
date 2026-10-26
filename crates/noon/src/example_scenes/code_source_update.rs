//! A reserved CUDA declaration edit with shared lexical colors and ordinary animation.

use crate::{
    AnimationCompositionRequest, AnimationOptions, Code, CodeBackground, CodeHighlightOptions,
    CodeLineNumberConfig, CodeOptions, CodeParagraphConfig, CodeSyntaxColors, Color,
    ContinuationStep, ExecutionSession, FadeEndpoint, LiveContinuation, LiveProgram, LiveSession,
    ManimRotationPivot, RateFunction, Scene, SemanticAnimationCompositionKind,
    SemanticFadeDirection,
};

pub const DECLARATION: &str = "__shared__ float tile[TILE_DIM][TILE_DIM];";
pub const PADDED_DECLARATION: &str = "__shared__ float tile[TILE_DIM][TILE_DIM + 1];";
pub const SOURCE: &str = concat!(
    "__shared__ float tile[TILE_DIM][TILE_DIM];\n",
    "int x = blockIdx.x * TILE_DIM + threadIdx.x;\n",
    "int y = blockIdx.y * TILE_DIM + threadIdx.y;\n",
    "for (int j = 0; j < TILE_DIM; j += BLOCK_ROWS) {\n",
    "  if (x < cols && y + j < rows) {\n",
    "    tile[threadIdx.y + j][threadIdx.x] =\n",
    "        in[static_cast<size_t>(y + j) * cols + x];\n",
    "  }\n",
    "}\n",
    "__syncthreads();",
);

/// Source, palette associations and choreography are lesson content. Rust Code
/// owns token boundaries, indentation, gutters, reserved extent and row origins.
pub fn listing(scene: &mut Scene) -> Result<Code, String> {
    let code = Code::new(
        scene,
        SOURCE,
        CodeOptions {
            language: Some("cuda".into()),
            formatter_style: "monokai".into(),
            background: CodeBackground::Window,
            reserved_source: Some(SOURCE.replacen(DECLARATION, PADDED_DECLARATION, 1)),
            syntax_colors: CodeSyntaxColors {
                foreground: Some(Color::WHITE),
                comment: Some(Color::GRAY),
                keyword: Some(Color::PURPLE),
                number: Some(Color::GOLD),
                ..Default::default()
            },
            token_colors: [
                ("in", Color::TEAL),
                ("tile", Color::TEAL),
                ("__syncthreads", Color::PINK),
                ("TILE_DIM", Color::GOLD),
                ("BLOCK_ROWS", Color::GOLD),
                ("threadIdx.x", Color::GOLD),
                ("threadIdx.y", Color::GOLD),
                ("blockIdx.x", Color::GOLD),
                ("blockIdx.y", Color::GOLD),
            ]
            .into_iter()
            .map(|(token, color)| (token.into(), color))
            .collect(),
            paragraph_config: CodeParagraphConfig {
                font_size: 20.0,
                ..Default::default()
            },
            line_number_config: CodeLineNumberConfig {
                font_size: Some(11.0),
                color: Some(Color::GRAY),
                min_digits: 2,
            },
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    code.family()
        .scale(1.2, 1.2)
        .map_err(|error| error.to_string())?;
    code.family()
        .rotate(0.04, ManimRotationPivot::Center)
        .map_err(|error| error.to_string())?;
    code.highlight_lines(0..1, CodeHighlightOptions::default())
        .map_err(|error| error.to_string())?;
    Ok(code)
}

/// Independent before/after scenes qualify that the rest of the page is stable.
pub fn scene(updated: bool) -> Result<Scene, String> {
    let mut scene = Scene::new();
    let code = listing(&mut scene)?;
    if updated {
        code.replace_line(0, PADDED_DECLARATION)
            .map_err(|error| error.to_string())?;
    }
    scene
        .add_many(&[code.family().into()])
        .map_err(|error| error.to_string())?;
    Ok(scene)
}

pub fn session(updated: bool) -> Result<ExecutionSession, String> {
    scene(updated)?
        .execution_session()
        .map_err(|error| error.to_string())
}

pub struct CodeSourceUpdate {
    code: Code,
    stage: u8,
}

impl LiveContinuation for CodeSourceUpdate {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 | 2 => {
                self.stage += 1;
                live.wait_segment(0.2)
                    .map(ContinuationStep::Await)
                    .map_err(|error| error.to_string())
            }
            1 => {
                let change = self
                    .code
                    .replace_line_in_live_session(live, 0, PADDED_DECLARATION)
                    .map_err(|error| error.to_string())?;
                live.add(&change.previous)
                    .map_err(|error| error.to_string())?;
                let options = AnimationOptions::new()
                    .run_time(0.75)
                    .rate_func(RateFunction::Linear);
                let segment = live
                    .declare_and_activate_animation_composition(
                        SemanticAnimationCompositionKind::Parallel,
                        &[
                            AnimationCompositionRequest::Fade {
                                target: &change.previous,
                                direction: SemanticFadeDirection::Out,
                                endpoint: FadeEndpoint::default(),
                                options: options.remover(true),
                            },
                            AnimationCompositionRequest::TextWrite {
                                target: &change.current,
                                reverse_member_order: false,
                                options: options.introducer(true),
                            },
                        ],
                        options,
                        AnimationOptions::new(),
                    )
                    .map_err(|error| error.to_string())?;
                self.stage = 2;
                Ok(ContinuationStep::Await(segment))
            }
            3 => {
                self.stage = 4;
                Ok(ContinuationStep::Finished)
            }
            _ => Err("Code source update resumed after completion".into()),
        }
    }
}

pub fn program() -> Result<LiveProgram<CodeSourceUpdate>, String> {
    let mut scene = Scene::new();
    let code = listing(&mut scene)?;
    scene
        .add_many(&[code.family().into()])
        .map_err(|error| error.to_string())?;
    scene
        .into_live_program(CodeSourceUpdate { code, stage: 0 })
        .map_err(|error| error.to_string())
}

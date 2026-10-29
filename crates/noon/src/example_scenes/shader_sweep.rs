//! An editable duotone material: object-local stripes travel with the left panel;
//! scene-space stripes stay fixed behind the right panel. Phase is an ordinary
//! animation channel, with no shader clock or per-frame host callback.
use crate::{
    effects::{
        EffectUpdate, ShaderCoordinates, ShaderDependencies, ShaderEffect,
        ShaderParameterDefinition, ShaderParameterKey, ShaderParameterType, ShaderProfile,
        ShaderSchema, ShaderUnit, ShaderValue, WgslEffectProgram,
    },
    AnimationCompositionRequest, AnimationOptions, Color, ContinuationStep, LiveContinuation,
    LiveProgram, LiveSession, Mobject, RateFunction, Scene, SemanticAnimationCompositionKind,
    TransformToRequest,
};

const SOURCE: &str = "
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let stripes = 0.5 + 0.5 * cos(6.28318530718 * (input.position.y * p.frequency + p.phase));
    return vec4<f32>(input.color.rgb * mix(p.low.rgb, p.high.rgb, stripes), 1.0);
}";

pub struct ShaderSweep {
    panels: [Mobject; 2],
    phase: [ShaderParameterKey; 2],
    stage: u8,
    deferred: Option<[WgslEffectProgram; 2]>,
    retire: bool,
}
fn message(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn material(coordinates: ShaderCoordinates) -> Result<WgslEffectProgram, String> {
    WgslEffectProgram::prepare(
        ShaderSchema {
            profile: ShaderProfile::PaintedPointwiseV1,
            dependencies: ShaderDependencies {
                coordinates,
                authored_time: false,
                seed: false,
            },
            parameters: vec![
                ShaderParameterDefinition {
                    name: "phase".into(),
                    kind: ShaderParameterType::Scalar {
                        range: 0.0..=1.0,
                        unit: ShaderUnit::Unitless,
                    },
                    default: ShaderValue::Scalar(0.0),
                },
                ShaderParameterDefinition {
                    name: "frequency".into(),
                    kind: ShaderParameterType::Scalar {
                        range: 0.25..=8.0,
                        unit: ShaderUnit::Unitless,
                    },
                    default: ShaderValue::Scalar(2.0),
                },
                ShaderParameterDefinition {
                    name: "low".into(),
                    kind: ShaderParameterType::Color,
                    default: ShaderValue::Color(Color::rgb(0.09, 0.2, 0.42)),
                },
                ShaderParameterDefinition {
                    name: "high".into(),
                    kind: ShaderParameterType::Color,
                    default: ShaderValue::Color(Color::rgb(0.48, 0.94, 0.83)),
                },
            ]
            .into_boxed_slice(),
        },
        SOURCE,
    )
    .map_err(message)
}

impl LiveContinuation for ShaderSweep {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let segment = match self.stage {
            0 | 2 | 4 => live.wait_segment(0.5),
            1 | 3 => {
                if let Some(programs) = self.deferred.take() {
                    for (panel, program) in self.panels.iter().zip(programs) {
                        live.add_effect(panel, ShaderEffect::new(program), "stripes")
                            .map_err(message)?;
                    }
                }
                let outward = self.stage == 1;
                let targets = self
                    .panels
                    .each_ref()
                    .map(|panel| live.target_editor(panel));
                let [left, right] = targets.map(|target| target.map_err(message));
                let targets = [left?, right?];
                for (i, target) in targets.iter().enumerate() {
                    live.set_rotation(target, if outward { 0.35 } else { 0.0 })
                        .map_err(message)?;
                    live.set_effect(
                        target,
                        "stripes",
                        EffectUpdate::default().shader_parameter(
                            &self.phase[i],
                            ShaderValue::Scalar(if outward { 0.65 } else { 0.0 }),
                        ),
                    )
                    .map_err(message)?;
                }
                let options = AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Smooth);
                live.declare_and_activate_animation_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.panels[0], &targets[0], options)
                                .method_target(),
                        ),
                        AnimationCompositionRequest::TransformTo(
                            TransformToRequest::new(&self.panels[1], &targets[1], options)
                                .method_target(),
                        ),
                    ],
                    options,
                    AnimationOptions::new(),
                )
            }
            5 if self.retire => {
                for panel in &self.panels {
                    live.remove_effect(panel, "stripes").map_err(message)?;
                }
                live.wait_segment(0.5)
            }
            5 | 6 => return Ok(ContinuationStep::Finished),
            _ => return Err("shader sweep resumed after completion".into()),
        }
        .map_err(message)?;
        self.stage += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn program() -> Result<LiveProgram<ShaderSweep>, String> {
    build_program(false)
}

/// The same lesson with enrollment after its first wait and explicit retirement.
/// Qualifies new immutable programs introduced by an ordinary live continuation.
pub fn late_program() -> Result<LiveProgram<ShaderSweep>, String> {
    build_program(true)
}

fn build_program(late: bool) -> Result<LiveProgram<ShaderSweep>, String> {
    let mut scene = Scene::new();
    let mut background = scene.rectangle(16.0, 9.0).map_err(message)?;
    background
        .set_fill(0.035, 0.045, 0.09, 1.0)
        .map_err(message)?;
    background.disable_stroke().map_err(message)?;
    scene.add(&background).map_err(message)?;
    let programs = [
        material(ShaderCoordinates::LocalNormalized)?,
        material(ShaderCoordinates::Scene)?,
    ];
    let mut panels = Vec::new();
    for (i, program) in programs.iter().enumerate() {
        let mut panel = scene.rectangle(4.8, 2.5).map_err(message)?;
        panel.set_fill(1.0, 1.0, 1.0, 1.0).map_err(message)?;
        panel.disable_stroke().map_err(message)?;
        panel
            .set_translation(if i == 0 { -3.0 } else { 3.0 }, 0.0)
            .map_err(message)?;
        if !late {
            panel
                .add_effect(ShaderEffect::new(program.clone()), "stripes")
                .map_err(message)?;
        }
        scene.add(&panel).map_err(message)?;
        panels.push(panel);
    }
    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    for (text, size, x, y) in [
        ("A MATERIAL THAT MOVES", 36.0, 0.0, 3.1),
        (
            "Animate the controls. Keep the objects editable.",
            20.0,
            0.0,
            2.5,
        ),
        ("LOCAL: follows the object", 18.0, -3.0, -2.0),
        ("SCENE: stays in the world", 18.0, 3.0, -2.0),
    ] {
        let mut label = scene
            .text(
                crate::Text::new(text)
                    .with_font_size(size)
                    .color(Color::rgb(0.85, 0.9, 0.96)),
            )
            .map_err(message)?;
        label.set_translation(x, y).map_err(message)?;
        scene.add(&label).map_err(message)?;
    }
    scene
        .into_live_program(ShaderSweep {
            panels: panels.try_into().map_err(|_| "expected two panels")?,
            phase: programs
                .each_ref()
                .map(|program| program.parameter("phase").expect("declared phase")),
            stage: 0,
            deferred: late.then_some(programs),
            retire: late,
        })
        .map_err(message)
}

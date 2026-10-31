//! External ScanBand fixture for #1897: a checked material, not a built-in effect.
use crate::{
    effects::{
        EffectUpdate, ShaderCoordinates, ShaderDependencies, ShaderEffect,
        ShaderParameterDefinition, ShaderParameterType, ShaderProfile, ShaderSchema, ShaderUnit,
        ShaderValue, WgslEffectProgram,
    },
    AnimationOptions, RateFunction, Scene, SceneExecution,
};

pub const SOURCE: &str = include_str!("scan_band.wgsl");

pub fn material() -> Result<WgslEffectProgram, String> {
    WgslEffectProgram::prepare(
        ShaderSchema {
            profile: ShaderProfile::PaintedPointwiseV1,
            dependencies: ShaderDependencies {
                coordinates: ShaderCoordinates::LocalNormalized,
                authored_time: false,
                seed: false,
            },
            parameters: [
                ShaderParameterDefinition {
                    name: "width".into(),
                    kind: ShaderParameterType::Scalar {
                        range: f64::from(f32::from_bits(1))..=1.0,
                        unit: ShaderUnit::Normalized,
                    },
                    default: ShaderValue::Scalar(0.2),
                },
                ShaderParameterDefinition {
                    name: "phase".into(),
                    kind: ShaderParameterType::Scalar {
                        range: 0.0..=1.0,
                        unit: ShaderUnit::Normalized,
                    },
                    default: ShaderValue::Scalar(0.0),
                },
            ]
            .into(),
        },
        SOURCE,
    )
    .map_err(|error| error.to_string())
}

pub fn execution() -> Result<SceneExecution, String> {
    let program = material()?;
    let mut scene = Scene::new();
    let mut panel = scene.rectangle(4.0, 2.0).map_err(|e| e.to_string())?;
    panel.disable_stroke().map_err(|e| e.to_string())?;
    panel
        .set_fill(1.0, 1.0, 1.0, 0.5)
        .map_err(|e| e.to_string())?;
    panel.set_rotation(0.37).map_err(|e| e.to_string())?;
    panel
        .add_effect(ShaderEffect::new(program.clone()), "scan")
        .map_err(|e| e.to_string())?;
    scene.add(&panel).map_err(|e| e.to_string())?;
    let mut target = panel.target_editor().map_err(|e| e.to_string())?;
    target
        .set_effect(
            "scan",
            EffectUpdate::default().shader_parameter(
                &program.parameter("phase").expect("declared phase"),
                ShaderValue::Scalar(1.0),
            ),
        )
        .map_err(|e| e.to_string())?;
    let animation = scene
        .declare_transform_to(
            &panel,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .map_err(|e| e.to_string())?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    scene
        .live(&mut session)
        .play_animation(&animation)
        .map_err(|e| e.to_string())?;
    scene
        .into_execution_with_session(session)
        .map_err(|e| e.to_string())
}

//! Ordinary authoring and raw execution admission share the same shader schema.
use super::*;
use crate::{AnimationOptions, Color, ExecutionSession, RateFunction};
use noon_compile::{
    CompiledEffect, CompiledEffectStack, CompiledObject, CompiledScene, ExecutionPatch,
};
use noon_core::{
    EffectTrackEndBehavior, EffectTrackValue, GeometryRef, ObjectId, Property, SemanticNodeId,
    ShaderCoordinates, ShaderDependencies, ShaderEffect, ShaderParameterDefinition,
    ShaderParameterType, ShaderProfile, ShaderSchema, ShaderUnit, ShaderValue, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, WgslEffectProgram,
};
use std::sync::Arc;

fn program() -> WgslEffectProgram {
    let scalar = |name: String, value| ShaderParameterDefinition {
        name: name.into(),
        kind: ShaderParameterType::Scalar {
            range: 0.0..=1.0,
            unit: ShaderUnit::Normalized,
        },
        default: ShaderValue::Scalar(value),
    };
    let mut parameters = vec![
        scalar("phase".into(), 0.25),
        ShaderParameterDefinition {
            name: "direction".into(),
            kind: ShaderParameterType::Vector2 {
                min: [-1.0; 2],
                max: [1.0; 2],
                unit: ShaderUnit::Normalized,
            },
            default: ShaderValue::Vector2([-0.5, 0.25]),
        },
        ShaderParameterDefinition {
            name: "tint".into(),
            kind: ShaderParameterType::Color,
            default: ShaderValue::Color(Color::rgba(1.0, 0.0, 0.0, 1.0)),
        },
    ];
    parameters.extend((3..16).map(|i| scalar(format!("field{i}"), 0.125)));
    WgslEffectProgram::prepare(ShaderSchema {
        profile: ShaderProfile::PaintedPointwiseV1,
        dependencies: ShaderDependencies {
            coordinates: ShaderCoordinates::LocalNormalized, authored_time: false, seed: false,
        },
        parameters: parameters.into_boxed_slice(),
    }, "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return input.color * p.tint; }").unwrap()
}

fn effective(session: &ExecutionSession, source: &Mobject) -> ShaderEffect {
    let id = session.execution_object_id(source.node_id()).unwrap();
    only_effect(
        session
            .frame()
            .objects
            .iter()
            .find(|row| row.id == id)
            .unwrap()
            .effects
            .as_ref()
            .unwrap(),
    )
    .definition
    .as_shader()
    .unwrap()
    .clone()
}

#[test]
fn shader_targets_share_ordinary_timing_seek_live_reconciliation_and_completion() {
    let program = program();
    let key = |name| program.parameter(name).unwrap();
    let mut scene = Scene::new();
    let mut source = scene.circle(0.4).unwrap();
    source.disable_stroke().unwrap();
    scene.add(&source).unwrap();
    scene
        .add_effect(
            &source,
            ShaderEffect::new(program.clone())
                .with_parameter(&key("field3"), ShaderValue::Scalar(0.625))
                .unwrap(),
            "scan",
        )
        .unwrap();
    let original = source.get_effect("scan").unwrap();
    let mut target = source.target_editor().unwrap();
    target.shift(2.0, 0.0).unwrap();
    target
        .set_effect(
            "scan",
            EffectUpdate::default()
                .shader_parameter(&key("phase"), ShaderValue::Scalar(0.75))
                .shader_parameter(&key("direction"), ShaderValue::Vector2([0.5, -0.25]))
                .shader_parameter(
                    &key("tint"),
                    ShaderValue::Color(Color::rgba(0.0, 0.0, 1.0, 1.0)),
                )
                .shader_parameter(&key("field15"), ShaderValue::Scalar(0.875)),
        )
        .unwrap();
    assert_ne!(
        target.get_effect("scan").unwrap().node_id(),
        original.node_id()
    );
    let mut conflict_target = source.target_editor().unwrap();
    conflict_target
        .set_effect(
            "scan",
            EffectUpdate::default().shader_parameter(&key("phase"), ShaderValue::Scalar(0.75)),
        )
        .unwrap();
    let options = || {
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear)
    };
    let a = scene
        .declare_transform_to(&source, &conflict_target, options())
        .unwrap();
    let b = scene
        .declare_transform_to(&source, &conflict_target, options())
        .unwrap();
    let conflict = scene
        .declare_animation(
            noon_core::SemanticAnimationIntent::Composition {
                kind: noon_core::SemanticAnimationCompositionKind::Parallel,
                children: vec![a.node_id(), b.node_id()],
            },
            AnimationOptions::new(),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let before = session.frame().clone();
    assert!(scene.live(&mut session).play_animation(&conflict).is_err());
    assert_eq!(session.frame(), &before);
    let segment = scene
        .live(&mut session)
        .declare_and_activate_transform_to(
            &source,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    scene
        .live(&mut session)
        .advance_segment_to(segment, 0.5)
        .unwrap();
    assert!(scene
        .live(&mut session)
        .set_effect(
            &source,
            &original,
            EffectUpdate::default().shader_parameter(&key("field3"), ShaderValue::Scalar(0.875)),
        )
        .is_err());
    for time in [0.75, 0.25, 0.5, 0.5] {
        session.seek(time).unwrap();
        let actual = effective(&session, &source);
        assert_eq!(actual.values()[0], ShaderValue::Scalar(0.25 + 0.5 * time));
        assert_eq!(
            actual.values()[1],
            ShaderValue::Vector2([-0.5 + time, 0.25 - 0.5 * time])
        );
        assert_eq!(
            actual.values()[2],
            ShaderValue::Color(Color::rgba((1.0 - time) as f32, 0.0, time as f32, 1.0))
        );
        assert_eq!(actual.values()[3], ShaderValue::Scalar(0.625));
        assert_eq!(
            actual.values()[15],
            ShaderValue::Scalar(0.125 + 0.75 * time)
        );
        assert!(Arc::ptr_eq(actual.program().module(), program.module()));
    }
    assert_eq!(
        original
            .authored_definition()
            .unwrap()
            .as_shader()
            .unwrap()
            .values()[0],
        ShaderValue::Scalar(0.25)
    );
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    assert_eq!(
        original.authored_definition().unwrap().as_shader().unwrap(),
        &effective(&session, &source)
    );
    scene
        .live(&mut session)
        .set_effect(
            &source,
            &original,
            EffectUpdate::default().shader_parameter(&key("field15"), ShaderValue::Scalar(0.375)),
        )
        .unwrap();
    assert_eq!(
        effective(&session, &source).values()[15],
        ShaderValue::Scalar(0.375)
    );
    scene
        .live(&mut session)
        .remove_effect(&source, &original)
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(&source, ShaderEffect::new(program), "scan")
        .unwrap();
    assert_ne!(
        source.get_effect("scan").unwrap().node_id(),
        original.node_id()
    );
    assert!(scene
        .live(&mut session)
        .set_effect(&source, &original, EffectUpdate::default())
        .is_err());
    assert_eq!(
        effective(&session, &source).values()[15],
        ShaderValue::Scalar(0.125)
    );
}

#[test]
fn shader_raw_channels_reject_invalid_schema_and_retire_exact_generation_atomically() {
    let program = program();
    let binding = CompiledEffect {
        attachment: SemanticNodeId::new(10, 3),
        definition: ShaderEffect::new(program.clone()).into(),
    };
    let object = ObjectId::new(1);
    let mut row = CompiledObject::new(
        object,
        GeometryRef::circle(0.4),
        Transform2D::IDENTITY,
        Style::default(),
    );
    row.effects = Some(Arc::new(
        CompiledEffectStack::new([binding.clone()]).unwrap(),
    ));
    let track = |index: u8, value| TrackDefinition {
        id: TrackId::new(1),
        object,
        property: Property::EffectShader(index),
        values: TrackValues::Effect {
            attachment: binding.attachment,
            from: EffectTrackValue::Shader {
                parameter: index,
                value,
            },
            to: EffectTrackValue::Shader {
                parameter: index,
                value,
            },
            end_behavior: EffectTrackEndBehavior::Sample,
        },
        timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
        time_map: Default::default(),
    };
    for (index, value) in [
        (0, ShaderValue::Scalar(1.01)),
        (0, ShaderValue::Scalar(f64::NAN)),
        (0, ShaderValue::Vector2([0.0, 0.0])),
        (16, ShaderValue::Scalar(0.5)),
        (255, ShaderValue::Scalar(0.5)),
    ] {
        assert!(CompiledScene::compile_objects(vec![row.clone()], &[track(index, value)]).is_err());
    }
    for alpha in [0.0, 0.5, 1.0] {
        let value = |v| EffectTrackValue::Shader {
            parameter: 0,
            value: v,
        };
        assert!(value(ShaderValue::Scalar(0.5))
            .sample_value(value(ShaderValue::Boolean(true)), alpha)
            .is_err());
        assert!(value(ShaderValue::Boolean(false))
            .sample_value(value(ShaderValue::Boolean(true)), alpha)
            .is_err());
        assert!(value(ShaderValue::Unsigned(0))
            .sample_value(value(ShaderValue::Unsigned(1)), alpha)
            .is_err());
    }
    let tracks: Vec<_> = binding
        .parameter_channels(
            &EffectUpdate::default()
                .shader_parameter(
                    &program.parameter("phase").unwrap(),
                    ShaderValue::Scalar(0.75),
                )
                .shader_parameter(
                    &program.parameter("field15").unwrap(),
                    ShaderValue::Scalar(0.875),
                ),
        )
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(i, (property, values))| TrackDefinition {
            id: TrackId::new(i as u64 + 1),
            property,
            values,
            ..track(0, ShaderValue::Scalar(0.25))
        })
        .collect();
    let mut runtime = noon_runtime::SceneInstance::new(
        CompiledScene::compile_objects(vec![row], &tracks).unwrap(),
    );
    let mut stale = tracks[0].clone();
    stale.id = TrackId::new(3);
    if let TrackValues::Effect { attachment, .. } = &mut stale.values {
        *attachment = SemanticNodeId::new(10, 2);
    }
    let before = runtime.frame().clone();
    assert!(runtime
        .apply_execution_patch(&ExecutionPatch::AddTrack(stale))
        .is_err());
    assert_eq!(runtime.frame(), &before);
    runtime.evaluate(0.5).unwrap();
    let edited = EffectUpdate::default()
        .shader_parameter(
            &program.parameter("field3").unwrap(),
            ShaderValue::Scalar(0.625),
        )
        .apply_to(binding.definition.clone())
        .unwrap();
    runtime
        .apply_execution_patch(&ExecutionPatch::SetEffects {
            object,
            effects: Arc::new(
                CompiledEffectStack::new([CompiledEffect {
                    definition: edited,
                    ..binding.clone()
                }])
                .unwrap(),
            ),
        })
        .unwrap();
    let value = only_effect(runtime.frame().objects[0].effects.as_ref().unwrap()).definition;
    let values = value.as_shader().unwrap().values();
    assert_eq!(values[0], ShaderValue::Scalar(0.375));
    assert_eq!(values[3], ShaderValue::Scalar(0.625));
    assert_eq!(values[15], ShaderValue::Scalar(0.3125));
    runtime
        .apply_execution_patch(&ExecutionPatch::SetEffectStack {
            object,
            expected: Box::new([binding.attachment]),
            effects: None,
        })
        .unwrap();
    assert_eq!(runtime.last_patch_stats().track_locators_removed, 2);
    runtime.seek(1.5).unwrap();
    assert!(runtime.frame().objects[0].effects.is_none());
}

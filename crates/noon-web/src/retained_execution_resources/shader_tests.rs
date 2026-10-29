//! Ordinary authoring and the real worker resource/frame codecs. Native and
//! single-context WASM product execution never use this serialization path.
use super::*;
use crate::{
    RetainedFamilyExecutionDeltaEncoder, TransportEffectDefinition, TransportShaderProgramHandle,
};
use noon::{effects::*, Color, ExecutionSession, Mobject, Scene};
use noon_core::ShaderParameterDefinition as Parameter;

fn program(source: String) -> WgslEffectProgram {
    WgslEffectProgram::prepare(
        ShaderSchema {
            profile: ShaderProfile::PaintedPointwiseV1,
            dependencies: ShaderDependencies {
                coordinates: ShaderCoordinates::LocalNormalized,
                authored_time: false,
                seed: true,
            },
            parameters: vec![
                Parameter {
                    name: "gain".into(),
                    kind: ShaderParameterType::Scalar {
                        range: 0.0..=1.0,
                        unit: ShaderUnit::Normalized,
                    },
                    default: ShaderValue::Scalar(0.25),
                },
                Parameter {
                    name: "direction".into(),
                    kind: ShaderParameterType::Vector2 {
                        min: [-2.0; 2],
                        max: [2.0; 2],
                        unit: ShaderUnit::Scene,
                    },
                    default: ShaderValue::Vector2([-0.0, 0.5]),
                },
                Parameter {
                    name: "tint".into(),
                    kind: ShaderParameterType::Color,
                    default: ShaderValue::Color(Color::rgba(0.2, 0.4, 0.8, 0.6)),
                },
                Parameter {
                    name: "enabled".into(),
                    kind: ShaderParameterType::Boolean,
                    default: ShaderValue::Boolean(true),
                },
                Parameter {
                    name: "mode".into(),
                    kind: ShaderParameterType::Unsigned {
                        range: 0..=u32::MAX,
                    },
                    default: ShaderValue::Unsigned(u32::MAX),
                },
            ]
            .into_boxed_slice(),
        },
        source,
    )
    .unwrap()
}
const SOURCE: &str = "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return vec4<f32>(input.color.rgb * p.tint.rgb, p.gain); }";
fn fixture() -> (Scene, [Mobject; 2], WgslEffectProgram) {
    // Large legal source makes an accidental per-frame resend unambiguous.
    let program = program(format!("//{}\n{SOURCE}", "a".repeat(60_000)));
    let mut scene = Scene::new();
    let mut objects = Vec::new();
    for x in [-1.5, 1.5] {
        let mut object = scene.circle(0.4).unwrap();
        object.set_translation(x, 0.0).unwrap();
        scene.add(&object).unwrap();
        scene
            .add_effect(&object, ShaderEffect::new(program.clone()), "material")
            .unwrap();
        objects.push(object);
    }
    (scene, objects.try_into().unwrap(), program)
}
fn encode(
    encoder: &mut RetainedFamilyExecutionDeltaEncoder,
    session: &mut ExecutionSession,
    snapshot: bool,
) -> RetainedFamilyExecutionDeltaEnvelope {
    let camera = session.inspection_camera().unwrap();
    encoder
        .encode_renderer_publication(
            &session.take_renderer_publication(),
            camera,
            snapshot,
            false,
        )
        .unwrap()
        .unwrap()
}
fn mirror() -> InstalledRetainedExecutionMirror {
    InstalledRetainedExecutionMirror::from_bundle_bytes(
        &RetainedResourceBundle::empty().encode_binary().unwrap(),
    )
    .unwrap()
}
fn key(delta: &RetainedFamilyExecutionDeltaEnvelope) -> TransportShaderProgramHandle {
    match delta.retained.objects[0].effects[0].definition {
        TransportEffectDefinition::Shader { program, .. } => program,
        _ => panic!("shader"),
    }
}
fn apply(
    mirror: &mut InstalledRetainedExecutionMirror,
    delta: &RetainedFamilyExecutionDeltaEnvelope,
) {
    mirror
        .apply_json(&serde_json::to_string(delta).unwrap())
        .unwrap();
}

#[test]
fn shader_programs_transfer_once_share_across_attachments_and_retire_exactly() {
    let (scene, objects, program) = fixture();
    let mut session = scene.execution_session().unwrap();
    let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(17);
    let mut mirror = mirror();
    let initial = encode(&mut encoder, &mut session, true);
    let handle = key(&initial);
    assert_eq!(
        initial
            .resource_additions
            .as_ref()
            .unwrap()
            .shader_program_count(),
        1
    );
    for object in &initial.retained.objects {
        let TransportEffectDefinition::Shader { program, .. } = object.effects[0].definition else {
            panic!("shader")
        };
        assert_eq!(program, handle);
    }
    // CBOR resource roundtrip and the actual JSON worker envelope both preserve
    // source/schema and every typed default, including signed zero/full u32.
    let bundle = initial.resource_additions.as_ref().unwrap();
    let installed = RetainedResourceBundle::decode_binary(&bundle.encode_binary().unwrap())
        .unwrap()
        .install()
        .unwrap();
    assert_eq!(installed.shader_program(handle), Some(&program));
    apply(&mut mirror, &initial);
    for (sender, received) in session
        .frame()
        .objects
        .iter()
        .zip(&mirror.frame().unwrap().objects)
    {
        assert_eq!(sender.effects, received.effects);
    }
    scene
        .live(&mut session)
        .set_effect(
            &objects[0],
            "material",
            EffectUpdate::default()
                .shader_parameter(
                    &program.parameter("gain").unwrap(),
                    ShaderValue::Scalar(0.75),
                )
                .shader_parameter(
                    &program.parameter("enabled").unwrap(),
                    ShaderValue::Boolean(false),
                )
                .seed(u32::MAX),
        )
        .unwrap();
    let changed = encode(&mut encoder, &mut session, false);
    assert_eq!(changed.retained.objects.len(), 1);
    assert!(changed.resource_additions.is_none());
    assert_eq!(key(&changed), handle);
    let json = serde_json::to_string(&changed).unwrap();
    assert!(
        json.len() < 3_000,
        "parameter row must not resend 60 KiB source/schema: {} bytes",
        json.len()
    );
    assert!(!json.contains("fn shade"));
    apply(&mut mirror, &changed);
    for (sender, received) in session
        .frame()
        .objects
        .iter()
        .zip(&mirror.frame().unwrap().objects)
    {
        assert_eq!(sender.effects, received.effects);
    }
    for (step, object) in objects.iter().enumerate() {
        scene
            .live(&mut session)
            .remove_effect(object, "material")
            .unwrap();
        let removed = encode(&mut encoder, &mut session, false);
        assert!(removed.resource_additions.is_none());
        assert_eq!(
            removed.resource_retirements.shader_programs,
            if step == 0 { vec![] } else { vec![handle] }
        );
        apply(&mut mirror, &removed);
        assert_eq!(
            mirror.resources().shader_program_count(),
            usize::from(step == 0)
        );
    }
    scene
        .live(&mut session)
        .add_effect(&objects[0], ShaderEffect::new(program), "material")
        .unwrap();
    let reenrolled = encode(&mut encoder, &mut session, false);
    assert_ne!(
        key(&reenrolled),
        handle,
        "retired handles never silently rebind"
    );
    assert_eq!(
        reenrolled
            .resource_additions
            .as_ref()
            .unwrap()
            .shader_program_count(),
        1
    );
    let mut missing = reenrolled.clone();
    missing.resource_additions = None;
    assert!(mirror
        .apply_json(&serde_json::to_string(&missing).unwrap())
        .is_err());
    assert_eq!(mirror.resources().shader_program_count(), 0);
    apply(&mut mirror, &reenrolled);
    assert_eq!(mirror.resources().shader_program_count(), 1);
}

#[test]
fn shader_resource_and_value_failures_preserve_frame_resources_and_retry_sequence() {
    let (scene, objects, _) = fixture();
    let mut session = scene.execution_session().unwrap();
    let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(19);
    let initial = encode(&mut encoder, &mut session, true);
    let replacement = program(format!("{SOURCE}\nfn marker() -> u32 {{ return 17u; }}"));
    scene
        .live(&mut session)
        .remove_effect(&objects[0], "material")
        .unwrap();
    scene
        .live(&mut session)
        .add_effect(&objects[0], ShaderEffect::new(replacement), "material")
        .unwrap();
    let next = encode(&mut encoder, &mut session, false);
    for case in 0..8 {
        let mut mirror = mirror();
        apply(&mut mirror, &initial);
        let before = mirror.frame().unwrap().clone();
        let context = mirror.publication_context();
        let mut bad = serde_json::to_value(&next).unwrap();
        match case {
            0 => bad["resource_additions"]["shader_programs"][0]["source"] = "fn shade(".into(),
            1 => {
                bad["resource_additions"]["shader_programs"][0]["schema"]["parameters"][0]
                    ["default"] = serde_json::to_value(ShaderValue::Scalar(2.0)).unwrap()
            }
            2 => {
                let copy = bad["resource_additions"]["shader_programs"][0].clone();
                bad["resource_additions"]["shader_programs"]
                    .as_array_mut()
                    .unwrap()
                    .push(copy);
            }
            3 => {
                bad["objects"][0]["effects"][0]["values"][0] =
                    serde_json::to_value(ShaderValue::Boolean(true)).unwrap()
            }
            4 => bad["objects"][0]["effects"][0]["values"] = serde_json::json!([]),
            5 => bad["objects"][0]["effects"][0]["program"]["session"] = 99.into(),
            6 => {
                bad["resource_additions"]["shader_programs"][0]["source"] =
                    "a".repeat(65_537).into()
            }
            _ => {
                bad["resource_additions"]["shader_programs"][0]["schema"]["unexpected"] =
                    true.into()
            }
        }
        assert!(
            mirror.apply_json(&bad.to_string()).is_err(),
            "corrupt shader case{case}"
        );
        assert_eq!(mirror.frame().unwrap(), &before);
        assert_eq!(mirror.publication_context(), context);
        assert_eq!(mirror.resources().shader_program_count(), 1);
        // A typed-value failure happens after tentative resource remapping. A
        // forged retry cannot use that uncommitted program through a leaked map.
        let mut missing = next.clone();
        missing.resource_additions = None;
        assert!(mirror
            .apply_json(&serde_json::to_string(&missing).unwrap())
            .is_err());
        assert_eq!(mirror.frame().unwrap(), &before);
        apply(&mut mirror, &next);
        assert_eq!(mirror.resources().shader_program_count(), 2);
    }
}

#[test]
fn composed_local_shader_worker_rejection_preserves_resources_frame_and_sequence() {
    let original = program(SOURCE.into());
    let mut schema = original.schema().clone();
    schema.dependencies.coordinates = ShaderCoordinates::Scene;
    let material = WgslEffectProgram::prepare(schema, original.source()).unwrap();
    let mut scene = Scene::new();
    let leaf = scene.circle(0.4).unwrap();
    let family = scene.family(&[(&leaf).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    scene
        .add_effect(&family, ShaderEffect::new(material), "material")
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(17);
    let valid = encode(&mut encoder, &mut session, true);
    // The real receiver must check a valid program's unsupported source role,
    // not merely trust the producer's checked attachment.
    let mut wire = serde_json::to_value(&valid).unwrap();
    wire["resource_additions"]["shader_programs"][0]["schema"]["dependencies"]["coordinates"] =
        "LocalNormalized".into();
    let mut mirror = mirror();
    let result = mirror.apply_json(&serde_json::to_string(&wire).unwrap());
    assert!(
        format!("{result:?}").contains("UnsupportedLocalShaderCoordinates"),
        "{result:?}"
    );
    assert!(mirror.frame().is_none());
    assert_eq!(mirror.resources().shader_program_count(), 0);
    apply(&mut mirror, &valid); // The rejected sequence remains retryable.
    assert_eq!(mirror.resources().shader_program_count(), 1);
}

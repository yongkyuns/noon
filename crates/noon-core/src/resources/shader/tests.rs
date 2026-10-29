use super::*;

const SCAN: &str = "
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let coverage = select(0.0, 1.0, abs(input.position.x - p.phase) <= p.width * 0.5);
    return vec4<f32>(p.tint.rgb, p.tint.a * coverage);
}";

fn schema() -> ShaderSchema {
    ShaderSchema {
        profile: ShaderProfile::PaintedPointwiseV1,
        dependencies: ShaderDependencies {
            coordinates: ShaderCoordinates::LocalNormalized,
            authored_time: false,
            seed: false,
        },
        parameters: vec![
            ShaderParameterDefinition {
                name: "width".into(),
                kind: ShaderParameterType::Scalar {
                    range: 0.01..=1.0,
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
                default: ShaderValue::Scalar(0.5),
            },
            ShaderParameterDefinition {
                name: "tint".into(),
                kind: ShaderParameterType::Color,
                default: ShaderValue::Color(Color::rgba(1.0, 1.0, 0.0, 1.0)),
            },
        ]
        .into_boxed_slice(),
    }
}

#[test]
fn prepared_sources_share_programs_but_keep_values_and_failed_edits_independent() {
    let program = WgslEffectProgram::prepare(schema(), SCAN).unwrap();
    assert_eq!(program.source(), SCAN);
    let original = ShaderEffect::new(program.clone());
    let phase = program.parameter("phase").unwrap();
    let changed = original
        .with_parameter(&phase, ShaderValue::Scalar(0.75))
        .unwrap();
    assert_eq!(original.values()[phase.index()], ShaderValue::Scalar(0.5));
    assert_eq!(changed.values()[phase.index()], ShaderValue::Scalar(0.75));
    assert!(Arc::ptr_eq(
        original.program().module(),
        changed.program().module()
    ));
    let unchanged = original
        .with_parameter(&phase, ShaderValue::Scalar(0.5))
        .unwrap();
    assert!(Arc::ptr_eq(&original.values, &unchanged.values));
    let zero = original
        .with_parameter(&phase, ShaderValue::Scalar(0.0))
        .unwrap();
    let negative_zero = zero
        .with_parameter(&phase, ShaderValue::Scalar(-0.0))
        .unwrap();
    assert!(
        matches!(negative_zero.values()[phase.index()], ShaderValue::Scalar(v) if v.to_bits() == (-0.0f64).to_bits()),
        "WGSL bitcasts can distinguish signed zero"
    );

    for bad in [
        ShaderValue::Scalar(f64::NAN),
        ShaderValue::Scalar(1.01),
        ShaderValue::Vector2([0.2, 0.3]),
    ] {
        assert!(original.with_parameter(&phase, bad).is_err());
    }
    let other =
        WgslEffectProgram::prepare(schema(), SCAN.replace("p.tint.rgb", "vec3<f32>(0.0)")).unwrap();
    assert_eq!(
        original.with_parameter(&other.parameter("phase").unwrap(), ShaderValue::Scalar(0.2)),
        Err(ShaderProgramError::WrongProgram)
    );
    assert!(WgslEffectProgram::prepare(schema(), "fn shade(").is_err());
    assert_eq!(original.values()[phase.index()], ShaderValue::Scalar(0.5));
}

#[test]
fn program_identity_excludes_attachment_values_but_keys_check_the_complete_schema() {
    let first = WgslEffectProgram::prepare(schema(), SCAN).unwrap();
    let mut changed = schema();
    changed.parameters[1].default = ShaderValue::Scalar(0.75);
    let second = WgslEffectProgram::prepare(changed, SCAN).unwrap();
    assert_eq!(
        first.module(),
        second.module(),
        "same pipeline despite different defaults"
    );
    assert_ne!(first, second);
    assert_eq!(
        ShaderEffect::new(first.clone()).with_parameter(
            &second.parameter("phase").unwrap(),
            ShaderValue::Scalar(0.2)
        ),
        Err(ShaderProgramError::WrongProgram)
    );
    assert_eq!(first.parameter("missing"), None);
}

#[test]
fn schema_rejects_duplicate_names_invalid_ranges_defaults_and_type_confusion() {
    for bad in ["width", "bad-name", "noon_private", "_reserved", "return"] {
        let mut candidate = schema();
        candidate.parameters[1].name = bad.into();
        assert!(
            WgslEffectProgram::prepare(candidate, SCAN).is_err(),
            "{bad}"
        );
    }
    for range in [1.0..=0.0, 0.0..=f64::INFINITY, 0.0..=f64::MAX] {
        let mut candidate = schema();
        candidate.parameters[0].kind = ShaderParameterType::Scalar {
            range,
            unit: ShaderUnit::Normalized,
        };
        assert!(WgslEffectProgram::prepare(candidate, SCAN).is_err());
    }
    assert!(!ShaderParameterType::Color.accepts(ShaderValue::Vector2([0.0, 1.0])));
    assert!(!ShaderParameterType::Vector2 {
        min: [0.0; 2],
        max: [1.0; 2],
        unit: ShaderUnit::Scene
    }
    .accepts(ShaderValue::Color(Color::WHITE)));
    assert!(!ShaderParameterType::Unsigned { range: 0..=7 }.accepts(ShaderValue::Unsigned(8)));
    assert!(!ShaderParameterType::Boolean.is_continuous());
    assert!(ShaderParameterType::Color.is_continuous());
    assert!(matches!(
        WgslEffectProgram::prepare(schema(), "x".repeat(65537)),
        Err(ShaderProgramError::Schema(_))
    ));
    let mut oversized = schema();
    oversized.parameters = vec![oversized.parameters[0].clone(); 17].into_boxed_slice();
    assert!(matches!(
        WgslEffectProgram::prepare(oversized, SCAN),
        Err(ShaderProgramError::Schema(_))
    ));
}

#[test]
fn checked_wgsl_rejects_undeclared_inputs_bad_abi_and_external_resources() {
    for source in [
        SCAN.replace("p.phase", "input.time"),
        SCAN.replace("p.phase", "f32(input.seed)"),
        SCAN.replace("p.phase", "p.missing"),
        SCAN.replace("fn shade", "fn renamed"),
        "fn shade() -> vec4<f32> { return vec4<f32>(0.0); }".into(),
        "fn shade(a: NoonShaderParameters, b: NoonShaderInput) -> vec4<f32> { return b.color; }"
            .into(),
        format!("var<private> hidden_time: f32;\n{SCAN}"),
        format!("@group(0) @binding(0) var texture: texture_2d<f32>;\n{SCAN}"),
        format!("override hidden_seed: u32 = 7u;\n{SCAN}"),
        format!(
            "{SCAN}\n@fragment fn entry() -> @location(0) vec4<f32> {{ return vec4<f32>(0.0); }}"
        ),
        SCAN.replace("let coverage", "discard; let coverage"),
        SCAN.replace("let coverage", "workgroupBarrier(); let coverage"),
        format!(
            "fn helper() {{ workgroupBarrier(); }}\n{}",
            SCAN.replace("let coverage", "helper(); let coverage")
        ),
        format!(
            "fn helper() {{ discard; }}\n{}",
            SCAN.replace("let coverage", "helper(); let coverage")
        ),
        SCAN.replace(
            "let coverage",
            "if input.position.x < 0.5 { discard; } let coverage",
        ),
    ] {
        assert!(
            WgslEffectProgram::prepare(schema(), source.as_str()).is_err(),
            "unexpected admission: {source}"
        );
    }
    let mut declared = schema();
    declared.dependencies.authored_time = true;
    declared.dependencies.seed = true;
    let source = SCAN.replace("p.phase", "fract(input.time + f32(input.seed))");
    WgslEffectProgram::prepare(declared, source).unwrap();
}

#[test]
fn shader_values_use_normal_transactions_and_reject_entire_invalid_patches() {
    use crate::{
        EffectParameter, EffectUpdate, SemanticMutationTransaction, SemanticNodeCreation,
        SemanticObjectState, SemanticStore, StoredGeometry,
    };
    let program = WgslEffectProgram::prepare(schema(), SCAN).unwrap();
    let phase = program.parameter("phase").unwrap();
    let width = program.parameter("width").unwrap();
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let object = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    let binding = tx.create_effect(object, "scan", ShaderEffect::new(program.clone()));
    let created = tx.apply(&mut store).unwrap();
    let binding = created.resolve(binding).unwrap();
    let original = store.semantic_effect_state(binding).unwrap().definition();
    let update = EffectUpdate::default().shader_parameter(&phase, ShaderValue::Scalar(0.75));
    assert_eq!(
        update.parameters().collect::<Vec<_>>(),
        [EffectParameter::Shader(1)]
    );
    let prepared = update.prepare(original.clone()).unwrap();
    for (time, expected) in [(1.0, 0.75), (0.0, 0.5), (0.5, 0.625), (0.25, 0.5625)] {
        let value = prepared
            .sample(time)
            .unwrap()
            .apply_to(original.clone())
            .unwrap();
        assert_eq!(
            value.as_shader().unwrap().values()[1],
            ShaderValue::Scalar(expected)
        );
        assert!(Arc::ptr_eq(
            value.as_shader().unwrap().program().module(),
            program.module()
        ));
    }
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(binding, update);
    tx.apply(&mut store).unwrap();
    let accepted = store.semantic_effect_state(binding).unwrap().clone();
    let revision = store.scene_revision();
    let other =
        WgslEffectProgram::prepare(schema(), SCAN.replace("p.tint.rgb", "vec3<f32>(0.0)")).unwrap();
    for bad in [
        EffectUpdate::default()
            .shader_parameter(&width, ShaderValue::Scalar(0.8))
            .shader_parameter(&phase, ShaderValue::Scalar(2.0)),
        EffectUpdate::default().shader_parameter(&phase, ShaderValue::Color(Color::WHITE)),
        EffectUpdate::default()
            .shader_parameter(&other.parameter("phase").unwrap(), ShaderValue::Scalar(0.2)),
    ] {
        let mut tx = SemanticMutationTransaction::new();
        tx.update_effect(binding, bad);
        assert!(tx.apply(&mut store).is_err());
        assert_eq!(store.scene_revision(), revision);
        assert_eq!(store.semantic_effect_state(binding).unwrap(), &accepted);
    }
}

#[test]
fn shader_copy_intent_addresses_the_last_schema_parameter_without_aliasing() {
    use crate::{
        EffectParameter, EffectUpdate, SemanticMutationTransaction, SemanticNodeCreation,
        SemanticObjectState, SemanticStore, StoredGeometry,
    };
    let mut schema = schema();
    let mut parameters = schema.parameters.into_vec();
    for i in parameters.len()..16 {
        parameters.push(ShaderParameterDefinition {
            name: format!("p{i}").into(),
            kind: ShaderParameterType::Scalar {
                range: 0.0..=1.0,
                unit: ShaderUnit::Normalized,
            },
            default: ShaderValue::Scalar(0.0),
        });
    }
    schema.parameters = parameters.into_boxed_slice();
    let program = WgslEffectProgram::prepare(schema, SCAN).unwrap();
    let last = program.parameter("p15").unwrap();
    let mut store = SemanticStore::new();
    let mut tx = SemanticMutationTransaction::new();
    let source = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    tx.create_effect(source, "scan", ShaderEffect::new(program));
    let source = tx.apply(&mut store).unwrap().resolve(source).unwrap();
    let mut tx = SemanticMutationTransaction::new();
    let copied = tx.create_node(SemanticNodeCreation::object(SemanticObjectState::new(
        StoredGeometry::Circle { radius: 1.0 },
    )));
    store.copy_effects_into(source, copied, &mut tx).unwrap();
    let copied = tx.apply(&mut store).unwrap().resolve(copied).unwrap();
    let binding = store.effect_by_name(copied, "scan").unwrap().unwrap();
    let mut tx = SemanticMutationTransaction::new();
    tx.update_effect(
        binding,
        EffectUpdate::default().shader_parameter(&last, ShaderValue::Scalar(0.0)),
    );
    assert!(tx.apply(&mut store).unwrap().impacts().is_empty());
    let intent = store
        .semantic_effect_state(binding)
        .unwrap()
        .copy_provenance()
        .unwrap();
    assert!(intent.writes_parameter(EffectParameter::Shader(15)));
    assert!(!intent.writes_parameter(EffectParameter::Shader(14)));
    assert!(!intent.writes_parameter(EffectParameter::Shader(255)));
    assert!(!intent.writes_parameter(EffectParameter::Color));
    let original = store.effect_by_name(source, "scan").unwrap().unwrap();
    assert!(store
        .semantic_effect_state(original)
        .unwrap()
        .copy_provenance()
        .is_none());
}

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

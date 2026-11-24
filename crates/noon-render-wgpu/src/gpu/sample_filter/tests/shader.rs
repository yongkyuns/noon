//! Real source buffers and a separate f64 pointwise reference, not a mock GPU.
use super::*;
use noon_core::EffectValue;
use noon_core::{
    ShaderCoordinates, ShaderDependencies, ShaderEffect, ShaderParameterDefinition,
    ShaderParameterType, ShaderProfile, ShaderSchema, ShaderUnit, ShaderValue, WgslEffectProgram,
};

const SOURCE: &str = "
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let phase = input.position.x + input.position.y * 0.25
        + p.offset.x + p.offset.y * 0.125 + input.time * 0.125
        + f32(input.seed & 255u) / 1024.0 + f32(p.count & 3u) * 0.0625 + p.field15 * 0.125;
    let coverage = select(0.0, clamp(phase * p.amount, 0.0, 1.0), p.enabled);
    return vec4<f32>(input.color.rgb * p.tint.rgb, coverage * p.tint.a);
}";

fn program(coordinates: ShaderCoordinates, source: &str) -> WgslEffectProgram {
    let scalar = |name: String, value| ShaderParameterDefinition {
        name: name.into(),
        kind: ShaderParameterType::Scalar {
            range: 0.0..=1.0,
            unit: ShaderUnit::Unitless,
        },
        default: ShaderValue::Scalar(value),
    };
    let mut parameters = vec![
        scalar("amount".into(), 0.03125),
        ShaderParameterDefinition {
            name: "offset".into(),
            kind: ShaderParameterType::Vector2 {
                min: [-1.0; 2],
                max: [1.0; 2],
                unit: ShaderUnit::Normalized,
            },
            default: ShaderValue::Vector2([0.125, -0.25]),
        },
        ShaderParameterDefinition {
            name: "tint".into(),
            kind: ShaderParameterType::Color,
            default: ShaderValue::Color(noon_core::Color::rgba(0.5, 0.75, 0.25, 0.5)),
        },
        ShaderParameterDefinition {
            name: "enabled".into(),
            kind: ShaderParameterType::Boolean,
            default: ShaderValue::Boolean(true),
        },
        ShaderParameterDefinition {
            name: "count".into(),
            kind: ShaderParameterType::Unsigned {
                range: 0..=u32::MAX,
            },
            default: ShaderValue::Unsigned(0x80000003),
        },
    ];
    parameters
        .extend((5..16).map(|i| scalar(format!("field{i}"), if i == 15 { 0.5 } else { 0.0 })));
    WgslEffectProgram::prepare(
        ShaderSchema {
            profile: ShaderProfile::PaintedPointwiseV1,
            dependencies: ShaderDependencies {
                coordinates,
                authored_time: true,
                seed: true,
            },
            parameters: parameters.into_boxed_slice(),
        },
        source,
    )
    .unwrap()
}

fn reference(
    input: &[u8],
    coordinates: ShaderCoordinates,
    context: ShaderSampleContext,
    opacity: f64,
) -> Vec<u8> {
    let mut result = Vec::new();
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] {
            let pixel = [f64::from(x) + 0.5, f64::from(y) + 0.5];
            let point = match coordinates {
                ShaderCoordinates::OutputPixels => [
                    pixel[0] + context.output_origin[0],
                    pixel[1] + context.output_origin[1],
                ],
                space => {
                    let matrix = if space == ShaderCoordinates::LocalNormalized {
                        context.local_from_capture.unwrap()
                    } else {
                        context.scene_from_capture
                    };
                    matrix.map(|row| row[0] * pixel[0] + row[1] * pixel[1] + row[2])
                }
            };
            let phase = point[0] + point[1] / 4.0 + 0.125 - 0.25 / 8.0
                + context.authored_time / 8.0
                + 255.0 / 1024.0
                + 3.0 / 16.0
                + 0.5 / 8.0;
            let gain = (phase / 32.0).clamp(0.0, 1.0) * 0.5 * opacity;
            let at = ((y * SIZE[0] + x) * 4) as usize;
            let alpha = f64::from(input[at + 3]) / 255.0;
            for channel in 0..3 {
                let straight = if alpha == 0.0 {
                    0.0
                } else {
                    f64::from(input[at + channel]) / 255.0 / alpha
                };
                result.push(
                    (straight * [0.5, 0.75, 0.25][channel] * alpha * gain * 255.0).round() as u8,
                );
            }
            result.push((alpha * gain * 255.0).round() as u8);
        }
    }
    result
}

#[test]
#[ignore = "requires a real native raster adapter; custom WGSL operator oracle"]
fn typed_shader_pixels_coordinates_time_alpha_and_program_lifetime() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("typed shader adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let input: Vec<_> = (0..SIZE[0] * SIZE[1])
        .flat_map(|i| {
            let alpha = i * 29 % 256;
            [
                (i * 7 % 256) * alpha / 255,
                (i * 11 % 256) * alpha / 255,
                (i * 13 % 256) * alpha / 255,
                alpha,
            ]
            .map(|v| v as u8)
        })
        .collect();
    let mut filter = SampleFilter::default();
    let mut scope = SampleScope::default();
    let mut count = 0;
    let mut maximum = 0;
    let mut context = ShaderSampleContext {
        local_from_capture: Some([[0.02, 0.015, -0.13], [0.01, -0.03, 0.8]]),
        scene_from_capture: [[0.0125, -0.00625, 0.25], [0.003125, 0.025, -0.125]],
        output_origin: [17.25, -4.5],
        authored_time: 0.0,
    };
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let texture = source(&device, format);
        let mut upload = input.clone();
        if format == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in upload.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        queue.write_texture(
            texture.as_image_copy(),
            &upload,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE[0] * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        for coordinates in [
            ShaderCoordinates::LocalNormalized,
            ShaderCoordinates::Scene,
            ShaderCoordinates::OutputPixels,
        ] {
            let program = program(coordinates, SOURCE);
            let definition = ShaderEffect::new(program.clone())
                .apply_effect_update(&noon_core::EffectUpdate::default().seed(u32::MAX))
                .unwrap();
            let (lease, first) =
                pollster::block_on(filter.prepare_shader_program(&device, &program)).unwrap();
            assert_eq!(first.pipeline_compiles, usize::from(count == 0));
            let (same, reused) =
                pollster::block_on(filter.prepare_shader_program(&device, &program)).unwrap();
            assert!(crate::gpu::GpuLease::ptr_eq(&lease, &same));
            assert_eq!(reused.pipeline_compiles, 0);
            for opacity in [1.0, 0.5, 0.125] {
                let mut midpoint = None;
                for time in [0.0, 0.5, 1.0, 0.25, 0.5] {
                    context.authored_time = time;
                    let mut p = parameters(0.0);
                    p.definition = definition.clone().into();
                    p.shader = Some(&context);
                    p.scope_opacity = opacity as f32;
                    let stats = prepare(
                        &mut filter,
                        &mut scope,
                        &device,
                        &queue,
                        &texture,
                        p.clone(),
                    )
                    .unwrap();
                    assert_eq!(stats.pipeline_compiles, 0);
                    let actual = read(&device, &queue, scope.output().unwrap());
                    let expected = reference(&input, coordinates, context, opacity);
                    if time == 0.5 {
                        if let Some(previous) = &midpoint {
                            assert_eq!(
                                &actual, previous,
                                "same-time shader replay must be byte-exact"
                            );
                        } else {
                            midpoint = Some(actual.clone());
                        }
                    }
                    for (&a, &b) in actual.iter().zip(&expected) {
                        maximum = maximum.max(a.abs_diff(b));
                        assert!(a.abs_diff(b) <= 1, "space={coordinates:?} time={time} opacity={opacity}: got{a} expected{b}");
                    }
                    let clean = prepare(
                        &mut filter,
                        &mut scope,
                        &device,
                        &queue,
                        &texture,
                        p.clone(),
                    )
                    .unwrap();
                    assert_eq!(
                        (
                            clean.texture_allocations,
                            clean.buffer_allocations,
                            clean.bind_group_creations,
                            clean.bytes_uploaded,
                            clean.composite_passes
                        ),
                        (0, 0, 0, 0, 0)
                    );
                    let output = scope.output().unwrap().clone();
                    let mut rejected = p.clone();
                    rejected.scratch_budget_bytes = 0;
                    assert_eq!(
                        prepare(&mut filter, &mut scope, &device, &queue, &texture, rejected),
                        Err(EffectPrepareError::ScratchBudgetExceeded)
                    );
                    assert_eq!(scope.output(), Some(&output));
                    count += 1;
                }
            }
            let disabled = definition
                .with_parameter(
                    &program.parameter("enabled").unwrap(),
                    ShaderValue::Boolean(false),
                )
                .unwrap();
            let mut p = parameters(0.0);
            p.definition = disabled.into();
            p.shader = Some(&context);
            let disabled = prepare(&mut filter, &mut scope, &device, &queue, &texture, p).unwrap();
            assert_eq!(
                (
                    disabled.pipeline_compiles,
                    disabled.texture_allocations,
                    disabled.buffer_allocations
                ),
                (0, 0, 0)
            );
            assert!(read(&device, &queue, scope.output().unwrap())
                .iter()
                .all(|&v| v == 0));
        }
    }
    let texture = source(&device, wgpu::TextureFormat::Rgba8Unorm);
    queue.write_texture(
        texture.as_image_copy(),
        &input,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE[0] * 4),
            rows_per_image: None,
        },
        texture.size(),
    );
    let identity = WgslEffectProgram::prepare(ShaderSchema {
        profile: ShaderProfile::PaintedPointwiseV1,
        dependencies: ShaderDependencies { coordinates: ShaderCoordinates::LocalNormalized, authored_time: false, seed: false },
        parameters: Box::new([]),
    }, "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return input.color; }").unwrap();
    let (identity_lease, _) =
        pollster::block_on(filter.prepare_shader_program(&device, &identity)).unwrap();
    let mut p = parameters(0.0);
    p.definition = ShaderEffect::new(identity.clone()).into();
    p.shader = Some(&context);
    prepare(
        &mut filter,
        &mut scope,
        &device,
        &queue,
        &texture,
        p.clone(),
    )
    .unwrap();
    assert_eq!(
        read(&device, &queue, scope.output().unwrap()),
        input,
        "identity must not square source coverage"
    );
    let static_context = ShaderSampleContext {
        authored_time: f64::NAN,
        ..context
    };
    let mut still = p.clone();
    still.shader = Some(&static_context);
    assert_eq!(
        prepare(&mut filter, &mut scope, &device, &queue, &texture, still)
            .unwrap()
            .composite_passes,
        0,
        "undeclared time must not dirty a static shader"
    );
    let output = scope.output().unwrap().clone();
    let no_local = ShaderSampleContext {
        local_from_capture: None,
        ..context
    };
    let mut missing = p.clone();
    missing.shader = Some(&no_local);
    assert_eq!(
        prepare(&mut filter, &mut scope, &device, &queue, &texture, missing),
        Err(EffectPrepareError::InvalidSourceBounds)
    );
    let huge = ShaderSampleContext {
        output_origin: [1e20, 0.0],
        ..context
    };
    let mut imprecise = p.clone();
    imprecise.shader = Some(&huge);
    imprecise.definition =
        ShaderEffect::new(program(ShaderCoordinates::OutputPixels, SOURCE)).into();
    assert_eq!(
        prepare(
            &mut filter,
            &mut scope,
            &device,
            &queue,
            &texture,
            imprecise
        ),
        Err(EffectPrepareError::CaptureCoordinatesOutOfRange)
    );
    assert_eq!(scope.output(), Some(&output));
    let cache = filter.shader_programs.len();
    let bad = program(
        ShaderCoordinates::Scene,
        &format!("{SOURCE}\n// fresh program"),
    );
    filter.test_shader_source = Some(|source| format!("{source}\ninvalid generated WGSL"));
    assert!(pollster::block_on(filter.prepare_shader_program(&device, &bad)).is_err());
    filter.test_shader_source = None;
    assert_eq!(filter.shader_programs.len(), cache);
    assert_eq!(scope.output(), Some(&output));
    assert_eq!(read(&device, &queue, scope.output().unwrap()), input);
    for i in 0..16 {
        let source = WgslEffectProgram::prepare(
            identity.schema().clone(),
            format!("{}\n// program {i}", identity.source()),
        )
        .unwrap();
        let (lease, _) =
            pollster::block_on(filter.prepare_shader_program(&device, &source)).unwrap();
        let mut p = parameters(0.0);
        p.definition = ShaderEffect::new(source).into();
        p.shader = Some(&context);
        let work = prepare(&mut filter, &mut scope, &device, &queue, &texture, p).unwrap();
        assert_eq!((work.texture_allocations, work.buffer_allocations), (0, 0));
        assert!(
            filter.shader_programs.len() <= 3,
            "dead program cache keys must retire"
        );
        drop(lease);
    }
    let mut stack_filter = crate::EffectStackFilter::default();
    let mut stack_scope = crate::EffectStackScope::default();
    let stack_program = program(ShaderCoordinates::OutputPixels, SOURCE);
    let custom: noon_core::EffectDefinition = ShaderEffect::new(stack_program.clone())
        .apply_effect_update(&noon_core::EffectUpdate::default().seed(u32::MAX))
        .unwrap()
        .into();
    let cells: noon_core::EffectDefinition =
        noon_core::Pixelate::new(noon_core::EffectUpdate::default().size(noon_core::Pixels(4.0)))
            .unwrap()
            .into();
    let prepare_stack = |filter: &mut crate::EffectStackFilter,
                         scope: &mut crate::EffectStackScope,
                         effects: &[noon_core::EffectDefinition],
                         budget: u64,
                         time: f64| {
        let context = ShaderSampleContext {
            authored_time: time,
            ..context
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let work = filter.prepare_encode(
            &device,
            &queue,
            &mut encoder,
            scope,
            crate::GlowCapture {
                source: &texture,
                silhouette: None,
                revision: 1,
            },
            effects,
            crate::EffectStackParameters {
                shader: Some(&context),
                working_format: wgpu::TextureFormat::Rgba8Unorm,
                output_height: SIZE[1],
                world_view_height: f64::from(SIZE[1]),
                scope_opacity: 0.5,
                scratch_budget_bytes: budget,
                source_bounds: None,
            },
        );
        if work.is_ok() {
            queue.submit([encoder.finish()]);
        }
        work
    };
    let ordered = [cells.clone(), custom.clone()];
    assert_eq!(
        prepare_stack(&mut stack_filter, &mut stack_scope, &ordered, 3840, 0.0),
        Err(EffectPrepareError::ShaderProgramNotPrepared)
    );
    assert_eq!(stack_scope.scratch_bytes(), 0);
    let (stack_lease, _) =
        pollster::block_on(stack_filter.prepare_shader_program(&device, &stack_program)).unwrap();
    let weak = crate::gpu::GpuLease::downgrade(&stack_lease);
    let mut first_order = Vec::new();
    let mut ordered_maximum = 0;
    for order in 0..2 {
        let mut start = None;
        let effects = if order == 0 {
            [custom.clone(), cells.clone()]
        } else {
            ordered.clone()
        };
        for time in [0.0, 0.75, 0.0] {
            prepare_stack(&mut stack_filter, &mut stack_scope, &effects, 3840, time).unwrap();
            let actual = read(&device, &queue, stack_scope.output().unwrap());
            let context = ShaderSampleContext {
                authored_time: time,
                ..context
            };
            let expected = if order == 0 {
                cell_reference(
                    &reference(&input, ShaderCoordinates::OutputPixels, context, 1.0),
                    4.0,
                    [0.0; 2],
                    0.5,
                    SIZE,
                )
            } else {
                reference(
                    &cell_reference(&input, 4.0, [0.0; 2], 1.0, SIZE),
                    ShaderCoordinates::OutputPixels,
                    context,
                    0.5,
                )
            };
            let error = actual
                .iter()
                .zip(expected)
                .map(|(&a, b)| a.abs_diff(b))
                .max()
                .unwrap();
            ordered_maximum = ordered_maximum.max(error);
            if time == 0.0 {
                if let Some(previous) = &start {
                    assert_eq!(
                        &actual, previous,
                        "ordered shader rewind must be byte-exact"
                    );
                } else {
                    start = Some(actual.clone());
                }
            }
            assert!(error <= 2, "ordered shader/cell composition error{error}");
            if order == 0 && time == 0.0 {
                first_order = actual;
            } else if order == 1 && time == 0.0 {
                assert!(
                    actual
                        .iter()
                        .zip(&first_order)
                        .any(|(a, b)| a.abs_diff(*b) > 2),
                    "operator order must remain visible"
                );
            }
        }
    }
    let output = stack_scope.output().unwrap().clone();
    assert_eq!(
        prepare_stack(&mut stack_filter, &mut stack_scope, &ordered, 1, 0.0),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert_eq!(stack_scope.output(), Some(&output));
    drop(stack_lease);
    prepare_stack(&mut stack_filter, &mut stack_scope, &[cells], 3840, 0.0).unwrap();
    assert!(
        weak.upgrade().is_none(),
        "warm output texture must not retain a removed shader pipeline"
    );
    eprintln!("ordered custom shader:6 images/{} channels, maximum byte error{ordered_maximum}; retired program released",6*input.len());
    drop(identity_lease);
    eprintln!("typed shader oracle: {count} images / {} channels, maximum byte error{maximum}; exact identity and failed compile retained; program churn16", count*input.len());
}

//! Normal Rust authoring/capture. A separate no-effect scene supplies qualified
//! raster coverage; the f64 field/projection below qualifies shader composition.
use super::super::*;
use noon::effects::{
    ShaderCoordinates, ShaderDependencies, ShaderEffect, ShaderParameterDefinition,
    ShaderParameterType, ShaderProfile, ShaderSchema, ShaderUnit, ShaderValue, WgslEffectProgram,
};

const SOURCE: &str = "
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let coverage = clamp(p.phase + dot(input.position, p.weight), 0.0, 1.0);
    return vec4<f32>(input.color.rgb * p.tint.rgb, coverage * p.tint.a);
}";
fn weights(space: ShaderCoordinates) -> [f64; 2] {
    match space {
        ShaderCoordinates::LocalNormalized => [0.125, 0.25],
        ShaderCoordinates::Scene => [0.025, 0.05],
        ShaderCoordinates::OutputPixels => [0.00125, 0.0025],
    }
}
fn program(space: ShaderCoordinates, source: &str) -> WgslEffectProgram {
    WgslEffectProgram::prepare(
        ShaderSchema {
            profile: ShaderProfile::PaintedPointwiseV1,
            dependencies: ShaderDependencies {
                coordinates: space,
                authored_time: false,
                seed: false,
            },
            parameters: vec![
                ShaderParameterDefinition {
                    name: "phase".into(),
                    kind: ShaderParameterType::Scalar {
                        range: 0.0..=1.0,
                        unit: ShaderUnit::Normalized,
                    },
                    default: ShaderValue::Scalar(0.2),
                },
                ShaderParameterDefinition {
                    name: "tint".into(),
                    kind: ShaderParameterType::Color,
                    default: ShaderValue::Color(Color::WHITE),
                },
                ShaderParameterDefinition {
                    name: "weight".into(),
                    kind: ShaderParameterType::Vector2 {
                        min: [-1.0; 2],
                        max: [1.0; 2],
                        unit: ShaderUnit::Unitless,
                    },
                    default: ShaderValue::Vector2(weights(space)),
                },
            ]
            .into_boxed_slice(),
        },
        source,
    )
    .unwrap()
}
fn options() -> AnimationOptions {
    AnimationOptions::new()
        .run_time(1.0)
        .rate_func(RateFunction::Linear)
}
fn fixture(
    program: Option<&WgslEffectProgram>,
    angle: f64,
    scale: [f64; 2],
) -> (Scene, [Mobject; 2], noon::DeclaredAnimation) {
    let mut scene = Scene::new();
    let mut objects = Vec::new();
    for i in 0..2 {
        let mut object = scene.rectangle(1.6, 1.2).unwrap();
        object.disable_stroke().unwrap();
        object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
        object
            .set_translation(
                if i == 0 { -1.6 } else { 1.6 },
                if i == 0 { -0.25 } else { 0.35 },
            )
            .unwrap();
        object
            .set_rotation(if i == 0 { angle } else { -angle })
            .unwrap();
        object
            .set_scale(
                scale[0] * if i == 0 { 1.0 } else { 0.8 },
                scale[1] * if i == 0 { 1.0 } else { 1.1 },
            )
            .unwrap();
        object
            .set_object_opacity(if i == 0 { 0.75 } else { 0.5 })
            .unwrap();
        if let Some(program) = program {
            let tint = if i == 0 {
                Color::rgba(0.5, 0.75, 1.0, 0.5)
            } else {
                Color::rgba(1.0, 0.5, 0.75, 0.75)
            };
            let definition = ShaderEffect::new(program.clone())
                .with_parameter(
                    &program.parameter("phase").unwrap(),
                    ShaderValue::Scalar(if i == 0 { 0.3 } else { 0.2 }),
                )
                .unwrap()
                .with_parameter(
                    &program.parameter("tint").unwrap(),
                    ShaderValue::Color(tint),
                )
                .unwrap();
            scene.add_effect(&object, definition, "material").unwrap();
        }
        scene.add(&object).unwrap();
        objects.push(object);
    }
    let objects: [Mobject; 2] = objects.try_into().unwrap();
    let mut target = objects[1].target_editor().unwrap();
    target.shift(0.4, 0.0).unwrap();
    target.set_rotation(-angle + 0.2).unwrap();
    if let Some(program) = program {
        target
            .set_effect(
                "material",
                EffectUpdate::default().shader_parameter(
                    &program.parameter("phase").unwrap(),
                    ShaderValue::Scalar(0.7),
                ),
            )
            .unwrap();
    }
    let animation = scene
        .declare_transform_to(&objects[1], &target, options())
        .unwrap();
    (scene, objects, animation)
}

#[allow(clippy::too_many_arguments)]
fn reference(
    input: &[u8],
    size: [u32; 2],
    camera: Camera2D,
    space: ShaderCoordinates,
    angle: f64,
    scale: [f64; 2],
    time: f64,
    second_phase: f64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    for y in 0..size[1] {
        for x in 0..size[0] {
            let world = [
                f64::from(camera.center.x)
                    + ((f64::from(x) + 0.5) / f64::from(size[0]) - 0.5)
                        * f64::from(camera.world_size.x),
                f64::from(camera.center.y)
                    + (0.5 - (f64::from(y) + 0.5) / f64::from(size[1]))
                        * f64::from(camera.world_size.y),
            ];
            let first = world[0] < 0.0;
            let point = match space {
                ShaderCoordinates::Scene => world,
                ShaderCoordinates::OutputPixels => [f64::from(x) + 0.5, f64::from(y) + 0.5],
                ShaderCoordinates::LocalNormalized => {
                    let position = if first {
                        [-1.6, -0.25]
                    } else {
                        [1.6 + 0.4 * time, 0.35]
                    };
                    let rotation = if first { angle } else { -angle + 0.2 * time };
                    let delta = [world[0] - position[0], world[1] - position[1]];
                    let (sin, cos) = rotation.sin_cos();
                    let local = [
                        (cos * delta[0] + sin * delta[1])
                            / (scale[0] * if first { 1.0 } else { 0.8 }),
                        (-sin * delta[0] + cos * delta[1])
                            / (scale[1] * if first { 1.0 } else { 1.1 }),
                    ];
                    [local[0] / 1.6 + 0.5, local[1] / 1.2 + 0.5]
                }
            };
            let weight = weights(space);
            let phase = if first { 0.3 } else { second_phase };
            let tint = if first {
                [0.5, 0.75, 1.0, 0.5]
            } else {
                [1.0, 0.5, 0.75, 0.75]
            };
            let gain =
                (phase + point[0] * weight[0] + point[1] * weight[1]).clamp(0.0, 1.0) * tint[3];
            let at = ((y * size[0] + x) * 4) as usize;
            for c in 0..4 {
                output.push(
                    (f64::from(input[at + c]) * gain * if c == 3 { 1.0 } else { tint[c] }).round()
                        as u8,
                );
            }
        }
    }
    output
}

#[test]
#[ignore = "requires a real adapter; normal Rust shader source/capture qualification"]
fn shader_scene_animation_pose_camera_resolution_and_replay_match_independent_field() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("ordinary shader scene adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut samples = 0;
    let mut maximum = 0;
    for space in [
        ShaderCoordinates::LocalNormalized,
        ShaderCoordinates::Scene,
        ShaderCoordinates::OutputPixels,
    ] {
        let program = program(space, SOURCE);
        for (angle, scale) in [(0.0, [1.0, 1.0]), (0.37, [1.4, 0.6]), (-0.51, [-1.2, 0.9])] {
            for size in [[80, 60], [160, 120]] {
                for (center, world) in [
                    (Vec2::ZERO, Vec2::new(8.0, 6.0)),
                    (Vec2::new(1.2, -0.35), Vec2::new(6.4, 4.8)),
                ] {
                    let camera = Camera2D::new(center, world).unwrap();
                    let (scene, objects, animation) = fixture(Some(&program), angle, scale);
                    let (plain, _, plain_animation) = fixture(None, angle, scale);
                    let mut session = scene.execution_session().unwrap();
                    let mut baseline = plain.execution_session().unwrap();
                    let segment = scene.live(&mut session).play_animation(&animation).unwrap();
                    plain
                        .live(&mut baseline)
                        .play_animation(&plain_animation)
                        .unwrap();
                    let mut surface =
                        Surface::with_target_size(&device, &queue, Vec2::new(80.0, 60.0), size);
                    let mut plain_surface =
                        Surface::with_target_size(&device, &queue, Vec2::new(80.0, 60.0), size);
                    surface.renderer.set_camera(&queue, camera);
                    plain_surface.renderer.set_camera(&queue, camera);
                    let mut midpoint = None;
                    for (step, time) in [0.0, 0.5, 1.0, 0.25, 0.5].into_iter().enumerate() {
                        session.seek(time).unwrap();
                        baseline.seek(time).unwrap();
                        let input = plain_surface.render_on(
                            &device,
                            &queue,
                            &mut baseline,
                            wgpu::Color::TRANSPARENT,
                        );
                        let actual = surface.render_on(
                            &device,
                            &queue,
                            &mut session,
                            wgpu::Color::TRANSPARENT,
                        );
                        assert_eq!(
                            surface.program_stats.pipeline_compiles,
                            usize::from(step == 0),
                            "one shared program for two independent attachments"
                        );
                        let expected = reference(
                            &input,
                            size,
                            camera,
                            space,
                            angle,
                            scale,
                            time,
                            0.2 + 0.5 * time,
                        );
                        let error = actual
                            .iter()
                            .zip(&expected)
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap();
                        maximum = maximum.max(error);
                        assert!(error<=2,"space={space:?} angle={angle} scale={scale:?} size={size:?} time={time}: max RGBA error{error}");
                        if time == 0.5 {
                            if let Some(old) = &midpoint {
                                assert_eq!(&actual, old, "ordinary shader rewind must be exact");
                            } else {
                                midpoint = Some(actual.clone());
                            }
                        }
                        samples += 1;
                    }
                    scene
                        .live(&mut session)
                        .advance_segment_to(segment, 1.0)
                        .unwrap();
                    scene.live(&mut session).complete_segment(segment).unwrap();
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                    scene
                        .live(&mut session)
                        .set_effect(
                            &objects[1],
                            "material",
                            EffectUpdate::default().shader_parameter(
                                &program.parameter("phase").unwrap(),
                                ShaderValue::Scalar(0.3),
                            ),
                        )
                        .unwrap();
                    let actual =
                        surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                    assert_eq!(
                        surface.program_stats,
                        noon_render_wgpu::EffectProgramStats {
                            rows_examined: 1,
                            pipeline_compiles: 0
                        }
                    );
                    assert_eq!(
                        (
                            surface.effect_stats.source_passes,
                            surface.effect_stats.texture_allocations
                        ),
                        (0, 0)
                    );
                    baseline.seek(1.0).unwrap();
                    let input = plain_surface.render_on(
                        &device,
                        &queue,
                        &mut baseline,
                        wgpu::Color::TRANSPARENT,
                    );
                    let expected = reference(&input, size, camera, space, angle, scale, 1.0, 0.3);
                    let error = actual
                        .iter()
                        .zip(expected)
                        .map(|(a, b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                    assert!(error <= 2, "released shader parameter error{error}");
                }
            }
        }
    }
    eprintln!("ordinary shader scene:{samples} samples, max byte error{maximum}; two shared-program attachments; exact rewind and local parameter-only reuse");
}

#[test]
#[ignore = "requires a real adapter; shader residency and CPU-rejected edits"]
fn offscreen_shader_residency_and_rejected_source_keep_the_published_scene() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let program = program(ShaderCoordinates::LocalNormalized, SOURCE);
    let mut scene = Scene::new();
    let mut object = scene.rectangle(2.0, 1.0).unwrap();
    object.disable_stroke().unwrap();
    object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    object.set_translation(20.0, 0.0).unwrap();
    scene
        .add_effect(&object, ShaderEffect::new(program.clone()), "material")
        .unwrap();
    scene.add(&object).unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut surface = Surface::new(&device, &queue);
    let hidden = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    assert!(hidden.iter().all(|&v| v == 0));
    assert_eq!(surface.program_stats.pipeline_compiles, 1);
    scene.live(&mut session).shift(&object, -20.0, 0.0).unwrap();
    let valid = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    assert_eq!(surface.program_stats.pipeline_compiles, 0);
    assert!(valid.iter().any(|&v| v > 0));
    let before = session.publication_context();
    let changes = session.resource_publication().changes().clone();
    assert!(WgslEffectProgram::prepare(
        program.schema().clone(),
        format!("{SOURCE}\nfn fs_sample() {{}}")
    )
    .is_err());
    assert_eq!(session.publication_context(), before);
    assert_eq!(session.resource_publication().changes(), &changes);
    assert_eq!(
        readback(&device, &queue, &surface.target),
        valid,
        "failed preparation must keep the actual last image"
    );
    assert_eq!(
        surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
        valid
    );
}

#[test]
#[ignore = "requires a real adapter; canonical shader time through an ordinary wait"]
fn shader_time_wait_pixels_and_seek_use_authored_time_without_source_recapture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let program = WgslEffectProgram::prepare(ShaderSchema {
        profile: ShaderProfile::PaintedPointwiseV1,
        dependencies: ShaderDependencies { coordinates: ShaderCoordinates::LocalNormalized, authored_time: true, seed: false },
        parameters: Box::new([]),
    }, "fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> { return vec4<f32>(input.color.rgb, 0.2 + 0.25 * input.time); }").unwrap();
    let make = |effect: bool| {
        let mut scene = Scene::new();
        let mut source = scene.rectangle(1.6, 1.2).unwrap();
        source.set_fill(0.5, 0.75, 1.0, 0.5).unwrap();
        source.disable_stroke().unwrap();
        source.set_object_opacity(0.75).unwrap();
        scene.add(&source).unwrap();
        if effect {
            scene
                .add_effect(&source, ShaderEffect::new(program.clone()), "clock")
                .unwrap();
        }
        scene
    };
    let scene = make(true);
    let plain = make(false);
    let mut session = scene.execution_session().unwrap();
    let mut baseline = plain.execution_session().unwrap();
    let wait = scene.live(&mut session).wait_segment(2.0).unwrap();
    let plain_wait = plain.live(&mut baseline).wait_segment(2.0).unwrap();
    assert_eq!(
        session.segment_state(wait).timeline(),
        noon_runtime::TimelineWakeState::Continuous
    );
    let mut surface = Surface::new(&device, &queue);
    let mut plain_surface = Surface::new(&device, &queue);
    let mut midpoint = None;
    let mut overall_maximum = 0;
    for (step, time) in [0.0, 0.25, 0.75, 1.5, 0.75, 1.5].into_iter().enumerate() {
        if step < 4 {
            scene
                .live(&mut session)
                .advance_segment_to(wait, time)
                .unwrap();
            plain
                .live(&mut baseline)
                .advance_segment_to(plain_wait, time)
                .unwrap();
        } else {
            session.seek(time).unwrap();
            baseline.seek(time).unwrap();
        }
        let input =
            plain_surface.render_on(&device, &queue, &mut baseline, wgpu::Color::TRANSPARENT);
        let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
        let expected: Vec<_> = input
            .iter()
            .map(|&v| (f64::from(v) * (0.2 + 0.25 * time)).round() as u8)
            .collect();
        let maximum = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        overall_maximum = overall_maximum.max(maximum);
        assert!(
            maximum <= 2,
            "canonical time={time}: max RGBA error{maximum}"
        );
        if step == 2 {
            midpoint = Some(actual.clone());
        }
        if step == 4 {
            assert_eq!(midpoint.as_ref(), Some(&actual));
        }
        assert_eq!(
            surface.program_stats.pipeline_compiles,
            usize::from(step == 0)
        );
        if step > 0 && step < 4 {
            assert_eq!(surface.effect_stats.source_passes, 0);
            assert_eq!(surface.effect_stats.texture_allocations, 0);
            assert_eq!(surface.effect_stats.sources_prepared, 1);
        }
    }
    eprintln!("ordinary shader canonical time:6 images, max byte error{overall_maximum}; independent coverage gain; exact seek; no source recapture or allocation on forward time changes");
}

#[test]
#[ignore = "requires a real adapter; owned GPU compilation, stale work and retirement"]
fn owned_program_preparation_rejects_stale_bindings_reuses_modules_and_retires_churn() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut scene = Scene::new();
    let mut object = scene.rectangle(1.6, 1.2).unwrap();
    object.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    object.disable_stroke().unwrap();
    scene.add(&object).unwrap();
    let mut hidden = scene.rectangle(1.0, 1.0).unwrap();
    hidden.set_translation(20.0, 0.0).unwrap();
    scene.add(&hidden).unwrap();
    let mut incoming = scene.rectangle(1.6, 1.2).unwrap();
    incoming.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
    incoming.disable_stroke().unwrap();
    let mut session = scene.execution_session().unwrap();
    let mut surface = Surface::new(&device, &queue);
    let input = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    let program = program(ShaderCoordinates::LocalNormalized, SOURCE);
    let definition = |program: &WgslEffectProgram| {
        ShaderEffect::new(program.clone())
            .with_parameter(
                &program.parameter("weight").unwrap(),
                ShaderValue::Vector2([0.0; 2]),
            )
            .unwrap()
            .with_parameter(
                &program.parameter("phase").unwrap(),
                ShaderValue::Scalar(0.7),
            )
            .unwrap()
    };
    scene
        .live(&mut session)
        .add_effect(&object, definition(&program), "material")
        .unwrap();
    let preparation = surface
        .renderer
        .effect_program_preparation(&device, &session.resource_publication())
        .unwrap();
    let preparation = preparation
        .try_ready()
        .expect_err("new module must be prepared asynchronously");
    // Input/parameter work may commit while device validation awaits. The request
    // owns only immutable modules, so it neither borrows nor rewrites this source.
    scene
        .live(&mut session)
        .set_effect(
            &object,
            "material",
            EffectUpdate::default().shader_parameter(
                &program.parameter("phase").unwrap(),
                ShaderValue::Scalar(0.6),
            ),
        )
        .unwrap();
    let prepared = pollster::block_on(preparation.prepare()).unwrap();
    assert_eq!(prepared.stats().pipeline_compiles, 1);
    let context = session.publication_context();
    let changes = session.resource_publication().changes().clone();
    surface
        .renderer
        .cache_effect_programs(&device, &prepared)
        .unwrap();
    {
        let mut hdr = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba16Float);
        hdr.copy_effect_program_cache_from(&device, &surface.renderer);
        let modules = hdr
            .effect_module_preparation(&device, std::slice::from_ref(&program))
            .try_ready()
            .expect("same-device modules survive a working-target replacement");
        assert_eq!(modules.stats().pipeline_compiles, 0);
        let mut compatible = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        compatible.copy_effect_program_cache_from(&device, &hdr);
        assert!(
            compatible.cache_effect_programs(&device, &modules).is_err(),
            "cache authority and row proofs are not transferred"
        );
        let rows = compatible
            .effect_program_preparation(&device, &session.resource_publication())
            .unwrap()
            .try_ready()
            .expect("new rows reuse the retained immutable modules");
        assert_eq!(rows.stats().pipeline_compiles, 0);
        compatible
            .commit_effect_programs(&device, &session.resource_publication(), rows)
            .unwrap();
        assert_eq!(compatible.resident_effect_program_counts(), [1, 1]);
        let (other_device, other_queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut other =
            GpuRenderer::new(&other_device, &other_queue, wgpu::TextureFormat::Rgba8Unorm);
        other.copy_effect_program_cache_from(&other_device, &compatible);
        assert!(
            other
                .effect_module_preparation(&other_device, std::slice::from_ref(&program))
                .try_ready()
                .is_err(),
            "a different device cannot reuse these programs"
        );
    }
    let current = surface
        .renderer
        .effect_program_preparation(&device, &session.resource_publication())
        .unwrap()
        .try_ready()
        .expect("newer parameter publication reuses compiled module");
    assert!(surface
        .renderer
        .commit_effect_programs(&device, &session.resource_publication(), prepared)
        .is_err());
    assert_eq!(session.publication_context(), context);
    assert_eq!(session.resource_publication().changes(), &changes);
    assert_eq!(surface.renderer.resident_effect_program_counts(), [0, 1]);
    assert_eq!(readback(&device, &queue, &surface.target), input);
    let work = surface
        .renderer
        .commit_effect_programs(&device, &session.resource_publication(), current)
        .unwrap();
    assert_eq!(work.pipeline_compiles, 0);
    assert_eq!(surface.renderer.resident_effect_program_counts(), [1, 1]);
    let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    let expected: Vec<_> = input
        .iter()
        .map(|&v| (f64::from(v) * 0.6).round() as u8)
        .collect();
    assert!(actual
        .iter()
        .zip(&expected)
        .all(|(a, b)| a.abs_diff(*b) <= 2));
    let old = surface
        .renderer
        .effect_program_preparation(&device, &session.resource_publication())
        .unwrap()
        .try_ready()
        .unwrap();
    surface.renderer.reset_retained_effect_publication();
    assert!(
        surface
            .renderer
            .commit_effect_programs(&device, &session.resource_publication(), old)
            .is_err(),
        "reset cache must reject even an equal-context token"
    );
    assert_eq!(readback(&device, &queue, &surface.target), actual);
    assert_eq!(
        surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
        actual
    );
    assert_eq!(surface.program_stats.pipeline_compiles, 1);
    scene
        .live(&mut session)
        .remove_effect(&object, "material")
        .unwrap();
    assert_eq!(
        surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
        input
    );
    assert_eq!(surface.renderer.resident_effect_program_counts(), [0, 0]);
    for index in 0..16 {
        let source = format!("{SOURCE}\nfn marker() -> u32 {{ return {index}u; }}");
        let program = WgslEffectProgram::prepare(program.schema().clone(), source).unwrap();
        scene
            .live(&mut session)
            .add_effect(&object, definition(&program), "material")
            .unwrap();
        let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
        let expected: Vec<_> = input
            .iter()
            .map(|&v| (f64::from(v) * 0.7).round() as u8)
            .collect();
        assert!(actual
            .iter()
            .zip(&expected)
            .all(|(a, b)| a.abs_diff(*b) <= 2));
        assert_eq!(surface.program_stats.pipeline_compiles, 1);
        assert_eq!(surface.renderer.resident_effect_program_counts(), [1, 1]);
        scene
            .live(&mut session)
            .remove_effect(&object, "material")
            .unwrap();
        assert_eq!(
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
            input
        );
        assert_eq!(surface.renderer.resident_effect_program_counts(), [0, 0]);
    }
    scene
        .live(&mut session)
        .add_effect(&incoming, definition(&program), "material")
        .unwrap();
    session.require_effect_resource_preparation();
    let context = session.publication_context();
    let pending = scene
        .live(&mut session)
        .prepare_membership(noon::SceneMembershipRequest::Replace {
            old: (&object).into(),
            new: (&incoming).into(),
        })
        .unwrap();
    let modules = pollster::block_on(
        surface
            .renderer
            .effect_module_preparation(&device, pending.programs())
            .prepare(),
    )
    .unwrap();
    assert_eq!(modules.stats().pipeline_compiles, 1);
    surface
        .renderer
        .cache_effect_programs(&device, &modules)
        .unwrap();
    assert_eq!(session.publication_context(), context);
    assert_eq!(readback(&device, &queue, &surface.target), input);
    scene
        .live(&mut session)
        .publish_staged_effects(pending)
        .unwrap();
    assert!(!scene.live(&mut session).contains(&object).unwrap());
    assert!(scene.live(&mut session).contains(&incoming).unwrap());
    let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
    let expected: Vec<_> = input
        .iter()
        .map(|&v| (f64::from(v) * 0.7).round() as u8)
        .collect();
    assert!(actual
        .iter()
        .zip(&expected)
        .all(|(a, b)| a.abs_diff(*b) <= 2));
    assert_eq!(surface.program_stats.pipeline_compiles, 0);
    eprintln!("owned shader preparation: stale context and reset rejected; current module reused; pending changes/last image retained;16 real program enrollment/retirement cycles; shader-bearing replacement prepares before publication and matches independent coverage gain");
}

use super::*;
use noon_core::{Glow, GlowSource};

fn glow(source: GlowSource, intensity: f64) -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(91, if source == GlowSource::Painted { 3 } else { 4 }),
            definition: Glow::new(
                EffectUpdate::default()
                    .radius(Pixels(2.0))
                    .source(source)
                    .intensity(intensity)
                    .color(Color::rgba(0.7, 0.2, 0.9, 0.6)),
            )
            .unwrap()
            .into(),
        }])
        .unwrap(),
    )
}

fn reference(images: &[Vec<u8>; 2], silhouette: bool, intensity: f64) -> Vec<u8> {
    let mut layers = reference_layers(images, false);
    layers[0] = blur_with_opacity(&layers[0], 1.0);
    let inner = blur_with_opacity(&nested::source_over_layers(&layers[..2]), 1.0);
    let source = nested::source_over_layers(&[inner, layers[2].clone()]);
    let mask = if silhouette {
        compose(&reference_layers(images, true))
    } else {
        source.clone()
    };
    let mut output = [24, 48, 64, 255].repeat((SIZE[0] * SIZE[1]) as usize);
    let mass = (-6i32..=6)
        .flat_map(|y| (-6i32..=6).map(move |x| (-f64::from(x * x + y * y) / 8.0).exp()))
        .sum::<f64>();
    for y in 0..SIZE[1] as i32 {
        for x in 0..SIZE[0] as i32 {
            let mut value = 0.0;
            for dy in -6i32..=6 {
                for dx in -6i32..=6 {
                    let xx = x + dx;
                    let yy = y + dy;
                    if xx >= 0 && yy >= 0 && xx < SIZE[0] as i32 && yy < SIZE[1] as i32 {
                        value +=
                            f64::from(mask[(yy as usize * SIZE[0] as usize + xx as usize) * 4 + 3])
                                / 255.0
                                * (-f64::from(dx * dx + dy * dy) / 8.0).exp()
                                / mass;
                    }
                }
            }
            let at = (y as usize * SIZE[0] as usize + x as usize) * 4;
            let alpha = (value * intensity * 0.6).clamp(0.0, 1.0);
            let original_alpha = f64::from(source[at + 3]) / 255.0;
            let tint = [0.7, 0.2, 0.9, 1.0];
            let filtered: [u8; 4] = std::array::from_fn(|c| {
                ((f64::from(source[at + c]) / 255.0 + (1.0 - original_alpha) * tint[c] * alpha)
                    * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8
            });
            over(
                &mut output[at..at + 4],
                filtered.map(|value| f64::from(value) / 255.0),
            );
        }
    }
    for y in 28..32 {
        for x in 37..43 {
            let at = (y * SIZE[0] as usize + x) * 4;
            over(&mut output[at..at + 4], [0.6, 0.45, 0.075, 0.6]);
        }
    }
    output
}

#[test]
#[ignore = "requires a real native raster adapter; original coverage versus filtered-input oracle"]
fn nested_glow_mask_uses_original_coverage_and_painted_uses_filtered_children() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("nested mask raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (mut runtime, images) = nested::nested_fixture(false);
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(&device, &queue, SIZE[0], SIZE[1]);
    renderer.set_camera(
        &queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    for step in 0..3 {
        let silhouette = step > 0;
        let intensity = if step == 2 { 1.25 } else { 1.5 };
        let effects = glow(
            if silhouette {
                GlowSource::Silhouette
            } else {
                GlowSource::Painted
            },
            intensity,
        );
        let patch = if step < 2 {
            ExecutionPatch::SetEffectStack {
                object: OWNER,
                expected: vec![SemanticNodeId::new(91, if step == 0 { 2 } else { 3 })]
                    .into_boxed_slice(),
                effects: Some(effects),
            }
        } else {
            ExecutionPatch::SetEffects {
                object: OWNER,
                effects,
            }
        };
        runtime.apply_execution_patch(&patch).unwrap();
        let publication = runtime.take_renderer_publication();
        let visible = renderer
            .effect_source_visibility(&publication, &[1, 3])
            .unwrap();
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &publication,
                visible,
                TextDeviceMetrics::uniform(10.0).unwrap(),
            )
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text);
        let mut encoder = device.create_command_encoder(&Default::default());
        let work = renderer
            .prepare_retained_effects(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &text,
                &publication,
                4_000_000,
            )
            .unwrap();
        assert_eq!(work.source_passes, [3, 2, 0][step]);
        let output = target(&device, 1);
        renderer
            .encode_retained(
                &mut encoder,
                &output.create_view(&Default::default()),
                &prepared,
                &text,
                wgpu::Color {
                    r: 24.0 / 255.0,
                    g: 48.0 / 255.0,
                    b: 64.0 / 255.0,
                    a: 1.0,
                },
                None,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        let expected = reference(&images, silhouette, intensity);
        let witness = maximum_error(&expected, &reference(&images, !silhouette, intensity));
        assert!(
            witness > 5,
            "original-mask and filtered-alpha oracles must disagree"
        );
        let error = maximum_error(&read(&device, &queue, &output), &expected);
        assert!(
            error <= 2,
            "nested mask step={step}: max byte error {error}"
        );
        eprintln!("nested mask step={step}: max byte error {error}, wrong-mask witness {witness}");
    }
}

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

fn mask_layers(images: &[Vec<u8>; 2], silhouette: bool, large_first: bool) -> [Vec<u8>; 3] {
    let mut layers = reference_layers(images, silhouette);
    if large_first {
        layers[0].fill(0);
        for y in 15..45 {
            for x in 15..55 {
                // A binary-exact quarter-scale image covers 40x30 pixels.
                // Resolve nearest sampling using rational pixel centers.
                let tx = (2 * (x - 15) + 1) * IMAGE_SIZE[0] / 80;
                let ty = (2 * (y - 15) + 1) * IMAGE_SIZE[1] / 60;
                let pixel = &images[0][(ty * IMAGE_SIZE[0] + tx) * 4..][..4];
                let alpha = f64::from(pixel[3]) / 255.0 * if silhouette { 0.25 } else { 0.125 };
                let at = (y * SIZE[0] as usize + x) * 4;
                over(
                    &mut layers[0][at..at + 4],
                    [
                        f64::from(pixel[0]) / 255.0 * alpha,
                        f64::from(pixel[1]) / 255.0 * alpha,
                        f64::from(pixel[2]) / 255.0 * alpha,
                        alpha,
                    ],
                );
            }
        }
    }
    layers
}

fn reference(
    images: &[Vec<u8>; 2],
    silhouette: bool,
    intensity: f64,
    child_opacity: f64,
    child_blur: bool,
    child_mask: Option<(noon_core::NoiseDissolve, bool)>,
    large_first: bool,
) -> Vec<u8> {
    let apply_mask = |pixels: Vec<u8>, original: bool| {
        if let Some((definition, _)) = child_mask.filter(|(_, private)| !original || *private) {
            crate::gpu::dissolve_filter::tests::reference(
                &pixels,
                crate::DissolveParameters {
                    definition,
                    // Quarter-scale image: [15,15]..[55,45]. Six pixels of
                    // Blur support plus one coverage fringe give this exact,
                    // even-aligned source domain; no GPU tile data is an input.
                    source_bounds: crate::EffectPixelBounds {
                        min: [8.0, 8.0],
                        max: [62.0, 52.0],
                    },
                    scope_opacity: 1.0,
                    scratch_budget_bytes: u64::MAX,
                },
                SIZE,
            )
        } else {
            pixels
        }
    };
    let apply_opacity = |pixels: Vec<u8>| {
        pixels
            .into_iter()
            .map(|value| (f64::from(value) * child_opacity).round() as u8)
            .collect::<Vec<_>>()
    };
    let mut layers = mask_layers(images, false, large_first);
    layers[0] = blur_with_opacity(&layers[0], 1.0);
    let inner = nested::source_over_layers(&layers[..2]);
    let inner = apply_opacity(apply_mask(
        if child_blur {
            blur_with_opacity(&inner, 1.0)
        } else {
            inner
        },
        false,
    ));
    let source = nested::source_over_layers(&[inner, layers[2].clone()]);
    let mask = if silhouette {
        let original = mask_layers(images, true, large_first);
        nested::source_over_layers(&[
            apply_opacity(apply_mask(nested::source_over_layers(&original[..2]), true)),
            original[2].clone(),
        ])
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
    let mut warm_bytes = 0;
    let mut mask_warm_bytes = 0;
    let mut partial_private = None;
    let mut empty_private = None;
    for step in 0..24 {
        let silhouette = step > 0;
        let intensity = if step == 10 || step == 11 {
            0.0
        } else if step >= 9 {
            1.0
        } else if step >= 2 {
            1.25
        } else {
            1.5
        };
        let child_blur = step < 5;
        let child_opacity = match step {
            3 | 5 => 0.5,
            4 => 0.0,
            8..=10 => 0.25,
            11..=23 => 0.75,
            _ => 1.0,
        };
        if step >= 3 {
            let child = ObjectId::new(70);
            let mut style = runtime
                .frame()
                .objects
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .style;
            style.opacity = child_opacity as f32;
            runtime
                .apply_execution_patch(&ExecutionPatch::SetStyle {
                    object: child,
                    style,
                    fill_paint: None,
                })
                .unwrap();
        }
        if step == 5 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: ObjectId::new(70),
                    expected: vec![ATTACHMENT].into_boxed_slice(),
                    effects: None,
                })
                .unwrap();
        }
        if step == 14 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetTransform {
                    object: ObjectId::new(1),
                    transform: Transform2D {
                        translation: Vec2::new(-0.5, 0.0),
                        scale: Vec2::new(0.25, 0.25),
                        ..Transform2D::IDENTITY
                    },
                })
                .unwrap();
        }
        let child_mask = match step {
            14..=19 | 22..=23 => {
                let progress = match step {
                    14 | 19 => 0.0,
                    15 | 18 | 22 => 0.5,
                    17 => 0.25,
                    _ => 1.0,
                };
                Some((
                    noon_core::NoiseDissolve::new(7, 0.2)
                        .unwrap()
                        .with_progress(progress)
                        .unwrap(),
                    step < 20,
                ))
            }
            _ => None,
        };
        if let Some((definition, private)) = child_mask {
            let attachment = SemanticNodeId::new(if private { 95 } else { 96 }, 1);
            let stack = Arc::new(
                CompiledEffectStack::new_with_visibility(
                    [CompiledEffect {
                        attachment,
                        definition: definition.into(),
                    }],
                    private.then_some(attachment),
                )
                .unwrap(),
            );
            let patch = if step == 14 || step == 22 {
                ExecutionPatch::SetEffectStack {
                    object: ObjectId::new(70),
                    expected: Box::new([]),
                    effects: Some(stack),
                }
            } else {
                ExecutionPatch::SetEffects {
                    object: ObjectId::new(70),
                    effects: stack,
                }
            };
            runtime.apply_execution_patch(&patch).unwrap();
        } else if step == 20 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: ObjectId::new(70),
                    expected: vec![SemanticNodeId::new(95, 1)].into_boxed_slice(),
                    effects: None,
                })
                .unwrap();
        }
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
        // Switching to an original mask also captures the child's original
        // coverage. Later child opacity edits reuse both child images and only
        // recapture the two parent streams.
        eprintln!("nested work step={step}: {work:?}");
        assert_eq!(
            work.source_passes,
            [3, 4, 0, 2, 2, 4, 2, 0, 4, 0, 0, 1, 4, 0, 5, 2, 2, 2, 2, 2, 0, 0, 2, 2][step]
        );
        if step == 15 {
            mask_warm_bytes = work.retained_texture_bytes;
        }
        if (16..=19).contains(&step) {
            assert_eq!(work.texture_allocations, 0);
            assert_eq!(work.retained_texture_bytes, mask_warm_bytes);
        }
        if step == 20 {
            // Both pointwise destinations cover at least the literal 54x44
            // source domain. Retiring the last effect releases both while
            // the child's independent group-opacity capture stays active.
            assert!(mask_warm_bytes - work.retained_texture_bytes >= 2 * 54 * 44 * 4);
        }
        if step == 1 {
            warm_bytes = work.retained_texture_bytes;
        }
        if (2..=4).contains(&step) {
            assert_eq!(work.texture_allocations, 0);
            assert_eq!(work.retained_texture_bytes, warm_bytes);
        }
        if step == 7 || step == 13 || step == 21 {
            assert_eq!(
                (
                    work.sources_prepared,
                    work.source_passes,
                    work.blur_passes,
                    work.composite_passes,
                    work.bytes_uploaded,
                    work.texture_allocations,
                    work.rows_examined
                ),
                (0, 0, 0, 0, 0, 0, 0)
            );
        }
        if step > 0 {
            assert_eq!(prepared.geometry.stats.full_rebuilds, 0);
        }
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
        let expected = reference(
            &images,
            silhouette,
            intensity,
            child_opacity,
            child_blur,
            child_mask,
            step >= 14,
        );
        let witness = maximum_error(
            &expected,
            &reference(
                &images,
                !silhouette,
                intensity,
                child_opacity,
                child_blur,
                child_mask,
                step >= 14,
            ),
        );
        if child_opacity == 0.0
            || intensity == 0.0
            || child_mask.is_some_and(|(mask, private)| private && mask.progress() == 1.0)
        {
            assert_eq!(
                witness, 0,
                "zero contribution has identical rendered mask references"
            );
        } else {
            assert!(
                witness > 5,
                "original-mask and filtered-alpha oracles must disagree"
            );
        }
        if child_opacity != 1.0
            && !child_mask.is_some_and(|(mask, private)| private && mask.progress() == 1.0)
        {
            assert!(
                maximum_error(
                    &expected,
                    &reference(
                        &images,
                        silhouette,
                        intensity,
                        1.0,
                        child_blur,
                        child_mask,
                        step >= 14
                    )
                ) > 5,
                "ignoring child opacity must change the independent reference"
            );
        }
        let actual = read(&device, &queue, &output);
        match step {
            15 => partial_private = Some(actual.clone()),
            16 => empty_private = Some(actual.clone()),
            18 => assert_eq!(
                Some(&actual),
                partial_private.as_ref(),
                "same-time reverse progress is byte-identical"
            ),
            22 => assert!(
                maximum_error(&actual, partial_private.as_ref().unwrap()) > 5,
                "named and private masks have distinct ancestor coverage"
            ),
            23 => assert!(
                maximum_error(&actual, empty_private.as_ref().unwrap()) > 5,
                "named paint filtering must preserve original coverage"
            ),
            _ => {}
        }
        let error = maximum_error(&actual, &expected);
        assert!(
            error <= 2,
            "nested mask step={step}: max byte error {error}"
        );
        eprintln!("nested mask step={step}: max byte error {error}, wrong-mask witness {witness}");
    }
}

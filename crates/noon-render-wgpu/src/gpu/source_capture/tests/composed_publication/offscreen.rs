use super::*;

// Independent full 2D Gaussian on an extended physical-pixel field. The output
// viewport is cropped only after both convolutions, never between filters.
fn gaussian(source: &[u8], size: [usize; 2], sigma: f64) -> Vec<u8> {
    let radius = (3.0 * sigma).ceil() as i32;
    let mut taps = Vec::new();
    let mut mass = 0.0;
    for y in -radius..=radius {
        for x in -radius..=radius {
            let weight = (-f64::from(x * x + y * y) / (2.0 * sigma * sigma)).exp();
            mass += weight;
            taps.push((x, y, weight));
        }
    }
    let mut output = vec![0; source.len()];
    for y in 0..size[1] as i32 {
        for x in 0..size[0] as i32 {
            let mut value = [0.0; 4];
            for &(dx, dy, weight) in &taps {
                let xx = x + dx;
                let yy = y + dy;
                if xx >= 0 && yy >= 0 && xx < size[0] as i32 && yy < size[1] as i32 {
                    for channel in 0..4 {
                        value[channel] +=
                            f64::from(source[(yy as usize * size[0] + xx as usize) * 4 + channel])
                                * weight
                                / mass;
                    }
                }
            }
            for channel in 0..4 {
                output[(y as usize * size[0] + x as usize) * 4 + channel] =
                    value[channel].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    output
}

fn reference() -> Vec<u8> {
    const PAD: usize = 24;
    let size = [SIZE[0] as usize + 2 * PAD, SIZE[1] as usize + 2 * PAD];
    let mut source = vec![0; size[0] * size[1] * 4];
    for y in 26 + PAD..34 + PAD {
        for x in PAD - 9..PAD - 5 {
            over(
                &mut source[(y * size[0] + x) * 4..][..4],
                [0.6, 0.15, 0.075, 0.6],
            );
        }
    }
    let filtered = gaussian(&gaussian(&source, size, 1.0), size, 3.0);
    (PAD..PAD + SIZE[1] as usize)
        .flat_map(|y| {
            filtered[(y * size[0] + PAD) * 4..(y * size[0] + PAD + SIZE[0] as usize) * 4]
                .iter()
                .copied()
        })
        .collect()
}

fn qualify(device: &wgpu::Device, queue: &wgpu::Queue, raster: bool) {
    let mut object = CompiledObject::new(
        ObjectId::new(1),
        GeometryRef::rectangle(0.4, 0.8),
        Transform2D {
            translation: Vec2::new(-4.7, 0.0),
            ..Transform2D::IDENTITY
        },
        Style {
            fill: Some(Color::rgba(1.0, 0.25, 0.125, 0.75)),
            stroke: None,
            opacity: 0.8,
            ..Style::default()
        },
    );
    object.effects = Some(effects(1.0));
    let mut scope = CompiledObject::new(
        OWNER,
        ObjectContentRef::Composed(Arc::new(ComposedSource::new([object.id], None).unwrap())),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            ..Style::default()
        },
    );
    scope.draw_enabled = false;
    scope.effects = Some(effects(0.0));
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![object, scope], &[]).unwrap());
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(device, queue);
    let mut preparer = RetainedFramePreparer::new();
    let expected = raster.then(reference);
    if let Some(expected) = &expected {
        assert!(
            expected.iter().copied().max().unwrap() > 4,
            "fixture must have a visible halo from a wholly offscreen child"
        );
    }
    for step in 0..5 {
        if matches!(step, 1 | 3 | 4) {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffects {
                    object: OWNER,
                    effects: effects(if step == 3 { 0.0 } else { 3.0 }),
                })
                .unwrap();
        }
        let publication = runtime.take_renderer_publication();
        let visible = renderer
            .effect_source_visibility(&publication, &[])
            .unwrap();
        let prepared = preparer
            .prepare_planned_publication_visible(
                device,
                &publication,
                visible,
                TextDeviceMetrics::uniform(10.0).unwrap(),
            )
            .unwrap();
        renderer.upload_retained(device, queue, &prepared, &mut text);
        let mut encoder = device.create_command_encoder(&Default::default());
        let work = renderer
            .prepare_retained_effects(
                device,
                queue,
                &mut encoder,
                &prepared,
                &text,
                &publication,
                2_000_000,
            )
            .unwrap();
        assert_eq!(
            work.source_passes,
            [0, 2, 0, 0, 0][step],
            "offscreen demand/neutral reuse at step {step}"
        );
        if step == 0 {
            assert_eq!(
                work.retained_texture_bytes, 0,
                "neutral parent must not demand wholly offscreen child storage"
            );
        }
        let output = target(device, 1);
        renderer
            .encode_retained(
                &mut encoder,
                &output.create_view(&Default::default()),
                &prepared,
                &text,
                wgpu::Color::TRANSPARENT,
                None,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        if let Some(expected) = &expected {
            let expected = if step == 0 || step == 3 {
                vec![0; expected.len()]
            } else {
                expected.clone()
            };
            let error = maximum_error(&read(device, queue, &output), &expected);
            assert!(
                error <= 2,
                "offscreen intermediate step={step}: max byte error {error}"
            );
            eprintln!("offscreen intermediate step={step}: max byte error {error}");
        }
    }
}

#[test]
#[cfg(feature = "ci-noop")]
fn offscreen_child_is_prepared_only_when_its_parent_demands_it() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    qualify(&device, &queue, false);
}

#[test]
#[ignore = "requires a real native raster adapter; extended-field offscreen oracle"]
fn offscreen_child_pixels_contribute_through_the_parent_filter() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("offscreen input raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    qualify(&device, &queue, true);
}

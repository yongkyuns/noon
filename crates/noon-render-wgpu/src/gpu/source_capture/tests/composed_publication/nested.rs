//! Literal composition and full two-dimensional convolution, independent of
//! renderer tile plans, packed uniforms and intermediate GPU images.
use super::*;

const INNER: ObjectId = ObjectId::new(70);

fn replace_effect(runtime: &mut SceneInstance, object: ObjectId, radius: f64) {
    let patch = if runtime
        .frame()
        .objects
        .iter()
        .find(|row| row.id == object)
        .unwrap()
        .effects
        .is_some()
    {
        ExecutionPatch::SetEffects {
            object,
            effects: effects(radius),
        }
    } else {
        ExecutionPatch::SetEffectStack {
            object,
            expected: Box::new([]),
            effects: Some(effects(radius)),
        }
    };
    runtime.apply_execution_patch(&patch).unwrap();
}

pub(super) fn nested_fixture(equal: bool) -> (SceneInstance, [Vec<u8>; 2]) {
    let (mut runtime, pixels) = grouped_fixture();
    let mut inner = CompiledObject::new(
        INNER,
        ObjectContentRef::Composed(Arc::new(
            ComposedSource::new(
                [ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)][..if equal { 3 } else { 2 }]
                    .iter()
                    .copied(),
                Some(OWNER),
            )
            .unwrap(),
        )),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            ..Style::default()
        },
    );
    inner.draw_enabled = false;
    inner.effects = Some(effects(2.0));
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(inner))
        .unwrap();
    replace_effect(&mut runtime, ObjectId::new(1), 2.0);
    (runtime, pixels)
}

pub(super) fn source_over_layers(layers: &[Vec<u8>]) -> Vec<u8> {
    let mut output = vec![0; layers[0].len()];
    for layer in layers {
        for (out, pixel) in output
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(layer.as_chunks::<4>().0)
        {
            over(out, pixel.map(|value| f64::from(value) / 255.0));
        }
    }
    output
}

fn nested_reference(
    images: &[Vec<u8>; 2],
    equal: bool,
    leaf: bool,
    inner: bool,
    root: bool,
) -> Vec<u8> {
    let mut layers = reference_layers(images, false);
    if leaf {
        layers[0] = blur_with_opacity(&layers[0], 1.0);
    }
    let count = if equal { 3 } else { 2 };
    let mut inside = source_over_layers(&layers[..count]);
    if inner {
        inside = blur_with_opacity(&inside, 1.0);
    }
    let mut source = if equal {
        inside
    } else {
        source_over_layers(&[inside, layers[2].clone()])
    };
    if root {
        source = blur_with_opacity(&source, 1.0);
    }
    let mut output = source_over_layers(&[
        [24, 48, 64, 255].repeat((SIZE[0] * SIZE[1]) as usize),
        source,
    ]);
    for y in 28..32 {
        for x in 37..43 {
            let at = (y * SIZE[0] as usize + x) * 4;
            over(&mut output[at..at + 4], [0.6, 0.45, 0.075, 0.6]);
        }
    }
    output
}

fn qualify_nested(device: &wgpu::Device, queue: &wgpu::Queue, raster: bool) {
    for equal in [false, true] {
        let (mut runtime, images) = nested_fixture(equal);
        let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
        renderer.set_camera(
            queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
        );
        let mut text = renderer.create_retained_text_state(device, queue);
        let mut preparer = RetainedFramePreparer::new();
        for step in 0..6 {
            match step {
                2 => replace_effect(&mut runtime, INNER, 0.0),
                3 => {
                    replace_effect(&mut runtime, OWNER, 0.0);
                    replace_effect(&mut runtime, INNER, 2.0);
                }
                4 => replace_effect(&mut runtime, ObjectId::new(1), 0.0),
                5 => {
                    replace_effect(&mut runtime, OWNER, 2.0);
                    replace_effect(&mut runtime, ObjectId::new(1), 2.0);
                }
                _ => {}
            }
            let publication = runtime.take_renderer_publication();
            let visible = renderer
                .effect_source_visibility(&publication, &[1, 3])
                .unwrap();
            let prepared = preparer
                .prepare_planned_publication_visible(
                    device,
                    &publication,
                    visible,
                    TextDeviceMetrics::uniform(10.0)
                        .unwrap()
                        .with_world_origin_pixels(Vec2::new(40.0, 30.0))
                        .unwrap(),
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
                    4_000_000,
                )
                .unwrap();
            if step == 1 {
                assert_eq!(
                    (
                        work.sources_prepared,
                        work.source_passes,
                        work.bytes_uploaded
                    ),
                    (0, 0, 0)
                );
            }
            assert_eq!(
                work.source_passes,
                [3, 0, 1, 0, 1, 2][step],
                "only the changed input closure is captured; equal={equal}, step={step}"
            );
            if step > 0 {
                assert_eq!(prepared.geometry.stats.full_rebuilds, 0);
            }
            let output = target(device, 1);
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
            if raster {
                let expected =
                    nested_reference(&images, equal, step != 4, step != 2, step != 3 && step != 4);
                let error = maximum_error(&read(device, queue, &output), &expected);
                assert!(
                    error <= 2,
                    "nested equal={equal}, step={step}: max byte error {error}"
                );
                let wrong = nested_reference(&images, equal, false, false, step != 3 && step != 4);
                if step == 0 {
                    assert!(
                        maximum_error(&wrong, &expected) > 10,
                        "reference must distinguish raw from filtered child inputs"
                    );
                }
                eprintln!("nested equal={equal}, step={step}: max byte error {error}");
            }
        }
    }
}

#[test]
#[cfg(feature = "ci-noop")]
fn nested_group_inputs_preserve_order_and_reuse_only_unchanged_captures() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    qualify_nested(&device, &queue, false);
}

#[test]
#[ignore = "requires a real native raster adapter; nested execution capture profile"]
fn nested_group_pixels_match_independent_filtered_composition() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("nested capture raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    qualify_nested(&device, &queue, true);
}

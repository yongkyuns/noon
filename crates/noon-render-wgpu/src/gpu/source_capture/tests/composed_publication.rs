//! Actual execution publication -> capture -> whole-interval painter proof.
//! The flat, fully present profile is deliberately narrower than #1897 closure.
use super::*;
use noon_compile::{
    CompiledEffect, CompiledEffectStack, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{ComposedSource, ObjectContentRef, SemanticNodeId};
use std::sync::Arc;

const OWNER: ObjectId = ObjectId::new(90);
const ATTACHMENT: SemanticNodeId = SemanticNodeId::new(91, 2);

mod nested;
mod offscreen;
mod silhouette;

fn effects(radius: f64) -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: ATTACHMENT,
            definition: Blur::new(EffectUpdate::default().radius(Pixels(radius)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    )
}

fn grouped_fixture() -> (SceneInstance, [Vec<u8>; 2]) {
    let (mut runtime, pixels) = fixture();
    let mut foreground = CompiledObject::new(
        ObjectId::new(80),
        GeometryRef::rectangle(0.6, 0.4),
        Transform2D::IDENTITY,
        Style {
            fill: Some(Color::rgba(1.0, 0.75, 0.125, 0.6)),
            stroke: None,
            ..Style::default()
        },
    );
    foreground.base_z_index = 1.0;
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(foreground))
        .unwrap();
    let mut scope = CompiledObject::new(
        OWNER,
        ObjectContentRef::Composed(Arc::new(
            ComposedSource::new([ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)], None)
                .unwrap(),
        )),
        Transform2D::IDENTITY,
        Style {
            fill: None,
            stroke: None,
            ..Style::default()
        },
    );
    scope.draw_enabled = false;
    scope.effects = Some(effects(2.0));
    runtime
        .apply_execution_patch(&ExecutionPatch::CreateObject(scope))
        .unwrap();
    (runtime, pixels)
}

fn reference(pixels: &[Vec<u8>; 2], filtered: bool) -> Vec<u8> {
    let source = compose(&reference_layers(pixels, false));
    let source = if filtered {
        blur_with_opacity(&source, 1.0)
    } else {
        source
    };
    let mut output = [24, 48, 64, 255].repeat((SIZE[0] * SIZE[1]) as usize);
    for (out, source) in output
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(source.as_chunks::<4>().0)
    {
        over(out, source.map(|value| f64::from(value) / 255.0));
    }
    for y in 28..32 {
        for x in 37..43 {
            let at = (y * SIZE[0] as usize + x) * 4;
            over(&mut output[at..at + 4], [0.6, 0.45, 0.075, 0.6]);
        }
    }
    output
}

fn qualify_composed_publication(device: &wgpu::Device, queue: &wgpu::Queue, pixels: bool) {
    let (mut runtime, images) = grouped_fixture();
    // The two priority writes have one valid final interval. Controller metadata
    // must stay out of that interval during runtime ordering and source capture.
    runtime
        .apply_execution_transaction(&ExecutionMutationTransaction::from_mutations([
            ExecutionPatch::SetZIndex {
                object: ObjectId::new(2),
                value: 0.25,
            },
            ExecutionPatch::SetZIndex {
                object: ObjectId::new(3),
                value: 0.5,
            },
        ]))
        .unwrap();
    let mut renderer = GpuRenderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    let mut text = renderer.create_retained_text_state(device, queue);
    let mut preparer = RetainedFramePreparer::new();
    for step in 0..5 {
        if (2..=3).contains(&step) {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffects {
                    object: OWNER,
                    effects: effects(if step == 2 { 0.0 } else { 2.0 }),
                })
                .unwrap();
        } else if step == 4 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: OWNER,
                    expected: vec![ATTACHMENT].into_boxed_slice(),
                    effects: None,
                })
                .unwrap();
        }
        let publication = runtime.take_renderer_publication();
        // Deliberately omit both images and the controller: the shared renderer
        // must recover the complete source interval through the canonical index.
        let visible = [1, 3];
        let expanded = renderer
            .effect_source_visibility(&publication, &visible)
            .unwrap();
        if step < 4 {
            assert_eq!(expanded, &[0, 1, 2, 3]);
        }
        let prepared = preparer
            .prepare_planned_publication_visible(
                device,
                &publication,
                if step == 4 { &[0, 1, 2, 3] } else { expanded },
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
                1_000_000,
            )
            .unwrap();
        if step == 0 {
            assert_eq!(work.sources_prepared, 1);
            assert_eq!(
                work.source_passes, 1,
                "one composed capture, never one per child"
            );
        } else {
            assert_eq!(
                work.source_passes, 0,
                "neutral/reactivation and parameter-only publications reuse the input"
            );
        }
        if step == 1 {
            assert_eq!(
                (
                    work.rows_examined,
                    work.sources_prepared,
                    work.bytes_uploaded
                ),
                (0, 0, 0)
            );
        }
        if step == 4 {
            assert_eq!(renderer.effect_capture_texture_bytes(), 0);
        }
        let output = target(device, 1);
        let draw = renderer
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
        if pixels {
            let expected = reference(&images, step != 2 && step != 4);
            let error = maximum_error(&read(device, queue, &output), &expected);
            assert!(
                error <= 2,
                "composed publication step {step}: max byte error {error}"
            );
            eprintln!("flat composed publication step {step}: max byte error {error}");
        }
        assert_eq!(
            draw.images,
            if step == 2 || step == 4 { 2 } else { 0 },
            "active isolation replaces all original image draws"
        );
    }
}

#[test]
#[cfg(feature = "ci-noop")]
fn flat_group_publication_reuses_capture_and_replaces_the_whole_painter_interval() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    qualify_composed_publication(&device, &queue, false);
}

#[test]
#[cfg(feature = "ci-noop")]
fn stale_interleaved_and_unqualified_group_profiles_fail_before_encoding() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    for case in 0..7 {
        let (mut runtime, _) = grouped_fixture();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        {
            let publication = runtime.take_renderer_publication();
            renderer
                .effect_source_visibility(&publication, &[0, 1, 2, 3])
                .unwrap();
        }
        let patch = match case {
            0 => ExecutionPatch::SetZIndex {
                object: ObjectId::new(2),
                value: 2.0,
            },
            1 => ExecutionPatch::SetTransform {
                object: OWNER,
                transform: Transform2D {
                    translation: Vec2::new(0.5, 0.0),
                    ..Transform2D::IDENTITY
                },
            },
            2 => ExecutionPatch::SetStyle {
                object: OWNER,
                style: Style {
                    opacity: 0.5,
                    fill: None,
                    stroke: None,
                    ..Style::default()
                },
                fill_paint: None,
            },
            6 => ExecutionPatch::SetDrawEnabled {
                object: ObjectId::new(2),
                enabled: false,
            },
            _ => {
                let members = if case == 3 {
                    vec![ObjectId::new(1), ObjectId::new(3)]
                } else if case == 4 {
                    vec![ObjectId::new((1_u64 << 32) | 1)]
                } else {
                    vec![ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)]
                };
                ExecutionPatch::SetContent {
                    object: OWNER,
                    content: ObjectContentRef::Composed(Arc::new(
                        ComposedSource::new(members, (case == 5).then_some(OWNER)).unwrap(),
                    )),
                    text_bounds: None,
                }
            }
        };
        if matches!(case, 0 | 1 | 3 | 4 | 5) {
            let before = runtime.frame().clone();
            assert!(
                runtime.apply_execution_patch(&patch).is_err(),
                "invalid source graphs reject at the compiled-publication barrier"
            );
            assert_eq!(runtime.frame(), &before);
            continue;
        }
        runtime.apply_execution_patch(&patch).unwrap();
        let publication = runtime.take_renderer_publication();
        if case == 2 {
            assert_eq!(
                renderer
                    .effect_source_visibility(&publication, &[0, 1, 2, 3])
                    .unwrap(),
                &[0, 1, 2, 3],
                "final scope opacity is now admitted; native pixel oracles qualify its composition"
            );
            continue;
        }
        assert!(
            renderer
                .effect_source_visibility(&publication, &[0, 1, 2, 3])
                .is_err(),
            "unqualified group profile {case} must not silently paint raw children"
        );
        // Failed renderer admission leaves its successful epoch authoritative;
        // a corrected publication must rebuild the disposable index and retry.
        let restore = if case == 6 {
            ExecutionPatch::SetDrawEnabled {
                object: ObjectId::new(2),
                enabled: true,
            }
        } else {
            ExecutionPatch::SetStyle {
                object: OWNER,
                style: Style {
                    fill: None,
                    stroke: None,
                    ..Style::default()
                },
                fill_paint: None,
            }
        };
        runtime.apply_execution_patch(&restore).unwrap();
        assert!(renderer
            .effect_source_visibility(&runtime.take_renderer_publication(), &[0, 1, 2, 3])
            .is_ok());
        if case == 6 {
            // Inert containment metadata must not impose capture-only limits
            // on the ordinary no-effect scene after its attachment retires.
            for patch in [
                ExecutionPatch::SetEffectStack {
                    object: OWNER,
                    expected: vec![ATTACHMENT].into_boxed_slice(),
                    effects: None,
                },
                ExecutionPatch::SetStyle {
                    object: OWNER,
                    style: Style {
                        fill: None,
                        stroke: None,
                        opacity: 0.5,
                        ..Style::default()
                    },
                    fill_paint: None,
                },
                ExecutionPatch::SetDrawEnabled {
                    object: ObjectId::new(2),
                    enabled: false,
                },
            ] {
                runtime.apply_execution_patch(&patch).unwrap();
            }
            assert!(renderer.effect_source_visibility(&runtime.take_renderer_publication(), &[0, 2, 3]).is_err(),
                "partial opacity still requires drawable capture inputs after the last effect retires");
            runtime
                .apply_execution_patch(&ExecutionPatch::SetStyle {
                    object: OWNER,
                    style: Style {
                        fill: None,
                        stroke: None,
                        ..Style::default()
                    },
                    fill_paint: None,
                })
                .unwrap();
            assert!(renderer
                .effect_source_visibility(&runtime.take_renderer_publication(), &[0, 2, 3])
                .is_ok());
            assert!(!renderer.has_retained_effect_sources());
            runtime
                .apply_execution_patch(&ExecutionPatch::SetStyle {
                    object: OWNER,
                    style: Style {
                        fill: None,
                        stroke: None,
                        ..Style::default()
                    },
                    fill_paint: None,
                })
                .unwrap();
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: OWNER,
                    expected: Box::new([]),
                    effects: Some(effects(2.0)),
                })
                .unwrap();
            assert!(renderer.effect_source_visibility(&runtime.take_renderer_publication(), &[0,2,3]).is_err(), "reattachment must revalidate hidden inputs even when the immutable plan is unchanged");
        }
    }
}

#[test]
#[ignore = "requires a real native raster adapter; flat execution profile only"]
fn flat_group_pixels_match_independent_composition_filter_and_foreground_order() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("group raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    qualify_composed_publication(&device, &queue, true);
}

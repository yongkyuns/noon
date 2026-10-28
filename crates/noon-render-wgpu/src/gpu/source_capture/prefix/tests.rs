use super::super::tests as oracle;
use super::*;
use crate::{text::TextDeviceMetrics, Camera2D, RetainedFramePreparer};
use noon_compile::{CompiledEffect, CompiledEffectStack, CompiledObject, ExecutionPatch};
use noon_core::{
    Blur, Color, ComposedSource, EffectUpdate, GeometryRef, ObjectId, Pixels, SemanticNodeId,
    Style, Transform2D, Vec2,
};
use noon_runtime::SceneInstance;

const SIZE: [u32; 2] = [80, 60];
const RGBA: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BGRA: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const BUDGET: u64 = 80 * 60 * 20;
fn fixture() -> (SceneInstance, [Vec<u8>; 2]) {
    let (mut runtime, images) = oracle::fixture();
    for (id, fill) in [
        (4, Color::rgba(0.0, 1.0, 0.0, 1.0)),
        (5, Color::rgba(1.0, 0.0, 1.0, 1.0)),
    ] {
        runtime
            .apply_execution_patch(&ExecutionPatch::CreateObject(CompiledObject::new(
                ObjectId::new(id),
                GeometryRef::rectangle(2.0, 1.5),
                Transform2D::IDENTITY,
                Style {
                    fill: Some(fill),
                    stroke: None,
                    ..Style::default()
                },
            )))
            .unwrap();
    }
    (runtime, images)
}
fn renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> GpuRenderer {
    let mut renderer = GpuRenderer::new(device, queue, format);
    renderer.set_viewport(device, queue, SIZE[0], SIZE[1]);
    renderer.set_camera(
        queue,
        Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
    );
    renderer
}
fn metrics() -> TextDeviceMetrics {
    TextDeviceMetrics::uniform(10.0)
        .unwrap()
        .with_world_origin_pixels(Vec2::new(40.0, 30.0))
        .unwrap()
}
fn request(ids: &[u64]) -> PainterPrefixCaptureRequest {
    PainterPrefixCaptureRequest {
        anchor: ObjectId::new(4),
        candidates: ids.iter().copied().map(ObjectId::new).collect(),
        source_revision: 1,
        origin: [0, 0],
        size: SIZE,
        clear_color: wgpu::Color {
            r: 24.0 / 255.0,
            g: 48.0 / 255.0,
            b: 64.0 / 255.0,
            a: 1.0,
        },
        scratch_budget_bytes: BUDGET,
    }
}
fn prepare(
    gpu: (&wgpu::Device, &wgpu::Queue),
    renderer: &GpuRenderer,
    text: &RetainedTextGpuState,
    prepared: &PreparedRetainedGpuFrame<'_>,
    filter: &mut PainterPrefixCaptureFilter,
    scope: &mut PainterPrefixCaptureScope,
    request: &PainterPrefixCaptureRequest,
) -> Result<EffectRasterStats, EffectPrepareError> {
    let (device, queue) = gpu;
    let mut encoder = device.create_command_encoder(&Default::default());
    let result = filter.prepare_encode(
        device,
        queue,
        &mut encoder,
        renderer,
        text,
        prepared,
        scope,
        request,
    );
    if result.is_ok() {
        queue.submit([encoder.finish()]);
    }
    result
}
#[test]
fn explicit_prefix_selection_preserves_isolation_guards_and_rejects_bad_requests_atomically() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let (mut runtime, _) = fixture();
    let publication = runtime.take_renderer_publication();
    let mut preparer = RetainedFramePreparer::new();
    let prepared = preparer
        .prepare_planned_publication_visible(&device, &publication, &[0, 1, 2, 3, 4], metrics())
        .unwrap();
    let mut renderer = renderer(&device, &queue, RGBA);
    let mut text = renderer.create_retained_text_state(&device, &queue);
    renderer.upload_retained(&device, &queue, &prepared, &mut text);
    let mut filter = PainterPrefixCaptureFilter::default();
    let mut scope = PainterPrefixCaptureScope::default();
    let mut p = request(&[1, 2, 3]);
    p.scratch_budget_bytes = 0;
    assert_eq!(
        prepare(
            (&device, &queue),
            &renderer,
            &text,
            &prepared,
            &mut filter,
            &mut scope,
            &p
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert!(scope.output().is_none());
    assert!(filter.placement.is_none());
    let p = request(&[1, 3]);
    let first = prepare(
        (&device, &queue),
        &renderer,
        &text,
        &prepared,
        &mut filter,
        &mut scope,
        &p,
    )
    .unwrap();
    assert_eq!(
        (
            first.pipeline_compiles,
            first.texture_allocations,
            first.buffer_allocations,
            first.composite_passes,
            first.scratch_bytes
        ),
        (0, 2, 2, 1, BUDGET)
    );
    let selection = [(ObjectId::new(1), 1.0), (ObjectId::new(3), 1.0)];
    let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
    assert!(
        PreparedSource::prepare(&prepared, &selection, false, alignment).is_none(),
        "spatial gaps must not become admissible ordinary isolated groups"
    );
    let output = scope.output().unwrap().clone();
    let revision = scope.revision();
    let mut other = crate::gpu::source_capture::prefix::tests::renderer(&device, &queue, RGBA);
    let mut other_text = other.create_retained_text_state(&device, &queue);
    other.upload_retained(&device, &queue, &prepared, &mut other_text);
    assert_eq!(
        prepare(
            (&device, &queue),
            &other,
            &other_text,
            &prepared,
            &mut filter,
            &mut scope,
            &p
        ),
        Err(EffectPrepareError::PublicationMismatch)
    );
    assert_eq!(scope.output(), Some(&output));
    assert_eq!(scope.revision(), revision);
    let mut fresh = PainterPrefixCaptureScope::default();
    assert!(prepare(
        (&device, &queue),
        &other,
        &other_text,
        &prepared,
        &mut filter,
        &mut fresh,
        &p
    )
    .is_ok());
    let mut invalid = Vec::new();
    for ids in [&[1, 1][..], &[3, 1], &[1, 4], &[1, 5], &[999]] {
        invalid.push((request(ids), EffectPrepareError::PublicationMismatch));
    }
    let mut bad = p.clone();
    bad.anchor = ObjectId::new(999);
    invalid.push((bad, EffectPrepareError::PublicationMismatch));
    let mut bad = p.clone();
    bad.size[0] = 0;
    invalid.push((bad, EffectPrepareError::InvalidProjection));
    let mut bad = p.clone();
    bad.size[1] = device.limits().max_texture_dimension_2d + 1;
    invalid.push((bad, EffectPrepareError::ExtentExceedsDevice));
    let mut bad = p.clone();
    bad.origin[0] = i32::MIN;
    invalid.push((bad, EffectPrepareError::CaptureCoordinatesOutOfRange));
    let mut bad = p.clone();
    bad.scratch_budget_bytes = BUDGET - 1;
    invalid.push((bad, EffectPrepareError::ScratchBudgetExceeded));
    for value in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        let mut bad = p.clone();
        bad.clear_color.r = value;
        invalid.push((bad, EffectPrepareError::InvalidCaptureClearColor));
    }
    let mut bad = p.clone();
    bad.clear_color.a = 0.0;
    invalid.push((bad, EffectPrepareError::InvalidCaptureClearColor));
    for (bad, error) in invalid {
        assert_eq!(
            prepare(
                (&device, &queue),
                &renderer,
                &text,
                &prepared,
                &mut filter,
                &mut scope,
                &bad
            ),
            Err(error)
        );
        assert_eq!(scope.output(), Some(&output));
        assert_eq!(scope.revision(), revision);
        assert!(!scope.invalidated);
    }
    let clean = EffectRasterStats {
        scratch_bytes: BUDGET,
        ..Default::default()
    };
    for _ in 0..32 {
        assert_eq!(
            prepare(
                (&device, &queue),
                &renderer,
                &text,
                &prepared,
                &mut filter,
                &mut scope,
                &p
            )
            .unwrap(),
            clean
        );
    }
    let mut p = p;
    p.source_revision += 1;
    let changed = prepare(
        (&device, &queue),
        &renderer,
        &text,
        &prepared,
        &mut filter,
        &mut scope,
        &p,
    )
    .unwrap();
    assert_eq!(
        changed,
        EffectRasterStats {
            composite_passes: 1,
            ..clean
        }
    );
    scope.invalidate();
    let retried = prepare(
        (&device, &queue),
        &renderer,
        &text,
        &prepared,
        &mut filter,
        &mut scope,
        &p,
    )
    .unwrap();
    assert_eq!(retried.texture_allocations, 0);
    assert_eq!(retried.composite_passes, 1);
    assert!(retried.bytes_uploaded > 0);
    assert_eq!(scope.output(), Some(&output));
    let mut resized = p.clone();
    resized.size = [40, 30];
    resized.scratch_budget_bytes = 40 * 30 * 20;
    let stats = prepare(
        (&device, &queue),
        &renderer,
        &text,
        &prepared,
        &mut filter,
        &mut scope,
        &resized,
    )
    .unwrap();
    assert_eq!(
        (
            stats.texture_allocations,
            stats.buffer_allocations,
            stats.scratch_bytes
        ),
        (2, 0, 40 * 30 * 20)
    );
    assert_ne!(scope.output(), Some(&output));
    scope.revision = u64::MAX;
    let before = scope.output().unwrap().clone();
    resized.source_revision += 1;
    assert_eq!(
        prepare(
            (&device, &queue),
            &renderer,
            &text,
            &prepared,
            &mut filter,
            &mut scope,
            &resized
        ),
        Err(EffectPrepareError::RevisionExhausted)
    );
    assert_eq!(scope.output(), Some(&before));
    scope.clear();
    assert!(scope.output().is_none());
    assert_eq!(scope.scratch_bytes(), 0);
    assert!(scope.revision().is_none());
}
fn expected(layers: &[Vec<u8>], clear: [u8; 4], origin: [i32; 2]) -> Vec<u8> {
    let mut result = clear.repeat((SIZE[0] * SIZE[1]) as usize);
    for layer in layers {
        for y in 0..SIZE[1] {
            for x in 0..SIZE[0] {
                let sx = x as i64 + i64::from(origin[0]);
                let sy = y as i64 + i64::from(origin[1]);
                if !(0..i64::from(SIZE[0])).contains(&sx) || !(0..i64::from(SIZE[1])).contains(&sy)
                {
                    continue;
                }
                let at = (y * SIZE[0] + x) as usize * 4;
                let input = (sy * i64::from(SIZE[0]) + sx) as usize * 4;
                oracle::over(
                    &mut result[at..at + 4],
                    std::array::from_fn(|c| f64::from(layer[input + c]) / 255.0),
                );
            }
        }
    }
    result
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture) -> Vec<u8> {
    let mut bytes = oracle::read(device, queue, t);
    if t.format() == BGRA {
        for p in bytes.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
    }
    bytes
}
fn retain(name: &str, actual: &[u8], reference: &[u8], p: &PainterPrefixCaptureRequest) {
    let Some(root) = std::env::var_os("NOON_PREFIX_ARTIFACT_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(root).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("actual.rgba"), actual).unwrap();
    std::fs::write(dir.join("reference.rgba"), reference).unwrap();
    std::fs::write(dir.join("profile.json"),format!(
        "{{\"schema\":1,\"capture_size\":{:?},\"origin\":{:?},\"anchor\":{},\"candidates\":{:?},\"clear_rgba\":{:?},\"color\":\"encoded-premultiplied-rgba8\",\"accepted_byte_error\":2}}\n",
        p.size,p.origin,p.anchor.get(),p.candidates.iter().map(|id|id.get()).collect::<Vec<_>>(),
        [p.clear_color.r,p.clear_color.g,p.clear_color.b,p.clear_color.a],
    )).unwrap();
}
#[test]
#[ignore = "requires a real native raster adapter; independent painter-prefix order, clear and cropping oracle"]
fn prefix_pixels_clear_selection_order_and_window_match_literal_layer_reference() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("prefix raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("prefix capture adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut profiles = 0;
    let mut checks = 0;
    let mut maximum = 0;
    let mut exact_empty = 0;
    for format in [RGBA, BGRA] {
        let (mut runtime, images) = fixture();
        let layers = oracle::reference_layers(&images, false);
        let publication = runtime.take_renderer_publication();
        let mut preparer = RetainedFramePreparer::new();
        let prepared = preparer
            .prepare_planned_publication_visible(&device, &publication, &[0, 1, 2, 3, 4], metrics())
            .unwrap();
        let mut renderer = renderer(&device, &queue, format);
        let mut text = renderer.create_retained_text_state(&device, &queue);
        renderer.upload_retained(&device, &queue, &prepared, &mut text);
        let mut filter = PainterPrefixCaptureFilter::default();
        let mut scope = PainterPrefixCaptureScope::default();
        for ids in [
            &[][..],
            &[1][..],
            &[2],
            &[3],
            &[1, 2],
            &[1, 3],
            &[2, 3],
            &[1, 2, 3],
        ] {
            for clear in [[0, 0, 0, 0], [24, 48, 64, 255], [16, 32, 48, 128]] {
                for origin in [[0, 0], [17, -13], [-19, 7], [80, 60], [-80, -60]] {
                    let mut p = request(ids);
                    p.origin = origin;
                    p.clear_color = wgpu::Color {
                        r: f64::from(clear[0]) / 255.0,
                        g: f64::from(clear[1]) / 255.0,
                        b: f64::from(clear[2]) / 255.0,
                        a: f64::from(clear[3]) / 255.0,
                    };
                    let cold = scope.output().is_none();
                    let stats = prepare(
                        (&device, &queue),
                        &renderer,
                        &text,
                        &prepared,
                        &mut filter,
                        &mut scope,
                        &p,
                    )
                    .unwrap();
                    assert_eq!(stats.texture_allocations, if cold { 2 } else { 0 });
                    let actual = read(&device, &queue, scope.output().unwrap());
                    let selected = ids
                        .iter()
                        .map(|id| layers[*id as usize - 1].clone())
                        .collect::<Vec<_>>();
                    let reference = expected(&selected, clear, origin);
                    let error = oracle::maximum_error(&actual, &reference);
                    maximum = maximum.max(error);
                    assert!(error<=2,"prefix {format:?} ids={ids:?} clear={clear:?} origin={origin:?}: error {error}");
                    checks += actual.len();
                    retain(
                        &format!("case-{profiles:04}-{format:?}"),
                        &actual,
                        &reference,
                        &p,
                    );
                    if ids.is_empty() || origin[0].unsigned_abs() == 80 {
                        assert_eq!(actual, reference);
                        exact_empty += 1;
                    }
                    assert_eq!(
                        prepare(
                            (&device, &queue),
                            &renderer,
                            &text,
                            &prepared,
                            &mut filter,
                            &mut scope,
                            &p
                        )
                        .unwrap(),
                        EffectRasterStats {
                            scratch_bytes: BUDGET,
                            ..Default::default()
                        }
                    );
                    profiles += 1;
                }
            }
        }
        // A real seeded wrong-clear capture exercises this reference and then
        // recovers through the normal retained operator, without a wider bound.
        let p = request(&[1, 2, 3]);
        prepare(
            (&device, &queue),
            &renderer,
            &text,
            &prepared,
            &mut filter,
            &mut scope,
            &p,
        )
        .unwrap();
        let reference = expected(&layers, [24, 48, 64, 255], [0, 0]);
        let mut broken = scope.source.as_ref().unwrap().clone();
        broken.clear_color = wgpu::Color::TRANSPARENT;
        let r = scope.resources.as_ref().unwrap();
        let mut encoder = device.create_command_encoder(&Default::default());
        broken
            .encode(
                &renderer,
                &text,
                &prepared.geometry,
                &mut encoder,
                &r.view,
                &r.multisampled,
                &r.camera_binding,
                &r.text_camera,
                &r.instances,
                &r.images,
                None,
                SIZE,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        let wrong = read(&device, &queue, scope.output().unwrap());
        let error = oracle::maximum_error(&wrong, &reference);
        assert!(error >= 64, "wrong-clear negative control escaped: {error}");
        retain(
            &format!("negative-clear-{format:?}"),
            &wrong,
            &reference,
            &p,
        );
        scope.invalidate();
        prepare(
            (&device, &queue),
            &renderer,
            &text,
            &prepared,
            &mut filter,
            &mut scope,
            &p,
        )
        .unwrap();
        assert!(
            oracle::maximum_error(&read(&device, &queue, scope.output().unwrap()), &reference) <= 2
        );
        eprintln!("prefix wrong-clear negative control {format:?}: byte error {error} (bound 2), exact source recovered");
    }
    assert_eq!(profiles, 240);
    eprintln!("prefix capture: {profiles} profiles, {checks} independent RGBA checks, max byte error {maximum}, {exact_empty} exact empty/offscreen images");
}
fn blur_stack() -> Arc<CompiledEffectStack> {
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(91, 2),
            definition: Blur::new(EffectUpdate::default().radius(Pixels(2.0)))
                .unwrap()
                .into(),
        }])
        .unwrap(),
    )
}
#[test]
#[ignore = "requires a real native raster adapter; earlier leaf and composed filter inputs and stale-prefix rejection"]
fn prefix_consumes_prior_leaf_and_whole_group_filters_without_double_opacity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("filtered prefix raster adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (mut runtime, images) = fixture();
    let layers = oracle::reference_layers(&images, false);
    let mut renderer = renderer(&device, &queue, RGBA);
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let mut preparer = RetainedFramePreparer::new();
    let mut filter = PainterPrefixCaptureFilter::default();
    let mut scope = PainterPrefixCaptureScope::default();
    for phase in 0..3 {
        if phase == 1 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: ObjectId::new(1),
                    expected: Box::default(),
                    effects: Some(blur_stack()),
                })
                .unwrap();
        } else if phase == 2 {
            runtime
                .apply_execution_patch(&ExecutionPatch::SetEffectStack {
                    object: ObjectId::new(1),
                    expected: vec![SemanticNodeId::new(91, 2)].into_boxed_slice(),
                    effects: None,
                })
                .unwrap();
            let mut group = CompiledObject::new(
                ObjectId::new(90),
                noon_core::ObjectContentRef::Composed(Arc::new(
                    ComposedSource::new(
                        [ObjectId::new(1), ObjectId::new(2), ObjectId::new(3)],
                        None,
                    )
                    .unwrap(),
                )),
                Transform2D::IDENTITY,
                Style {
                    fill: None,
                    stroke: None,
                    opacity: 0.5,
                    ..Style::default()
                },
            );
            group.draw_enabled = false;
            group.effects = Some(blur_stack());
            runtime
                .apply_execution_patch(&ExecutionPatch::CreateObject(group))
                .unwrap();
        }
        let publication = runtime.take_renderer_publication();
        let visible = (0..publication.frame().objects.len()).collect::<Vec<_>>();
        let expanded = renderer
            .effect_source_visibility(&publication, &visible)
            .unwrap();
        let prepared = preparer
            .prepare_planned_publication_visible(&device, &publication, expanded, metrics())
            .unwrap();
        renderer.upload_retained(&device, &queue, &prepared, &mut text);
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer
            .prepare_retained_effects(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &text,
                &publication,
                64 * 1024 * 1024,
            )
            .unwrap();
        let mut p = request(&[1, 2, 3]);
        p.source_revision = phase + 1;
        let stats = filter
            .prepare_encode(
                &device,
                &queue,
                &mut encoder,
                &renderer,
                &text,
                &prepared,
                &mut scope,
                &p,
            )
            .unwrap();
        queue.submit([encoder.finish()]);
        if phase == 1 {
            assert_eq!(stats.pipeline_compiles, 2);
        }
        let selected = match phase {
            0 => layers.to_vec(),
            1 => vec![
                oracle::blur_with_opacity(&layers[0], 1.0),
                layers[1].clone(),
                layers[2].clone(),
            ],
            _ => vec![oracle::blur_with_opacity(&oracle::compose(&layers), 0.5)],
        };
        let reference = expected(&selected, [24, 48, 64, 255], [0, 0]);
        let actual = read(&device, &queue, scope.output().unwrap());
        let error = oracle::maximum_error(&actual, &reference);
        assert!(error <= 2, "filtered prefix phase {phase}: error {error}");
        retain(&format!("filtered-{phase}"), &actual, &reference, &p);
        if phase == 2 {
            let output = scope.output().unwrap().clone();
            let revision = scope.revision();
            for ids in [&[1, 3][..], &[1, 2, 4]] {
                let mut bad = request(ids);
                bad.anchor = ObjectId::new(5);
                bad.source_revision = 100;
                assert_eq!(
                    prepare(
                        (&device, &queue),
                        &renderer,
                        &text,
                        &prepared,
                        &mut filter,
                        &mut scope,
                        &bad
                    ),
                    Err(EffectPrepareError::PublicationMismatch)
                );
                assert_eq!(scope.output(), Some(&output));
                assert_eq!(scope.revision(), revision);
            }
            renderer.invalidate_effect_captures();
            p.source_revision += 1;
            assert_eq!(
                prepare(
                    (&device, &queue),
                    &renderer,
                    &text,
                    &prepared,
                    &mut filter,
                    &mut scope,
                    &p
                ),
                Err(EffectPrepareError::PublicationMismatch)
            );
            assert_eq!(scope.output(), Some(&output));
            assert_eq!(scope.revision(), revision);
        }
        eprintln!("filtered prefix phase {phase}: max byte error {error}");
    }
}

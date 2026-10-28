//! Camera-owned Bloom through ordinary authoring, shared time and retained output.
use super::{decode, expected, read, reference, SIZE};
use noon::{
    effects::{Bloom, EffectUpdate, Pixels},
    AnimationOptions, RateFunction, Scene,
};
use noon_core::{EffectParameter, ViewColorProfile};
use noon_render_wgpu::{
    text::TextDeviceMetrics, Camera2D, GpuRenderer, HdrBloomCapture, HdrBloomFilter,
    HdrBloomParameters, HdrBloomScope, RetainedFramePreparer,
};

#[test]
fn view_bloom_admission_and_profile_changes_preserve_atomic_publication() {
    let mut scene = Scene::new();
    let mut camera = scene.camera_frame().unwrap();
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .unwrap();
    camera
        .add_effect(
            Bloom::new(EffectUpdate::default().threshold(0.25)).unwrap(),
            "bloom",
        )
        .unwrap();
    let a = scene.square(1.0).unwrap();
    let family = scene.family(&[(&a).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut session = scene.execution_session().unwrap();
    let camera_id = session.execution_object_id(camera.node_id()).unwrap();
    let publication = session.take_renderer_publication();
    assert_eq!(publication.primary_view(), Some(camera_id));
    assert_eq!(
        publication.view_effects().unwrap().input_role(),
        noon_core::EffectInputRole::AuthoredView
    );
    drop(publication);
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut wrong_host = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let publication = session.take_renderer_publication();
    assert!(matches!(
        wrong_host.effect_source_visibility(&publication, &[]),
        Err(noon_render_wgpu::EffectPrepareError::UnsupportedCapture)
    ));
    assert!(!wrong_host.has_retained_effect_sources());
    drop(publication);
    let before = scene.integration_store().borrow().scene_revision();
    let mut wrong = a.clone();
    assert!(wrong.add_effect(Bloom::default(), "wrong-initial").is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);

    assert!(scene
        .live(&mut session)
        .add_effect(&a, Bloom::default(), "wrong")
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert!(scene
        .live(&mut session)
        .add_effect(&family, Bloom::default(), "wrong")
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert!(scene
        .live(&mut session)
        .add_effect(&camera, Bloom::default(), "duplicate")
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert!(scene
        .live(&mut session)
        .set_view_color_profile(&camera, ViewColorProfile::Compatibility)
        .is_err());
    assert_eq!(scene.integration_store().borrow().scene_revision(), before);
    assert_eq!(
        session
            .take_renderer_publication()
            .view_effects()
            .unwrap()
            .definitions()[0]
            .as_bloom()
            .unwrap()
            .threshold(),
        0.25
    );
    let binding = camera.get_effect("bloom").unwrap();
    scene
        .live(&mut session)
        .set_effect(&camera, &binding, EffectUpdate::default().threshold(0.0))
        .unwrap();
    assert_eq!(
        session
            .take_renderer_publication()
            .view_effects()
            .unwrap()
            .definitions()[0]
            .as_bloom()
            .unwrap()
            .threshold(),
        0.0
    );
    // A single ordinary transaction may remove the attachment and leave HDR.
    let mut tx = noon_core::SemanticMutationTransaction::new();
    tx.set_view_color_profile(camera.node_id(), ViewColorProfile::Compatibility);
    tx.remove_node(binding.node_id());
    scene.live(&mut session).apply(tx).unwrap();
    let publication = session.take_renderer_publication();
    assert_eq!(publication.color_profile(), ViewColorProfile::Compatibility);
    assert!(publication.view_effects().is_none());
}
#[test]
#[ignore = "requires a real native float/blending adapter; absence fails qualification"]
fn authored_camera_bloom_animation_seek_removal_matches_independent_pixels() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("authored camera Bloom adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let (scene, mut camera, _) = super::scene(false, false, false);
    camera
        .set_view_color_profile(ViewColorProfile::LinearHdr)
        .unwrap();
    camera.set_object_opacity(0.75).unwrap();
    camera
        .add_effect(
            Bloom::new(
                EffectUpdate::default()
                    .radius(Pixels(0.65))
                    .threshold(0.05)
                    .intensity(3.0),
            )
            .unwrap(),
            "bloom",
        )
        .unwrap();
    let binding = camera.get_effect("bloom").unwrap();
    let mut target = camera.target_editor().unwrap();
    target
        .set_effect(
            "bloom",
            EffectUpdate::default()
                .radius(Pixels(3.0))
                .threshold(0.5)
                .intensity(0.0),
        )
        .unwrap();
    let declaration = scene
        .declare_transform_to(
            &camera,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut session = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut session)
        .play_animation(&declaration)
        .unwrap();
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba16Float);
    renderer.set_viewport(&device, &queue, SIZE, SIZE);
    renderer.set_camera(
        &queue,
        Camera2D::new(noon_core::Vec2::ZERO, noon_core::Vec2::new(8.0, 8.0)).unwrap(),
    );
    let mut preparer = RetainedFramePreparer::new();
    let mut text = renderer.create_retained_text_state(&device, &queue);
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("authored camera Bloom source"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut filter = HdrBloomFilter::new(&adapter, &device);
    let mut scope = HdrBloomScope::default();
    let clear = [0.015625, 0.03125, 0.0625, 0.25];
    let clear_color = wgpu::Color {
        r: clear[0],
        g: clear[1],
        b: clear[2],
        a: clear[3],
    };
    let input = expected(false, false, clear);
    let root = std::env::var_os("NOON_VIEW_BLOOM_ARTIFACTS").map(std::path::PathBuf::from);
    if let Some(root) = &root {
        std::fs::create_dir_all(root).unwrap();
    }
    let mut cases = Vec::new();
    let mut maximum = 0.0f64;
    let mut display_max = 0;
    let mut prior_mid = None;
    let mut opacity_negative = 0.0f64;
    for (phase, time) in [0.0, 0.5, 0.875, 0.25, 0.75, 0.5].into_iter().enumerate() {
        if phase <= 2 {
            scene
                .live(&mut session)
                .advance_segment_to(segment, time)
                .unwrap();
        } else {
            session.seek(time).unwrap();
        }
        let publication = session.take_renderer_publication();
        let bloom = publication.view_effects().unwrap().definitions()[0]
            .as_bloom()
            .unwrap();
        let p = HdrBloomParameters {
            sigma_pixels: 0.65 * (1.0 - time) + 3.0 * time,
            threshold: (0.05 * (1.0 - time) + 0.5 * time) as f32,
            intensity: (3.0 * (1.0 - time)) as f32,
            scratch_budget_bytes: 1_000_000,
        };
        assert_eq!(bloom.radius().value(), p.sigma_pixels);
        assert_eq!(bloom.threshold() as f32, p.threshold);
        assert_eq!(bloom.intensity() as f32, p.intensity);
        let visible = publication
            .painter_order()
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        let visible = renderer
            .effect_source_visibility(&publication, &visible)
            .unwrap();
        let prepared = preparer
            .prepare_planned_publication_visible(
                &device,
                &publication,
                visible,
                TextDeviceMetrics::uniform(2.0).unwrap(),
            )
            .unwrap();
        let upload = renderer.upload_retained(&device, &queue, &prepared, &mut text);
        if phase == 1 || phase == 2 {
            assert_eq!(upload.bytes_uploaded(),0,"camera parameter edits do not reupload source draws: phase={phase} all={} stats={:?}",publication.changes().is_all(),prepared.geometry_stats());
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        let effects = renderer
            .prepare_retained_effects(
                &device,
                &queue,
                &mut encoder,
                &prepared,
                &text,
                &publication,
                clear_color,
                1_000_000,
            )
            .unwrap();
        assert_eq!(
            effects.source_passes, 0,
            "view Bloom cannot capture the invisible camera rectangle"
        );
        renderer
            .encode_retained(
                &mut encoder,
                &source.create_view(&Default::default()),
                &prepared,
                &text,
                clear_color,
                None,
            )
            .unwrap();
        filter
            .prepare_view(
                &device,
                &queue,
                &mut scope,
                &publication,
                HdrBloomCapture {
                    source: &source,
                    revision: 1,
                },
                8.0,
                1_000_000,
            )
            .unwrap();
        filter.encode(&mut encoder, &mut scope);
        queue.submit([encoder.finish()]);
        let (linear, display) = reference(&input, [SIZE, SIZE], p);
        let raw = read(&device, &queue, scope.linear_output().unwrap());
        for (a, e) in decode(&raw).iter().zip(linear.iter().flatten()) {
            maximum = maximum.max((a - e).abs());
            assert!(
                (a - e).abs() <= 0.006 + 0.004 * e.abs(),
                "phase={phase}: {a} != {e}"
            );
        }
        let display_raw = read(&device, &queue, scope.display_output().unwrap());
        for (a, e) in display_raw.iter().zip(&display) {
            display_max = display_max.max(a.abs_diff(*e));
            assert!(a.abs_diff(*e) <= 2);
        }
        if phase == 0 {
            // Actual renderer execution with the incorrect camera-opacity uniform
            // consumer. The mathematical view oracle must reject this output.
            let mut wrong = HdrBloomFilter::new(&adapter, &device);
            let mut wrong_scope = HdrBloomScope::default();
            wrong
                .prepare(
                    &device,
                    &queue,
                    &mut wrong_scope,
                    HdrBloomCapture {
                        source: &source,
                        revision: 1,
                    },
                    HdrBloomParameters {
                        intensity: p.intensity * 0.75,
                        ..p
                    },
                )
                .unwrap();
            let mut encoder = device.create_command_encoder(&Default::default());
            wrong.encode(&mut encoder, &mut wrong_scope);
            queue.submit([encoder.finish()]);
            let wrong_raw = read(&device, &queue, wrong_scope.linear_output().unwrap());
            opacity_negative = decode(&wrong_raw)
                .iter()
                .zip(linear.iter().flatten())
                .map(|(a, e)| (a - e).abs())
                .fold(0.0f64, f64::max);
            assert!(opacity_negative > 0.02);
            if let Some(root) = &root {
                std::fs::write(root.join("camera-opacity-negative.rgba16f"), wrong_raw).unwrap();
            }
        }
        if phase == 1 {
            prior_mid = Some(raw.clone());
        }
        if phase == 5 {
            assert_eq!(
                Some(&raw),
                prior_mid.as_ref(),
                "backward/forward seek restores identical view output"
            );
        }
        let clean = filter
            .prepare_view(
                &device,
                &queue,
                &mut scope,
                &publication,
                HdrBloomCapture {
                    source: &source,
                    revision: 1,
                },
                8.0,
                1_000_000,
            )
            .unwrap();
        assert_eq!(clean.bytes_uploaded, 0);
        let mut encoder = device.create_command_encoder(&Default::default());
        assert_eq!(filter.encode(&mut encoder, &mut scope).passes, [0; 5]);
        queue.submit([encoder.finish()]);
        let id = cases.len();
        if let Some(root) = &root {
            std::fs::write(root.join(format!("{id}.rgba16f")), raw).unwrap();
            std::fs::write(root.join(format!("{id}.display.rgba8")), display_raw).unwrap();
        }
        cases.push(serde_json::json!({"id":id,"time":time,"radius":p.sigma_pixels,"threshold":p.threshold,"intensity":p.intensity,"expected":linear,"display_expected":display}));
    }
    scene
        .live(&mut session)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    scene.live(&mut session).complete_segment(segment).unwrap();
    scene
        .live(&mut session)
        .remove_effect(&camera, &binding)
        .unwrap();
    let publication = session.take_renderer_publication();
    assert!(publication.view_effects().is_none());
    let p = HdrBloomParameters {
        sigma_pixels: 0.0,
        threshold: 1.0,
        intensity: 0.0,
        scratch_budget_bytes: 1_000_000,
    };
    filter
        .prepare_view(
            &device,
            &queue,
            &mut scope,
            &publication,
            HdrBloomCapture {
                source: &source,
                revision: 1,
            },
            8.0,
            1_000_000,
        )
        .unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    let stats = filter.encode(&mut encoder, &mut scope);
    queue.submit([encoder.finish()]);
    assert_eq!(stats.passes, [0, 0, 0, 1, 1]);
    let (linear, display) = reference(&input, [SIZE, SIZE], p);
    for (a, e) in decode(&read(&device, &queue, scope.linear_output().unwrap()))
        .iter()
        .zip(linear.iter().flatten())
    {
        assert!((a - e).abs() <= 0.006 + 0.004 * e.abs());
    }
    for (a, e) in read(&device, &queue, scope.display_output().unwrap())
        .iter()
        .zip(display)
    {
        assert!(a.abs_diff(e) <= 2);
    }
    if let Some(root) = root {
        std::fs::write(root.join("report.json"),serde_json::to_vec(&serde_json::json!({"size":[SIZE,SIZE],"input":input,"camera_opacity":0.75,"cases":cases,"maximum_error":maximum,"display_maximum":display_max,"camera_opacity_negative":opacity_negative})).unwrap()).unwrap();
    }
    assert_eq!(
        EffectUpdate::default()
            .threshold(0.0)
            .parameters()
            .collect::<Vec<_>>(),
        [EffectParameter::Threshold]
    );
    eprintln!("authored camera Bloom: {} samples, float max={maximum}, display max={display_max}, exact seek restoration and camera-opacity independence; actual wrong-opacity negative={opacity_negative}",cases.len());
}

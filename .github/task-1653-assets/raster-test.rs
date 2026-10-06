#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn zero_contribution_camera_is_pixel_identical_and_visible_strokes_restore() {
    use noon_runtime::FrameChanges;
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: true, ..Default::default()
        }).await.expect("zero-contribution qualification requires software Vulkan");
        eprintln!("zero-contribution adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zero-contribution camera target"),
            size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("zero-contribution readback"), size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let scene = SceneInstance::new(CompiledScene::compile_objects(vec![
            CompiledObject::new(ObjectId::new(0), GeometryRef::rectangle(2.0, 2.0), Transform2D::IDENTITY,
                Style { opacity: 0.0, fill: Some(Color::WHITE), stroke: None, ..Style::default() }),
            CompiledObject::new(ObjectId::new(1), GeometryRef::circle(0.3), Transform2D::IDENTITY,
                Style { fill: Some(Color::rgba(1.0, 0.0, 0.0, 1.0)), stroke: None, ..Style::default() }),
        ], &[]).unwrap());
        let mut frame = scene.frame().clone();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(&queue, Camera2D::new(Vec2::ZERO, Vec2::new(2.0, 2.0)).unwrap());
        let mut preparer = FramePreparer::new();
        let mut render = |prepared: &noon_render_wgpu::PreparedFrame<'_>| {
            renderer.upload(&device, &queue, prepared);
            let mut encoder = device.create_command_encoder(&Default::default());
            let draws = renderer.encode(&mut encoder, &view, prepared, wgpu::Color::BLACK);
            let pixels = submit_and_read(&device, &queue, encoder, &target, &readback);
            (draws, pixels)
        };
        let (initial_draw, hidden_pixels) = render(&preparer.prepare(&frame));
        assert_eq!(initial_draw.instances_drawn, 1);
        assert_eq!(initial_draw.draw_calls, 1);
        assert!(rgba(&hidden_pixels, WIDTH / 2, HEIGHT / 2)[0] > 200, "independent visible row remains rendered");
        let mut omitted = frame.clone();
        omitted.presences[0] = false;
        let mut reference_preparer = FramePreparer::new();
        let (_, omitted_pixels) = render(&reference_preparer.prepare(&omitted));
        assert_eq!(hidden_pixels, omitted_pixels, "invisible camera and omitted draw must be byte-identical");
        // Independent reference uploads require a full candidate upload afterward.
        render(&preparer.prepare(&frame));
        for opacity in [0.25, 1.0, 0.0, 0.5] {
            frame.objects[0].style.opacity = opacity;
            let (draws, pixels) = render(&preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0])));
            assert_eq!(draws.instances_drawn, 1 + usize::from(opacity != 0.0));
            if opacity == 0.0 { assert_eq!(pixels, hidden_pixels); }
            else { assert_ne!(pixels, hidden_pixels); }
        }
        frame.objects[0].style.opacity = 1.0;
        frame.objects[0].style.fill = Some(Color::rgba(1.0, 1.0, 1.0, 0.0));
        frame.objects[0].style.stroke = Some(Color::WHITE);
        frame.objects[0].style.stroke_width = 0.1;
        let (draws, stroked) = render(&preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0])));
        assert_eq!(draws.instances_drawn, 2);
        assert_ne!(stroked, hidden_pixels, "visible stroke must not be culled with transparent fill");
        assert!(frame.presences[0]);
        assert_eq!(frame.objects[0].id, ObjectId::new(0));
    });
}

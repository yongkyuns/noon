    #[test]
    fn zero_contribution_mixed_camera_draw_restores_without_rebuilding_text_or_order() {
        let (mut frame, texts, fonts, geometries) = geometry_and_fast_text_frame();
        frame.objects[0].content = ObjectContentRef::Geometry(GeometryRef::rectangle(14.222222, 8.0));
        frame.objects[0].style.opacity = 0.0;
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut preparer = RetainedFramePreparer::new();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, 64, 64);
        let mut text_state = renderer.create_retained_text_state(&device, &queue);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zero contribution mixed camera regression"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut baseline = None;
        for (step, opacity) in [0.0, 0.25, 1.0, 0.0, 1.0].into_iter().enumerate() {
            frame.objects[0].style.opacity = opacity;
            let changes = if step == 0 { FrameChanges::all() } else { FrameChanges::objects(vec![0]) };
            let prepared = preparer.prepare_with_changes(&device, &frame, &changes, &texts, &fonts, &geometries, TextDeviceMetrics::uniform(67.5).unwrap()).unwrap();
            assert!(!prepared.geometry_only);
            assert_eq!(prepared.geometry.rectangles.len(), 1);
            assert_eq!(prepared.observe_object(0, ObjectId::new(1)).unwrap().submission_membership, opacity != 0.0);
            renderer.upload_retained(&device, &queue, &prepared, &mut text_state);
            let mut encoder = device.create_command_encoder(&Default::default());
            let draw = renderer.encode_retained(&mut encoder, &view, &prepared, &text_state, wgpu::Color::BLACK, None).unwrap();
            assert_eq!(draw.geometry.instances_drawn, usize::from(opacity != 0.0));
            assert!(draw.text.draw_calls > 0, "independent text must still draw");
            queue.submit([encoder.finish()]);
            let stats = preparer.incremental_stats();
            if let Some(baseline) = baseline {
                let baseline: RetainedFrameIncrementalStats = baseline;
                assert_eq!(stats.scratch_rebuilds, baseline.scratch_rebuilds);
                assert_eq!(stats.mixed_order_rebuilds, baseline.mixed_order_rebuilds);
                assert_eq!(stats.text_snapshot_copies, baseline.text_snapshot_copies);
            } else { baseline = Some(stats); }
        }
    }

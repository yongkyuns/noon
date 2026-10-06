    #[test]
    fn zero_contribution_hidden_transient_anchor_keeps_visible_before_and_after_effects() {
        use noon_runtime::{FrameChanges, TransientAnchorSide};
        let compiled = CompiledScene::compile_objects(vec![CompiledObject::new(
            ObjectId::new(1), GeometryRef::rectangle(14.222222, 8.0), Transform2D::IDENTITY,
            Style { opacity: 0.0, ..Style::default() },
        )], &[]).unwrap();
        let mut runtime = SceneInstance::new(compiled);
        let presentations = [
            TransientPresentationOccurrence::new(0, 7, state(GeometryRef::circle(0.25))).with_anchor_side(TransientAnchorSide::Before),
            TransientPresentationOccurrence::new(0, 8, state(GeometryRef::circle(0.5))).with_anchor_side(TransientAnchorSide::After),
        ];
        let publication = runtime.take_renderer_publication().with_transient_presentations(&presentations).unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        let mut frame = publication.frame().clone();
        let mut preparer = FramePreparer::new();
        preparer.set_painter_order(&frame, publication.painter_order());
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, 32, 32);
        renderer.upload_transient_presentations(&device, &queue, &derived);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("invisible stable anchor with independent visible effects"),
            size: wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        for opacity in [0.0, 0.5, 1.0, 0.0] {
            frame.objects[0].style.opacity = opacity;
            let stable = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
            assert_eq!(stable.rectangles.len(), 1);
            let mut encoder = device.create_command_encoder(&Default::default());
            renderer.upload(&device, &queue, &stable);
            let draws = renderer.encode_with_transient_presentations(&mut encoder, &view, &stable, &derived, wgpu::Color::BLACK);
            queue.submit([encoder.finish()]);
            assert_eq!(draws.instances_drawn, 2 + usize::from(opacity != 0.0));
            assert_eq!(stable.observe_object(0).unwrap().submission_membership, Some(opacity != 0.0));
            assert!(frame.presences[0]);
        }
    }

use super::*;

fn styled_object(id: u64, geometry: GeometryRef) -> FrameObjectState {
    let mut state = object(id, geometry);
    state.style.fill = None;
    state.style.stroke = Some(Color::WHITE);
    state.style.stroke_width = 0.02;
    state
}

fn path(seed: usize) -> GeometryRef {
    let x = seed as f32;
    GeometryRef::path(
        VectorPath::new()
            .move_to(Vec2::new(x, 0.0))
            .line_to(Vec2::new(x + 0.5, 0.5))
            .with_morph_target(
                VectorPath::new()
                    .move_to(Vec2::new(x, 0.2))
                    .line_to(Vec2::new(x + 0.3, 0.7)),
            ),
    )
}

fn simple_path(seed: usize) -> GeometryRef {
    let x = seed as f32;
    GeometryRef::path(
        VectorPath::new()
            .move_to(Vec2::new(x, 0.0))
            .line_to(Vec2::new(x + 0.5, 0.5)),
    )
}

#[test]
fn resident_first_use_phases_upload_instances_only_and_keep_prefix_on_fallback() {
    let geometries: Vec<_> = (0..600).map(path).collect();
    let style = styled_object(0, path(0)).style;
    let requests: Vec<_> = geometries
        .iter()
        .map(|geometry| PathMeshPreload {
            geometry,
            style,
            transform: Transform2D::IDENTITY,
        })
        .collect();
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer.set_path_mesh_cache_limit(1); // resident pinning is independent of the LRU budget.
    preparer.preload_paths(&requests).unwrap();
    let prefix_vertices = preparer.path_vertices.clone();
    let prefix_indices = preparer.path_indices.clone();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let preload = preparer.preloaded_frame();
    assert!(preload.paths.is_empty() && preload.render_batches.is_empty());
    let upload = renderer
        .upload_preloaded_paths(&device, &queue, &preload)
        .unwrap();
    assert!(upload.bytes_uploaded > 0 && upload.buffer_reallocations > 0);
    for phase in [0, 1, 0] {
        let frame = frame(
            geometries[phase * 300..(phase + 1) * 300]
                .iter()
                .enumerate()
                .map(|(i, g)| styled_object(i as u64, g.clone()))
                .collect(),
        );
        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
        assert_eq!(prepared.stats.path_indices_repacked, 0);
        assert!(
            prepared.path_vertex_dirty_ranges.is_empty()
                && prepared.path_index_dirty_ranges.is_empty()
        );
        let mut writes = Vec::new();
        let upload = renderer.upload_with_trace(&device, &queue, &prepared, &mut writes);
        assert_eq!(upload.bytes_uploaded, std::mem::size_of_val(prepared.paths));
        assert!(upload.bytes_uploaded < 1_000_000);
        // The first real frame may allocate its instance buffer, never geometry.
        assert!(writes
            .iter()
            .all(|write| write.buffer != "path_vertex" && write.buffer != "path_index"));
        assert_eq!(prepared.path_vertices, prefix_vertices);
        assert_eq!(prepared.path_indices, prefix_indices);
    }
    let fallback = frame(vec![styled_object(0, path(1000))]);
    let prepared = preparer.prepare(&fallback);
    assert_eq!(prepared.stats.geometry_cache_misses, 1);
    assert!(prepared
        .path_vertex_dirty_ranges
        .iter()
        .all(|r| r.start >= prefix_vertices.len()));
    assert!(prepared
        .path_index_dirty_ranges
        .iter()
        .all(|r| r.start >= prefix_indices.len()));
    assert_eq!(
        &prepared.path_vertices[..prefix_vertices.len()],
        prefix_vertices
    );
    assert_eq!(
        &prepared.path_indices[..prefix_indices.len()],
        prefix_indices
    );
    let back = frame(vec![styled_object(0, geometries[0].clone())]);
    let prepared = preparer.prepare(&back);
    assert_eq!(prepared.stats.geometry_cache_misses, 0);
    assert!(
        prepared.path_vertex_dirty_ranges.is_empty() && prepared.path_index_dirty_ranges.is_empty()
    );
    assert_eq!(preparer.cached_path_mesh_count(), 600);
}

#[test]
fn resident_keys_deduplicate_exact_requests_but_preserve_style_and_transform_variants() {
    let geometry = path(0);
    let base = PathMeshPreload {
        geometry: &geometry,
        style: styled_object(0, geometry.clone()).style,
        transform: Transform2D::IDENTITY,
    };
    let mut width = base;
    width.style.stroke_width *= 2.0;
    let mut scaled = base;
    scaled.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
    scaled.transform.scale = Vec2::new(2.0, 3.0);
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer
        .preload_paths(&[base, base, width, scaled])
        .unwrap();
    assert_eq!(preparer.cached_path_mesh_count(), 3);
    for request in [base, width, scaled] {
        let mut state = styled_object(0, geometry.clone());
        state.style = request.style;
        state.transform = request.transform;
        let prepared = preparer.prepare(&frame(vec![state]));
        assert_eq!(prepared.stats.geometry_cache_misses, 0);
        assert_eq!(prepared.stats.path_vertices_repacked, 0);
    }
}

#[test]
fn resident_replacement_never_recycles_prefix_ranges() {
    let a = path(0);
    let b = path(1);
    let style = styled_object(0, a.clone()).style;
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer
        .preload_paths(&[
            PathMeshPreload {
                geometry: &a,
                style,
                transform: Transform2D::IDENTITY,
            },
            PathMeshPreload {
                geometry: &b,
                style,
                transform: Transform2D::IDENTITY,
            },
        ])
        .unwrap();
    let prefix = preparer.path_vertices.clone();
    let mut current = frame(vec![styled_object(0, a)]);
    preparer.prepare(&current);
    current.objects[0].content = noon_core::ObjectContentRef::Geometry(path(100));
    preparer.replace_unique_path_geometry(&current, 0).unwrap();
    assert!(preparer.path_batch_vertex_ranges[0].start as usize >= prefix.len());
    assert!(preparer
        .path_vertex_free_ranges
        .iter()
        .all(|r| r.start as usize >= prefix.len()));
    current.objects[0].content = noon_core::ObjectContentRef::Geometry(b);
    let result = preparer.replace_unique_path_geometry(&current, 0).unwrap();
    assert_eq!(result.vertices_repacked, 0);
    assert_eq!(result.indices_repacked, 0);
    assert_eq!(&preparer.path_vertices[..prefix.len()], prefix);
}

#[test]
fn incremental_replacement_prunes_stale_meshes_without_changing_row_order() {
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer.set_path_mesh_cache_limit(1);
    let mut current = frame(vec![
        styled_object(0, path(0)),
        styled_object(1, path(1000)),
    ]);
    let initial = preparer.prepare(&current);
    let initial_vertex_count = initial.path_vertices.len();
    let initial_index_count = initial.path_indices.len();
    assert_eq!(initial.path_ids, [ObjectId::new(0), ObjectId::new(1)]);

    let mut final_vertices = Vec::new();
    let mut final_indices = Vec::new();
    for seed in 1..65 {
        current.objects[0].content = noon_core::ObjectContentRef::Geometry(path(seed));
        let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
        assert_eq!(prepared.path_ids, [ObjectId::new(0), ObjectId::new(1)]);
        assert_eq!(prepared.path_batches.len(), 2);
        assert_eq!(prepared.path_vertices.len(), initial_vertex_count);
        assert_eq!(prepared.path_indices.len(), initial_index_count);
        assert!(prepared
            .path_batch_cache_indices
            .iter()
            .all(|&index| index < prepared.path_mesh_cache.len()));
        assert!(prepared.path_mesh_cache.len() <= 4);
        final_vertices = prepared.path_vertices.to_vec();
        final_indices = prepared.path_indices.to_vec();
    }

    let mut full_rebuild = FramePreparer::for_individual_path_draws();
    let expected = full_rebuild.prepare(&current);
    assert_eq!(final_vertices, expected.path_vertices);
    assert_eq!(final_indices, expected.path_indices);
}

#[test]
fn incremental_prune_remaps_batches_even_when_packed_generation_is_stale() {
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer.set_path_mesh_cache_limit(1);
    let mut current = frame(vec![
        styled_object(0, simple_path(0)),
        styled_object(1, simple_path(100)),
    ]);
    current.reveals[1] = 0.5;
    preparer.prepare(&current);
    assert!(matches!(
        preparer.slots[1],
        PreparedSlot::Path {
            reveal_head: Some(_),
            ..
        }
    ));

    // Model a prior cache edit whose packed bytes have not been rebuilt yet.
    // Pruning must remap descriptors independently of this generation marker.
    preparer.path_mesh_cache_generation += 1;
    for seed in 1..=3 {
        current.objects[0].content = noon_core::ObjectContentRef::Geometry(simple_path(seed));
        preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
    }
    assert_ne!(
        preparer.packed_path_mesh_cache_generation,
        preparer.path_mesh_cache_generation
    );
    assert!(preparer
        .path_batch_cache_indices
        .iter()
        .all(|&index| index < preparer.path_mesh_cache.len()));
    let GeometryRef::VectorPath(second_path) = current.objects[1].geometry().unwrap() else {
        panic!("expected vector path geometry");
    };
    assert_eq!(
        preparer.path_mesh_cache[preparer.path_batch_cache_indices[1]].path,
        *second_path
    );

    // A subsequent incremental reveal reads the remapped row descriptor to
    // position its round-cap reveal head.
    current.reveals[1] = 0.6;
    let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![1]));
    assert!(prepared
        .lines
        .iter()
        .any(|line| line.transform.padding == 1.0));
}

#[test]
fn incremental_cache_pruning_is_amortized_when_live_rows_exceed_lru_limit() {
    const LIVE_ROWS: usize = 100;
    let mut preparer = FramePreparer::for_individual_path_draws();
    preparer.set_path_mesh_cache_limit(1);
    let mut current = frame(
        (0..LIVE_ROWS)
            .map(|index| styled_object(index as u64, simple_path(index)))
            .collect(),
    );
    let initial = preparer.prepare(&current);
    let initial_vertices = initial.path_vertices.len();
    let initial_indices = initial.path_indices.len();

    for seed in 1..=LIVE_ROWS {
        current.objects[0].content =
            noon_core::ObjectContentRef::Geometry(simple_path(seed + 1000));
        let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
        assert_eq!(prepared.path_ids.len(), LIVE_ROWS);
        assert_eq!(prepared.path_vertices.len(), initial_vertices);
        assert_eq!(prepared.path_indices.len(), initial_indices);
        assert_eq!(prepared.path_mesh_cache.len(), LIVE_ROWS + seed);
    }

    current.objects[0].content = noon_core::ObjectContentRef::Geometry(simple_path(2000));
    let prepared = preparer.prepare_incremental(&current, &FrameChanges::objects(vec![0]));
    assert_eq!(prepared.path_ids.len(), LIVE_ROWS);
    assert_eq!(prepared.path_mesh_cache.len(), LIVE_ROWS);
    assert!(prepared
        .path_batch_cache_indices
        .iter()
        .all(|&index| index < prepared.path_mesh_cache.len()));

    let mut full_rebuild = FramePreparer::for_individual_path_draws();
    let expected = full_rebuild.prepare(&current);
    assert_eq!(prepared.path_ids, expected.path_ids);
    assert_eq!(prepared.path_vertices, expected.path_vertices);
    assert_eq!(prepared.path_indices, expected.path_indices);
}

#[test]
fn submitted_frame_survives_resident_prefix_compaction_and_next_draw_uses_compact_buffers() {
    const INITIAL_PATHS: usize = 64;
    const LIVE_PATHS: usize = 2;
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let geometries = (0..INITIAL_PATHS).map(simple_path).collect::<Vec<_>>();
    let style = styled_object(0, geometries[0].clone()).style;
    let all_requests = geometries
        .iter()
        .map(|geometry| PathMeshPreload {
            geometry,
            style,
            transform: Transform2D::IDENTITY,
        })
        .collect::<Vec<_>>();

    let mut old_preparer = FramePreparer::for_individual_path_draws();
    old_preparer.preload_paths(&all_requests).unwrap();
    let resident = old_preparer.preloaded_frame();
    renderer
        .upload_preloaded_paths(&device, &queue, &resident)
        .unwrap();
    queue.submit([]);
    let old_vertex_capacity = renderer.path_vertex_capacity_bytes();
    let old_index_capacity = renderer.path_index_capacity_bytes();

    let frame_a = frame(
        geometries
            .iter()
            .enumerate()
            .map(|(index, geometry)| styled_object(index as u64, geometry.clone()))
            .collect(),
    );
    let target_a = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Frame A before path residency compaction"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view_a = target_a.create_view(&wgpu::TextureViewDescriptor::default());
    {
        let prepared_a = old_preparer.prepare(&frame_a);
        renderer.upload(&device, &queue, &prepared_a);
        let mut encoder_a = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Submit frame A before resident path compaction"),
        });
        let draw_a = renderer.encode(&mut encoder_a, &view_a, &prepared_a, wgpu::Color::BLACK);
        assert!(draw_a.draw_calls > 0);
        queue.submit(Some(encoder_a.finish()));
    }

    let compact_requests = all_requests[..LIVE_PATHS].to_vec();
    let mut compacted_preparer = FramePreparer::for_individual_path_draws();
    compacted_preparer.preload_paths(&compact_requests).unwrap();
    let compacted_resident = compacted_preparer.preloaded_frame();
    renderer
        .replace_preloaded_path_buffers(&device, &queue, &compacted_resident)
        .unwrap();
    queue.submit([]);
    assert!(renderer.path_vertex_capacity_bytes() < old_vertex_capacity);
    assert!(renderer.path_index_capacity_bytes() < old_index_capacity);

    let mut frame_b = frame(
        geometries
            .iter()
            .take(LIVE_PATHS)
            .enumerate()
            .map(|(index, geometry)| styled_object(index as u64, geometry.clone()))
            .collect(),
    );
    frame_b.morphs.fill(1.0);
    let target_b = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Frame B after path residency compaction"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view_b = target_b.create_view(&wgpu::TextureViewDescriptor::default());
    let prepared_b = compacted_preparer.prepare(&frame_b);
    assert_eq!(prepared_b.stats.geometry_cache_misses, 0);
    let upload_b = renderer.upload(&device, &queue, &prepared_b);
    assert_eq!(upload_b.buffer_reallocations, 0);
    let mut encoder_b = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Submit frame B using compact resident path buffers"),
    });
    let draw_b = renderer.encode(&mut encoder_b, &view_b, &prepared_b, wgpu::Color::BLACK);
    assert!(draw_b.draw_calls > 0);
    queue.submit(Some(encoder_b.finish()));
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}

#[test]
fn explicit_residency_compaction_bounds_long_new_slot_churn() {
    const CHURN: usize = 1_000;
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut preparer = RetainedFramePreparer::new();
    let mut live_geometry = simple_path(0);
    let style = styled_object(0, live_geometry.clone()).style;
    let initial_request = PathMeshPreload {
        geometry: &live_geometry,
        style,
        transform: Transform2D::IDENTITY,
    };
    preparer
        .preload_path_meshes(
            &device,
            &queue,
            &mut renderer,
            std::slice::from_ref(&initial_request),
        )
        .unwrap();
    queue.submit([]);

    for slot in 1..CHURN {
        let admitted = simple_path(slot);
        let request = PathMeshPreload {
            geometry: &admitted,
            style,
            transform: Transform2D::IDENTITY,
        };
        preparer
            .append_preload_path_meshes(
                &device,
                &queue,
                &mut renderer,
                std::slice::from_ref(&request),
            )
            .unwrap();
        queue.submit([]);
        live_geometry = admitted;

        let live_request = PathMeshPreload {
            geometry: &live_geometry,
            style,
            transform: Transform2D::IDENTITY,
        };
        if preparer.resident_path_maintenance_due(1) {
            preparer
                .compact_path_meshes(
                    &device,
                    &queue,
                    &mut renderer,
                    std::slice::from_ref(&live_request),
                )
                .unwrap();
            queue.submit([]);
            assert_eq!(preparer.resident_path_mesh_count(), 1);
        }
        assert!(
            preparer.resident_path_mesh_count() <= 1 + 64 + 1,
            "newly admitted slot history must stay within the live set plus one high-water batch"
        );
    }
    assert!(preparer.resident_path_mesh_count() <= 1 + 64);
}

#[test]
fn large_scene_single_root_retirement_does_not_trigger_full_compaction() {
    const ROOTS: usize = 100_000;
    let geometry = simple_path(0);
    let style = styled_object(0, geometry.clone()).style;
    let requests = (0..ROOTS)
        .map(|_| PathMeshPreload {
            geometry: &geometry,
            style,
            transform: Transform2D::IDENTITY,
        })
        .collect::<Vec<_>>();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    let mut preparer = RetainedFramePreparer::new();
    preparer
        .preload_path_meshes(&device, &queue, &mut renderer, &requests)
        .unwrap();
    queue.submit([]);

    assert_eq!(preparer.resident_path_mesh_count(), 1);
    assert!(!preparer.resident_path_maintenance_due(ROOTS - 1));
    assert!(preparer.resident_path_maintenance_due(0));
}

#[test]
fn native_preload_rejects_nonfinite_specializations_atomically() {
    let geometry = path(0);
    let base = PathMeshPreload {
        geometry: &geometry,
        style: styled_object(0, geometry.clone()).style,
        transform: Transform2D::IDENTITY,
    };
    let mut preparer = RetainedFramePreparer::new();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
    preparer
        .preload_path_meshes(&device, &queue, &mut renderer, &[base])
        .unwrap();
    let mut transform = base;
    transform.transform.scale.x = f32::NAN;
    let mut color = base;
    color.style.fill = Some(Color {
        red: f32::NAN,
        ..Color::WHITE
    });
    let mut opacity = base;
    opacity.style.opacity = f32::INFINITY;
    let mut width = base;
    width.style.stroke_width = -1.0;
    for invalid in [transform, color, opacity, width] {
        assert!(preparer
            .preload_path_meshes(&device, &queue, &mut renderer, &[base, invalid])
            .is_err());
    }
}

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
    let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
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
fn native_preload_rejects_nonfinite_specializations_atomically() {
    let geometry = path(0);
    let base = PathMeshPreload {
        geometry: &geometry,
        style: styled_object(0, geometry.clone()).style,
        transform: Transform2D::IDENTITY,
    };
    let mut preparer = RetainedFramePreparer::new();
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, wgpu::TextureFormat::Rgba8Unorm);
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

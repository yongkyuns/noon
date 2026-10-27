use super::*;

const SIZE: [u32; 2] = [17, 11];
const VIEW: [u32; 2] = [23, 19];
const RGBA: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const BGRA: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

fn parameters(opacity: f32) -> BackdropCompositeParameters {
    BackdropCompositeParameters {
        origin: [3, 2],
        viewport: VIEW,
        scope_opacity: opacity,
        scratch_budget_bytes: u64::from(SIZE[0] * SIZE[1] * 8),
    }
}
fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop reference image"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
fn inputs(textures: &[wgpu::Texture; 3], revisions: [u64; 3]) -> BackdropCompositeInputs<'_> {
    let [processed_prefix, target_paint, target_coverage] =
        std::array::from_fn(|i| GaussianBlurCapture {
            source: &textures[i],
            revision: revisions[i],
        });
    BackdropCompositeInputs {
        processed_prefix,
        target_paint,
        target_coverage,
    }
}
fn prepare(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filter: &mut BackdropCompositeFilter,
    scope: &mut BackdropCompositeScope,
    captures: BackdropCompositeInputs<'_>,
    p: BackdropCompositeParameters,
) -> Result<EffectRasterStats, EffectPrepareError> {
    let mut encoder = device.create_command_encoder(&Default::default());
    let result = filter.prepare_encode(device, queue, &mut encoder, scope, captures, p);
    if result.is_ok() {
        queue.submit([encoder.finish()]);
    }
    result
}

#[test]
fn invalid_preparation_is_atomic_and_cold_zero_opacity_allocates_nothing() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut filter = BackdropCompositeFilter::new(RGBA).unwrap();
    let mut scope = BackdropCompositeScope::default();
    let textures = std::array::from_fn(|_| texture(&device, SIZE, RGBA));
    let mut p = parameters(0.5);
    p.scratch_budget_bytes = 0;
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [1; 3]),
            p
        ),
        Err(EffectPrepareError::ScratchBudgetExceeded)
    );
    assert!(filter.program.is_none());
    p.scope_opacity = 0.0;
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [1; 3]),
            p
        )
        .unwrap(),
        EffectRasterStats::default()
    );
    assert!(filter.program.is_none());
    assert!(scope.contribution().is_none());
    assert!(scope.revision().is_none());
    assert!(BackdropCompositeFilter::new(wgpu::TextureFormat::Rgba8UnormSrgb).is_err());
    let first = prepare(
        &device,
        &queue,
        &mut filter,
        &mut scope,
        inputs(&textures, [1; 3]),
        parameters(0.5),
    )
    .unwrap();
    assert_eq!(
        (
            first.pipeline_compiles,
            first.texture_allocations,
            first.buffer_allocations,
            first.bind_group_creations,
            first.composite_passes
        ),
        (3, 2, 1, 3, 1)
    );
    let output = scope.contribution().unwrap().clone();
    let erase = scope.erase().unwrap().clone();
    let placement = scope.placement;
    let revision = scope.revision();
    let mut invalid = Vec::new();
    for value in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
        let mut p = parameters(value);
        p.origin = [0, 0];
        invalid.push((p, EffectPrepareError::InvalidScopeOpacity));
    }
    let mut p = parameters(0.5);
    p.viewport[0] = 0;
    invalid.push((p, EffectPrepareError::InvalidProjection));
    let mut p = parameters(0.5);
    p.viewport[1] = device.limits().max_texture_dimension_2d + 1;
    invalid.push((p, EffectPrepareError::ExtentExceedsDevice));
    let mut p = parameters(0.5);
    p.origin[1] = i32::MIN;
    invalid.push((p, EffectPrepareError::CaptureCoordinatesOutOfRange));
    let mut p = parameters(0.5);
    p.scratch_budget_bytes -= 1;
    invalid.push((p, EffectPrepareError::ScratchBudgetExceeded));
    let mut p = parameters(0.0);
    p.scratch_budget_bytes = 0;
    invalid.push((p, EffectPrepareError::ScratchBudgetExceeded));
    for (p, error) in invalid {
        assert_eq!(
            prepare(
                &device,
                &queue,
                &mut filter,
                &mut scope,
                inputs(&textures, [2; 3]),
                p
            ),
            Err(error)
        );
        assert_eq!(scope.contribution(), Some(&output));
        assert_eq!(scope.erase(), Some(&erase));
        assert_eq!(scope.placement, placement);
        assert_eq!(scope.revision(), revision);
    }
    for t in [&output, &erase] {
        for role in 0..3 {
            let mut c = inputs(&textures, [2; 3]);
            match role {
                0 => c.processed_prefix.source = t,
                1 => c.target_paint.source = t,
                _ => c.target_coverage.source = t,
            }
            assert_eq!(
                prepare(&device, &queue, &mut filter, &mut scope, c, parameters(0.5)),
                Err(EffectPrepareError::UnsupportedCapture)
            );
            assert_eq!(scope.revision(), revision);
        }
    }
    let smaller = texture(&device, [SIZE[0] - 1, SIZE[1]], RGBA);
    let mut c = inputs(&textures, [2; 3]);
    c.target_coverage.source = &smaller;
    assert_eq!(
        prepare(&device, &queue, &mut filter, &mut scope, c, parameters(0.5)),
        Err(EffectPrepareError::CaptureSizeMismatch)
    );
    let srgb = texture(&device, SIZE, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut c = inputs(&textures, [2; 3]);
    c.target_paint.source = &srgb;
    assert_eq!(
        prepare(&device, &queue, &mut filter, &mut scope, c, parameters(0.5)),
        Err(EffectPrepareError::UnsupportedCapture)
    );
    scope.output_revision = u64::MAX;
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [2; 3]),
            parameters(0.5)
        ),
        Err(EffectPrepareError::RevisionExhausted)
    );
    assert_eq!(scope.placement, placement);
    assert_eq!(scope.input_revisions, Some([1; 3]));
    assert_eq!(scope.contribution(), Some(&output));
}

#[test]
fn clean_local_changes_recovery_and_retirement_reuse_bounded_resources() {
    let (device, queue) = wgpu::Device::noop(&Default::default());
    let mut filter = BackdropCompositeFilter::new(BGRA).unwrap();
    let mut scope = BackdropCompositeScope::default();
    let textures = std::array::from_fn(|_| texture(&device, SIZE, RGBA));
    let p = parameters(0.5);
    let budget = p.scratch_budget_bytes;
    prepare(
        &device,
        &queue,
        &mut filter,
        &mut scope,
        inputs(&textures, [1; 3]),
        p,
    )
    .unwrap();
    let output = scope.contribution().unwrap().clone();
    let clean = EffectRasterStats {
        scratch_bytes: budget,
        ..Default::default()
    };
    for _ in 0..16 {
        assert_eq!(
            prepare(
                &device,
                &queue,
                &mut filter,
                &mut scope,
                inputs(&textures, [1; 3]),
                p
            )
            .unwrap(),
            clean
        );
    }
    let revision = scope.revision();
    let mut moved = p;
    moved.origin = [-5, -4];
    moved.viewport = [31, 27];
    let stats = prepare(
        &device,
        &queue,
        &mut filter,
        &mut scope,
        inputs(&textures, [1; 3]),
        moved,
    )
    .unwrap();
    assert_eq!(
        stats,
        EffectRasterStats {
            bytes_uploaded: size_of::<Placement>(),
            ..clean
        }
    );
    assert_eq!(scope.revision(), revision);
    assert_eq!(scope.contribution(), Some(&output));
    let mut zero = moved;
    zero.scope_opacity = 0.0;
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [1; 3]),
            zero
        )
        .unwrap(),
        clean
    );
    assert_eq!(scope.scratch_bytes(), budget);
    assert!(scope.contribution().is_none());
    assert!(scope.erase().is_none());
    assert!(scope.revision().is_none());
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [1; 3]),
            moved
        )
        .unwrap(),
        clean
    );
    for i in 1..=128 {
        let mut p = moved;
        p.scope_opacity = i as f32 / 128.0;
        let stats = prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, [1; 3]),
            p,
        )
        .unwrap();
        assert_eq!(
            stats,
            EffectRasterStats {
                bytes_uploaded: size_of::<Placement>(),
                composite_passes: 1,
                ..clean
            }
        );
        assert_eq!(scope.contribution(), Some(&output));
    }
    let mut p = moved;
    p.scope_opacity = 1.0;
    for role in 0..3 {
        let mut revisions = [1; 3];
        revisions[role] = 2;
        let stats = prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&textures, revisions),
            p,
        )
        .unwrap();
        assert_eq!(
            stats,
            EffectRasterStats {
                composite_passes: 1,
                ..clean
            }
        );
    }
    let replacement = std::array::from_fn(|_| texture(&device, SIZE, RGBA));
    let stats = prepare(
        &device,
        &queue,
        &mut filter,
        &mut scope,
        inputs(&replacement, [1; 3]),
        p,
    )
    .unwrap();
    assert_eq!(
        stats,
        EffectRasterStats {
            bind_group_creations: 1,
            composite_passes: 1,
            ..clean
        }
    );
    assert_eq!(scope.contribution(), Some(&output));
    let mut unrelated = BackdropCompositeScope::default();
    prepare(
        &device,
        &queue,
        &mut filter,
        &mut unrelated,
        inputs(&textures, [5; 3]),
        p,
    )
    .unwrap();
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&replacement, [1; 3]),
            p
        )
        .unwrap(),
        clean
    );
    scope.invalidate();
    assert_eq!(
        prepare(
            &device,
            &queue,
            &mut filter,
            &mut scope,
            inputs(&replacement, [1; 3]),
            p
        )
        .unwrap(),
        EffectRasterStats {
            composite_passes: 1,
            ..clean
        }
    );
    let smaller = std::array::from_fn(|_| texture(&device, [7, 5], BGRA));
    let stats = prepare(
        &device,
        &queue,
        &mut filter,
        &mut scope,
        inputs(&smaller, [1; 3]),
        p,
    )
    .unwrap();
    assert_eq!(
        (
            stats.pipeline_compiles,
            stats.texture_allocations,
            stats.buffer_allocations,
            stats.bind_group_creations,
            stats.composite_passes,
            stats.scratch_bytes
        ),
        (0, 2, 1, 3, 1, 7 * 5 * 8)
    );
    scope.clear();
    assert_eq!(scope.scratch_bytes(), 0);
    assert!(scope.contribution().is_none());
    assert!(scope.revision().is_none());
}

fn upload(queue: &wgpu::Queue, t: &wgpu::Texture, rgba: &[u8]) {
    let mut bytes = rgba.to_vec();
    if t.format() == BGRA {
        for p in bytes.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
    }
    queue.write_texture(
        t.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(t.width() * 4),
            rows_per_image: None,
        },
        t.size(),
    );
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture) -> Vec<u8> {
    let stride = (t.width() * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("backdrop qualification readback"),
        size: u64::from(stride) * u64::from(t.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        t.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: None,
            },
        },
        t.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut result = Vec::new();
    for row in mapped.chunks(stride as usize) {
        result.extend_from_slice(&row[..(t.width() * 4) as usize]);
    }
    if t.format() == BGRA {
        for p in result.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
    }
    result
}

// Independent scalar layer mixture: partition each pixel into uncovered area,
// target-painted area and glass-only area. Work in f64 straight target color,
// with no production shader, blend helper or intermediate images as a reference.
fn reference(destination: &[u8], fields: &[Vec<u8>; 3], p: BackdropCompositeParameters) -> Vec<u8> {
    let mut result = destination.to_vec();
    let opacity = f64::from(p.scope_opacity);
    for y in 0..VIEW[1] {
        for x in 0..VIEW[0] {
            let sx = x as i64 - i64::from(p.origin[0]);
            let sy = y as i64 - i64::from(p.origin[1]);
            if !(0..i64::from(SIZE[0])).contains(&sx) || !(0..i64::from(SIZE[1])).contains(&sy) {
                continue;
            }
            let at = (y * VIEW[0] + x) as usize * 4;
            let s = (sy * i64::from(SIZE[0]) + sx) as usize * 4;
            let mask = f64::from(fields[2][s + 3]) / 255.0;
            let paint_alpha = f64::from(fields[1][s + 3]) / 255.0;
            let painted_area = paint_alpha.min(mask);
            let glass_area = mask - painted_area;
            for channel in 0..4 {
                let target_color = if paint_alpha == 0.0 {
                    0.0
                } else {
                    f64::from(fields[1][s + channel]) / (255.0 * paint_alpha)
                };
                let preserved = f64::from(destination[at + channel]) * (1.0 - opacity * mask);
                let painted = 255.0 * target_color * opacity * painted_area;
                let glass = f64::from(fields[0][s + channel]) * opacity * glass_area;
                result[at + channel] =
                    (preserved + painted + glass).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    result
}
fn field(size: [u32; 2], kind: u32) -> Vec<u8> {
    let mut result = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let alpha = match (x + y * 3 + kind) % 7 {
                0 => 0,
                1 => 255,
                2 => 1,
                3 => 127,
                _ => (x * 31 + y * 47 + kind * 59) % 256,
            };
            result.extend([
                ((x * 43 + kind * 71) % 256 * alpha / 255) as u8,
                ((y * 37 + kind * 23) % 256 * alpha / 255) as u8,
                (((x + y) * 53 + kind * 97) % 256 * alpha / 255) as u8,
                alpha as u8,
            ]);
        }
    }
    result
}

#[test]
#[ignore = "requires a real raster adapter; independent clipped backdrop replacement oracle"]
fn backdrop_pixels_replace_transparent_prefix_clip_and_apply_scope_opacity_once() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("backdrop raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("backdrop adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let fields = [field(SIZE, 1), field(SIZE, 2), field(SIZE, 3)];
    let destination = field(VIEW, 4);
    let mut profiles = 0;
    let mut checked = 0;
    let mut maximum = 0;
    let mut exact_zero = 0;
    let mut exact_empty_pixels = 0;
    for target_format in [RGBA, BGRA] {
        let mut filter = BackdropCompositeFilter::new(target_format).unwrap();
        let mut scope = BackdropCompositeScope::default();
        let target = texture(&device, VIEW, target_format);
        for input_format in [RGBA, BGRA] {
            let sources = std::array::from_fn(|i| {
                let t = texture(&device, SIZE, if i == 1 { RGBA } else { input_format });
                upload(&queue, &t, &fields[i]);
                t
            });
            for origin in [[0, 0], [3, 2], [-4, -3], [19, 15], [-40, 0], [30, 0]] {
                for opacity in [0.0, 1.0 / 255.0, 0.25, 0.6, 1.0] {
                    let mut p = parameters(opacity);
                    p.origin = origin;
                    upload(&queue, &target, &destination);
                    let cold = filter.program.is_none();
                    let stats = render(
                        &device,
                        &queue,
                        &mut filter,
                        &mut scope,
                        inputs(&sources, [1; 3]),
                        p,
                        &target,
                    );
                    assert_eq!(
                        stats.pipeline_compiles,
                        if cold && opacity > 0.0 { 3 } else { 0 }
                    );
                    let actual = read(&device, &queue, &target);
                    let expected = reference(&destination, &fields, p);
                    retain_fixture(
                        &format!("case-{profiles:04}-{target_format:?}-{input_format:?}"),
                        &fields,
                        &destination,
                        &actual,
                        &expected,
                        p,
                    );
                    for y in 0..VIEW[1] {
                        for x in 0..VIEW[0] {
                            let sx = x as i64 - i64::from(origin[0]);
                            let sy = y as i64 - i64::from(origin[1]);
                            let outside = !(0..i64::from(SIZE[0])).contains(&sx)
                                || !(0..i64::from(SIZE[1])).contains(&sy);
                            let empty = outside
                                || fields[2][(sy * i64::from(SIZE[0]) + sx) as usize * 4 + 3] == 0;
                            if opacity == 0.0 || empty {
                                let at = (y * VIEW[0] + x) as usize * 4;
                                assert_eq!(&actual[at..at + 4], &destination[at..at + 4]);
                                exact_empty_pixels += 1;
                            }
                        }
                    }
                    for (channel, (&a, &e)) in actual.iter().zip(&expected).enumerate() {
                        let error = a.abs_diff(e);
                        maximum = maximum.max(error);
                        checked += 1;
                        assert!(error <= 2, "backdrop {target_format:?}/{input_format:?} origin={origin:?} opacity={opacity} channel={channel} actual={a} expected={e} error={error}");
                    }
                    if opacity == 0.0 || origin[0] == -40 || origin[0] == 30 {
                        assert_eq!(actual, destination);
                        exact_zero += 1;
                    }
                    let clean = prepare(
                        &device,
                        &queue,
                        &mut filter,
                        &mut scope,
                        inputs(&sources, [1; 3]),
                        p,
                    )
                    .unwrap();
                    assert_eq!(
                        clean,
                        EffectRasterStats {
                            scratch_bytes: scope.scratch_bytes(),
                            ..Default::default()
                        }
                    );
                    profiles += 1;
                }
            }
        }
    }
    negative_controls(&device, &queue);
    assert_eq!(profiles, 120);
    assert_eq!(exact_zero, 56);
    eprintln!("backdrop replacement: {profiles} profiles, {checked} scalar RGBA checks, maximum error {maximum}, {exact_zero} exact empty/zero/offscreen images, {exact_empty_pixels} exact uncovered/zero-opacity pixels");
}

fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    filter: &mut BackdropCompositeFilter,
    scope: &mut BackdropCompositeScope,
    captures: BackdropCompositeInputs<'_>,
    p: BackdropCompositeParameters,
    target: &wgpu::Texture,
) -> EffectRasterStats {
    let mut encoder = device.create_command_encoder(&Default::default());
    let stats = filter
        .prepare_encode(device, queue, &mut encoder, scope, captures, p)
        .unwrap();
    let view = target.create_view(&Default::default());
    {
        let attachment = Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("backdrop final replacement qualification"),
            color_attachments: &[attachment],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let placed = filter.encode(&mut pass, scope);
        assert_eq!(
            placed.composite_passes,
            if p.scope_opacity == 0.0 { 0 } else { 2 }
        );
    }
    queue.submit([encoder.finish()]);
    stats
}

// Compile and run actual corrupted shaders through the same GPU placement path.
// The independent layer reference and its acceptance bound remain unchanged.
fn negative_controls(device: &wgpu::Device, queue: &wgpu::Queue) {
    let fields = [field(SIZE, 1), field(SIZE, 2), field(SIZE, 3)];
    let destination = field(VIEW, 4);
    let sources = std::array::from_fn(|i| {
        let t = texture(device, SIZE, RGBA);
        upload(queue, &t, &fields[i]);
        t
    });
    let target = texture(device, VIEW, RGBA);
    let p = parameters(0.6);
    let expected = reference(&destination, &fields, p);
    let valid = include_str!("../backdrop_composite.wgsl");
    let overlay = valid.replace("mask * placement.opacity)", "result.contribution.a)");
    let twice = valid.replace(
        "* placement.opacity",
        "* placement.opacity * placement.opacity",
    );
    for (name, source) in [
        ("source-over instead of replacement", overlay),
        ("scope opacity applied twice", twice),
    ] {
        assert_ne!(source, valid);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(name),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let mut filter = BackdropCompositeFilter::new(RGBA).unwrap();
        filter.program = Some(Program::from_shader(device, RGBA, &shader));
        let mut scope = BackdropCompositeScope::default();
        upload(queue, &target, &destination);
        render(
            device,
            queue,
            &mut filter,
            &mut scope,
            inputs(&sources, [1; 3]),
            p,
            &target,
        );
        let actual = read(device, queue, &target);
        retain_fixture(
            if name.starts_with("source") {
                "negative-overlay"
            } else {
                "negative-double-opacity"
            },
            &fields,
            &destination,
            &actual,
            &expected,
            p,
        );
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(&a, &e)| a.abs_diff(e))
            .max()
            .unwrap();
        assert!(
            error >= 10,
            "negative control {name} escaped the unchanged byte bound 2: {error}"
        );
        eprintln!("backdrop negative control: {name}: maximum error {error} (accepted bound 2)");
    }
}

fn retain_fixture(
    name: &str,
    fields: &[Vec<u8>; 3],
    destination: &[u8],
    actual: &[u8],
    expected: &[u8],
    p: BackdropCompositeParameters,
) {
    let Some(root) = std::env::var_os("NOON_BACKDROP_ARTIFACT_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(root).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, data) in [
        ("processed-prefix.rgba", fields[0].as_slice()),
        ("target-paint.rgba", fields[1].as_slice()),
        ("original-coverage.rgba", fields[2].as_slice()),
        ("destination.rgba", destination),
        ("actual.rgba", actual),
        ("reference.rgba", expected),
    ] {
        std::fs::write(dir.join(name), data).unwrap();
    }
    std::fs::write(dir.join("profile.json"), format!(
        "{{\"schema\":1,\"color\":\"encoded-premultiplied-rgba8\",\"capture_size\":{:?},\"viewport\":{:?},\"origin\":{:?},\"scope_opacity\":{},\"maximum_accepted_byte_error\":2}}\n",
        SIZE, p.viewport, p.origin, p.scope_opacity,
    )).unwrap();
}

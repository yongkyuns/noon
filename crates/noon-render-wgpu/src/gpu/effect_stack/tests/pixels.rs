//! Actual GPU operator qualification; input pixels and reference composition are
//! constructed independently. This does not qualify authored stack/group scopes.
use super::*;

fn read(device: &wgpu::Device, queue: &wgpu::Queue, image: &wgpu::Texture) -> Vec<u8> {
    let row = image.width() * 4;
    let stride = row.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ordered operator qualification readback"),
        size: u64::from(stride) * u64::from(image.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        image.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(image.height()),
            },
        },
        image.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap()
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let bytes = buffer.get_mapped_range(..).unwrap();
    let result = bytes
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..(image.width() * 4) as usize].iter().copied())
        .collect();
    drop(bytes);
    buffer.unmap();
    result
}
fn upload(queue: &wgpu::Queue, image: &wgpu::Texture, bytes: &[u8]) {
    queue.write_texture(
        image.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width() * 4),
            rows_per_image: Some(image.height()),
        },
        image.size(),
    );
}
fn fixture(size: [u32; 2], kind: u32) -> (Vec<u8>, Vec<u8>) {
    let mut source = vec![0; (size[0] * size[1] * 4) as usize];
    let mut mask = source.clone();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let at = ((y * size[0] + x) * 4) as usize;
            let inside =
                (7..25).contains(&x) && (6..23).contains(&y) || (x == size[0] - 2 && y < 7);
            let alpha = if inside {
                ((x * 17 + y * 29) % 160 + 95) as u8
            } else {
                0
            };
            let pixel = match kind {
                0 if x == size[0] / 2 && y == size[1] / 2 => [83, 19, 131, 173],
                1 => [alpha / 2, alpha / 3, alpha / 5, alpha],
                2 if inside && (x + y) % 3 == 0 => [0, 0, 0, 0],
                2 => [alpha / 5, alpha / 2, alpha / 3, alpha],
                _ => [0; 4],
            };
            source[at..at + 4].copy_from_slice(&pixel);
            mask[at..at + 4].copy_from_slice(&[0, 0, 0, if inside { 255 } else { 0 }]);
        }
    }
    (source, mask)
}

#[test]
#[ignore = "requires a real native raster adapter; absence fails qualification"]
fn ordered_pixels_match_independent_composition_with_single_final_opacity() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("ordered effect raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("ordered effects adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let size = [37, 31];
    let image = texture(&device, size);
    let silhouette = texture(&device, size);
    let mut filter = EffectStackFilter::default();
    let hard_shadow: EffectDefinition = Shadow::new(
        EffectUpdate::default()
            .radius(Pixels(0.0))
            .offset(EffectOffset::Pixels([-2.5, 1.25]))
            .color(Color::rgba(0.9, 0.1, 0.2, 0.75)),
    )
    .unwrap()
    .into();
    let scene_blur: EffectDefinition = Blur::new(EffectUpdate::default().radius(0.05))
        .unwrap()
        .into();
    let profiles = [
        vec![blur(1.25), blur(2.25)],
        vec![blur(2.0), glow(1.5, GlowSource::Painted)],
        vec![glow(1.5, GlowSource::Painted), blur(2.0)],
        vec![blur(1.1), shadow(0.1)],
        vec![shadow(0.1), blur(1.1)],
        vec![glow(0.8, GlowSource::Painted), shadow(0.8), blur(0.6)],
        vec![shadow(0.1), glow(1.5, GlowSource::Silhouette)],
        vec![glow(1.5, GlowSource::Silhouette), shadow(0.1)],
        vec![
            glow(0.5, GlowSource::Painted),
            glow(1.5, GlowSource::Painted),
        ],
        vec![blur(0.0), blur(2.0), glow(0.0, GlowSource::Painted)],
        vec![glow(1.0, GlowSource::Painted), blur(0.0), shadow(0.1)],
        vec![hard_shadow, blur(0.8), glow(0.7, GlowSource::Silhouette)],
        vec![scene_blur, shadow(0.1)],
        vec![blur(64.0), glow(0.8, GlowSource::Painted)],
        vec![blur(0.8); MAX_EFFECT_STACK_LEN],
        vec![blur(1.25), dissolve(7, 0.08, 0.5)],
        vec![dissolve(7, 0.08, 0.5), blur(1.25)],
        vec![shadow(0.1), dissolve(u32::MAX, 0.5, 0.75)],
        vec![dissolve(11, 0.0, 0.25), glow(0.8, GlowSource::Painted)],
        vec![glow(0.8, GlowSource::Silhouette), dissolve(7, 0.08, 0.5)],
        vec![dissolve(7, 0.08, 0.5), glow(0.8, GlowSource::Silhouette)],
        vec![blur(1.25), dissolve(7, 0.08, 1.0)],
        vec![dissolve(7, 0.08, 1.0), glow(0.8, GlowSource::Silhouette)],
        vec![blur(1.25), dissolve(7, 0.08, 0.0)],
        vec![dissolve(7, 0.08, 0.25), dissolve(11, 0.5, 0.75)],
        vec![noon_core::Pixelate::default().into(), blur(1.25)],
        vec![blur(1.25), noon_core::Pixelate::default().into()],
        vec![
            noon_core::Pixelate::default().into(),
            dissolve(7, 0.08, 0.5),
        ],
        vec![
            dissolve(7, 0.08, 0.5),
            noon_core::Pixelate::default().into(),
        ],
        vec![
            noon_core::Pixelate::new(EffectUpdate::default().size(Pixels(1.0)))
                .unwrap()
                .into(),
            blur(1.25),
        ],
    ];
    let mut frames = 0;
    let mut channels = 0;
    let mut maximum = 0;
    for kind in 0..3 {
        let (input, mask) = fixture(size, kind);
        upload(&queue, &image, &input);
        upload(&queue, &silhouette, &mask);
        let capture = GlowCapture {
            source: &image,
            silhouette: Some(&silhouette),
            revision: u64::from(kind) + 1,
        };
        // Noncommutation is a required visible witness, not just a list-order assertion.
        let first = reference::evaluate(&input, &mask, size, &profiles[6], 1.0, 240, 8.0);
        let second = reference::evaluate(&input, &mask, size, &profiles[7], 1.0, 240, 8.0);
        assert!(
            first.iter().zip(second).any(|(&a, b)| a.abs_diff(b) > 3),
            "ineffective order witness"
        );
        for pair in [[15, 16], [19, 20], [21, 22]] {
            let first = reference::evaluate(&input, &mask, size, &profiles[pair[0]], 1.0, 240, 8.0);
            let second =
                reference::evaluate(&input, &mask, size, &profiles[pair[1]], 1.0, 240, 8.0);
            assert!(
                first.iter().zip(second).any(|(&a, b)| a.abs_diff(b) > 3),
                "ineffective mask order witness {pair:?}"
            );
        }
        for (profile, effects) in profiles.iter().enumerate() {
            for opacity in [1.0, 0.37] {
                let params = EffectStackParameters {
                    scope_opacity: opacity,
                    ..parameters()
                };
                let mut scope = EffectStackScope::default();
                // Each prefix is independently qualified, localizing operator errors.
                for end in 1..=effects.len() {
                    let work = render(
                        &mut filter,
                        &mut scope,
                        &device,
                        &queue,
                        capture,
                        &effects[..end],
                        params,
                    )
                    .unwrap();
                    let Some(output) = scope.output() else {
                        continue;
                    };
                    assert!(work.scratch_bytes <= params.scratch_budget_bytes);
                    let actual = read(&device, &queue, output);
                    let expected = reference::evaluate(
                        &input,
                        &mask,
                        size,
                        &effects[..end],
                        f64::from(opacity),
                        params.output_height,
                        params.world_view_height,
                    );
                    let error = actual
                        .iter()
                        .zip(&expected)
                        .map(|(&a, &b)| a.abs_diff(b))
                        .max()
                        .unwrap();
                    // Predeclared per-channel bound: two bytes for the mixed
                    // up-to-three-pass corpus, four for the eight-Gaussian
                    // boundary witness (successive quantization can accumulate).
                    let bound = if effects.len() == MAX_EFFECT_STACK_LEN {
                        4
                    } else {
                        2
                    };
                    assert!(error<=bound,"ordered reference mismatch: kind={kind} profile={profile} prefix={end} opacity={opacity} error={error}");
                    maximum = maximum.max(error);
                    channels += actual.len();
                    frames += 1;
                    let revision = scope.output_revision();
                    let clean = render(
                        &mut filter,
                        &mut scope,
                        &device,
                        &queue,
                        capture,
                        &effects[..end],
                        params,
                    )
                    .unwrap();
                    assert_eq!(
                        clean,
                        EffectRasterStats {
                            scratch_bytes: scope.scratch_bytes(),
                            ..Default::default()
                        }
                    );
                    assert_eq!(scope.output_revision(), revision);
                    assert_eq!(read(&device, &queue, scope.output().unwrap()), actual);
                }
                scope.invalidate();
                let recovered = render(
                    &mut filter,
                    &mut scope,
                    &device,
                    &queue,
                    capture,
                    effects,
                    params,
                )
                .unwrap();
                assert_eq!(recovered.texture_allocations, 0);
                assert!(recovered.composite_passes > 0);
                let expected = reference::evaluate(
                    &input,
                    &mask,
                    size,
                    effects,
                    f64::from(opacity),
                    params.output_height,
                    params.world_view_height,
                );
                let actual = read(&device, &queue, scope.output().unwrap());
                let bound = if effects.len() == MAX_EFFECT_STACK_LEN {
                    4
                } else {
                    2
                };
                assert!(actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, b)| a.abs_diff(b) <= bound));
            }
        }
    }
    eprintln!("ordered reference: {frames} prefix images / {channels} channels, maximum byte error {maximum}; clean exact, recovery reuses textures");
}

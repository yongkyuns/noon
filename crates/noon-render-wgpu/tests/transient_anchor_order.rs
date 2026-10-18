use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    Color, GeometryRef, ObjectContentRef, ObjectId, Style, Transform2D, Vec2, VectorPath,
};
use noon_render_wgpu::{prepare_derived_display, Camera2D, FramePreparer, GpuRenderer};
use noon_runtime::{
    SceneInstance, TransientAnchorSide, TransientPresentationOccurrence, TransientPresentationState,
};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 128;

fn state(geometry: GeometryRef, x: f32, color: Color) -> TransientPresentationState {
    let mut transform = Transform2D::IDENTITY;
    transform.translation = Vec2::new(x, 0.0);
    TransientPresentationState {
        z_index: 0.0,
        content: ObjectContentRef::Geometry(geometry),
        text_bounds: None,
        transform,
        style: Style {
            fill: Some(color),
            stroke: None,
            ..Style::default()
        },
        appearance: 1.0,
        presence: true,
        reveal: 1.0,
        morph: 0.0,
        render_geometry: None,
        render_transform: None,
    }
}
fn pixel(pixels: &[u8], x: u32) -> [u8; 4] {
    let offset = ((HEIGHT / 2 * WIDTH + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires software Vulkan; executed by Native Host Smoke"]
fn before_and_after_transients_preserve_pixels_across_stable_batch_boundaries() {
    for (packed_paths, reorder) in [(false, false), (true, false), (true, true)] {
        qualify_transient_anchor_order(packed_paths, reorder);
    }
}

fn qualify_transient_anchor_order(packed_paths: bool, reorder: bool) {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: true,
                ..Default::default()
            })
            .await
            .expect("transient anchor qualification requires Vulkan");
        eprintln!("transient anchor adapter: {:?}", adapter.get_info());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let positions = [-2.5, 0.0, 2.5];
        let objects = positions
            .iter()
            .enumerate()
            .map(|(index, &x)| {
                let geometry = if packed_paths {
                    GeometryRef::path(
                        VectorPath::new()
                            .move_to(Vec2::new(-0.6, 0.0))
                            .line_to(Vec2::new(0.6 + index as f32 * 0.02, 0.0)),
                    )
                } else if index == 1 {
                    GeometryRef::rectangle(1.2, 1.2)
                } else {
                    GeometryRef::circle(0.6)
                };
                let mut row = state(geometry.clone(), x, Color::WHITE);
                if packed_paths {
                    row.style.fill = None;
                    row.style.stroke = Some(Color::WHITE);
                    row.style.stroke_width = 1.2;
                    row.style.stroke_cap = noon_core::StrokeCap::Butt;
                    row.style.stroke_join = noon_core::StrokeJoin::Miter;
                }
                CompiledObject::new(
                    ObjectId::new(index as u64),
                    geometry,
                    row.transform,
                    row.style,
                )
            })
            .collect();
        let mut runtime = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
        let red_path = GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(-1.0, -1.0))
                .line_to(Vec2::new(1.0, -1.0))
                .line_to(Vec2::new(1.0, 1.0))
                .line_to(Vec2::new(-1.0, 1.0))
                .close(),
        );
        let mut occurrences = Vec::new();
        for (index, &x) in positions.iter().enumerate() {
            let anchor = index as u32;
            let base = anchor * 100;
            // Input order and ordinals intentionally disagree with anchor side.
            occurrences.push(TransientPresentationOccurrence::new(
                anchor,
                base + 1,
                state(GeometryRef::circle(0.2), x, Color::rgba(0.0, 1.0, 0.0, 1.0)),
            ));
            occurrences.push(
                TransientPresentationOccurrence::new(
                    anchor,
                    base + 30,
                    state(red_path.clone(), x, Color::rgba(1.0, 0.0, 0.0, 1.0)),
                )
                .with_anchor_side(TransientAnchorSide::Before),
            );
            occurrences.push(
                TransientPresentationOccurrence::new(
                    anchor,
                    base + 20,
                    state(
                        GeometryRef::circle(1.15),
                        x,
                        Color::rgba(0.0, 0.0, 1.0, 1.0),
                    ),
                )
                .with_anchor_side(TransientAnchorSide::Before),
            );
        }
        // These two occurrences overlap between adjacent stable sources. Their
        // ordering must remain After(A), Before(B), including when both sources
        // occupy adjacent spans of one packed draw.
        occurrences.push(TransientPresentationOccurrence::new(
            0,
            500,
            state(
                GeometryRef::circle(0.1),
                -1.25,
                Color::rgba(1.0, 0.0, 0.0, 1.0),
            ),
        ));
        occurrences.push(
            TransientPresentationOccurrence::new(
                1,
                600,
                state(
                    GeometryRef::circle(0.1),
                    -1.25,
                    Color::rgba(0.0, 1.0, 0.0, 1.0),
                ),
            )
            .with_anchor_side(TransientAnchorSide::Before),
        );
        let publication = runtime
            .take_renderer_publication()
            .with_transient_presentations(&occurrences)
            .unwrap();
        let derived = prepare_derived_display(&publication).unwrap();
        assert_eq!(derived.stats.painter_positions_visited, 0);
        assert_eq!(derived.stats.occurrences_packed, 11);
        let mut preparer = FramePreparer::new(&wgpu::Limits::default());
        preparer.set_painter_order(publication.frame(), publication.painter_order());
        let stable = if reorder {
            let original_indices = preparer
                .prepare(publication.frame())
                .mega_path_indices
                .to_vec();
            preparer.set_painter_order_range(publication.frame(), &[2, 0, 1], 0..3);
            let stable = preparer.prepare_incremental(
                publication.frame(),
                &noon_runtime::FrameChanges::default().with_painter_order(0..3),
            );
            assert_eq!(stable.stats.full_rebuilds, 0);
            assert_eq!(stable.stats.render_order_chunks_rebuilt, 1);
            assert_eq!(stable.mega_path_indices, original_indices);
            stable
        } else {
            preparer.prepare(publication.frame())
        };
        assert_eq!(
            stable.stats.mega_path_count,
            if packed_paths { 3 } else { 0 }
        );
        if packed_paths {
            assert_eq!(
                stable.stats.mega_path_batch_count,
                if reorder { 2 } else { 1 }
            );
        }
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        renderer.set_viewport(&device, &queue, WIDTH, HEIGHT);
        renderer.set_camera(
            &queue,
            Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 4.0)).unwrap(),
        );
        renderer.upload(&device, &queue, &stable);
        renderer.upload_transient_presentations(&device, &queue, &derived);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("transient anchor raster"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transient anchor readback"),
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        let drawn = renderer.encode_with_transient_presentations(
            &mut encoder,
            &texture.create_view(&Default::default()),
            &stable,
            &derived,
            wgpu::Color::BLACK,
        );
        assert_eq!(drawn.instances_drawn, 14);
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
        readback.unmap();
        let adjacent = pixel(&pixels, 88);
        assert!(
            adjacent[0] < 10 && adjacent[1] > 240 && adjacent[2] < 10,
            "Before(B) must paint above After(A) at their shared boundary: {adjacent:?}"
        );
        for center in [48, 128, 208] {
            let green = pixel(&pixels, center);
            assert!(
                green[0] < 10 && green[1] > 240 && green[2] < 10,
                "after-anchor occurrence must remain above stable content: {green:?}"
            );
            let white = pixel(&pixels, center + 12);
            assert!(
                white[0] > 240 && white[1] > 240 && white[2] > 240,
                "before-anchor occurrence must remain below stable content: {white:?}"
            );
            let red = pixel(&pixels, center + 25);
            assert!(
                red[0] > 240 && red[1] < 10 && red[2] < 10,
                "before-anchor occurrence ordinals must be preserved: {red:?}"
            );
        }
        if let Some(path) = std::env::var_os("NOON_TRANSIENT_ANCHOR_PROOF") {
            std::fs::write(path, &pixels).unwrap();
        }
    });
}

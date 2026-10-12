use wgpu::util::DeviceExt;

fn bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_ne_bytes()).collect()
}

fn render_offsets(source: &str, color: bool) -> Option<Vec<Vec<u8>>> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        return None;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: None,
        vertex: wgpu::VertexState {
            module: &shader, entry_point: Some("vs_glyph"), compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 56, step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32x2,3=>Float32x2,4=>Float32x2,5=>Float32x4],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader, entry_point: Some(if color { "fs_color" } else { "fs_mask" }), compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
    });
    let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &bytes(&[0.0, 0.0, 2.0 / 128.0, 2.0 / 128.0]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera.as_entire_binding(),
        }],
    });
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut outputs = Vec::new();
    let channels = if color { 4 } else { 1 };
    for [ox, oy] in [[2u32, 2u32], [213, 317], [777, 891]] {
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1024,
                height: 1024,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: if color {
                wgpu::TextureFormat::Rgba8UnormSrgb
            } else {
                wgpu::TextureFormat::R8Unorm
            },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut data = vec![0u8; 1024 * 1024 * channels];
        for y in 0..20u32 {
            for x in 0..32u32 {
                for channel in 0..channels {
                    data[((oy + y) * 1024 + ox + x) as usize * channels + channel] =
                        ((x * 17 + y * 23 + x * y * 3 + channel as u32 * 41) % 256) as u8;
                }
            }
        }
        queue.write_texture(
            atlas.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1024 * channels as u32),
                rows_per_image: Some(1024),
            },
            atlas.size(),
        );
        let av = atlas.create_view(&Default::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&av),
            }],
        });
        let angle = 0.173f32;
        let values = [
            -27.137,
            -14.291,
            51.37 * angle.cos(),
            51.37 * angle.sin(),
            -29.13 * angle.sin(),
            29.13 * angle.cos(),
            ox as f32 / 1024.0,
            oy as f32 / 1024.0,
            (ox + 32) as f32 / 1024.0,
            (oy + 20) as f32 / 1024.0,
            0.8,
            0.42,
            0.15,
            0.89,
        ];
        let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes(&values),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 128 * 128 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &camera_group, &[]);
            pass.set_bind_group(1, &group, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..6, 0..1);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(512),
                    rows_per_image: Some(128),
                },
            },
            target.size(),
        );
        queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        output.map_async(wgpu::MapMode::Read, .., move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let pixels = output.get_mapped_range(..).unwrap().to_vec();
        outputs.push(pixels);
        output.unmap();
    }
    Some(outputs)
}

fn assert_sampling_independent_of_atlas_origin(color: bool) {
    let Some(outputs) = render_offsets(include_str!("../src/text/glyph.wgsl"), color) else {
        eprintln!("skipping glyph atlas sampling regression: no native GPU adapter");
        return;
    };
    assert!(
        outputs[0].iter().any(|channel| *channel != 0),
        "sampling regression must render visible glyph pixels"
    );
    for pixels in &outputs[1..] {
        assert_eq!(
            pixels, &outputs[0],
            "identical glyph masks and quads must render identically at every atlas origin"
        );
    }
}

#[test]
fn mask_glyph_sampling_is_bitwise_independent_of_atlas_origin() {
    assert_sampling_independent_of_atlas_origin(false);
}

#[test]
fn color_glyph_sampling_is_bitwise_independent_of_atlas_origin() {
    assert_sampling_independent_of_atlas_origin(true);
}

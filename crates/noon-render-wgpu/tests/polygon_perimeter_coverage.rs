use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Polygon {
    points: [[f32; 2]; 4],
    count: u32,
    padding: [u32; 3],
}

// Independent f64 half-plane clipping oracle, rather than the shader's edge
// integral. Input coordinates remain the exact f32 values supplied to the GPU.
fn clipped_area(input: &Polygon) -> f64 {
    let mut points = input.points[..input.count as usize]
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect::<Vec<_>>();
    for (axis, boundary, greater) in [
        (0, 0.0, true),
        (0, 1.0, false),
        (1, 0.0, true),
        (1, 1.0, false),
    ] {
        let mut output = Vec::new();
        for i in 0..points.len() {
            let p = points[i];
            let q = points[(i + 1) % points.len()];
            let inside = |p: [f64; 2]| {
                if greater {
                    p[axis] >= boundary
                } else {
                    p[axis] <= boundary
                }
            };
            if inside(p) {
                output.push(p);
            }
            if inside(p) != inside(q) {
                let t = (boundary - p[axis]) / (q[axis] - p[axis]);
                output.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
            }
        }
        points = output;
    }
    (points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = points[(i + 1) % points.len()];
            p[0] * q[1] - p[1] * q[0]
        })
        .sum::<f64>()
        .abs()
        * 0.5)
        .clamp(0.0, 1.0)
}

#[test]
fn gpu_perimeter_coverage_matches_independent_clipping_for_thin_rotated_pieces() {
    let mut inputs = Vec::new();
    for scale in [0.0001_f32, 0.02, 1.0, 64.0, 4096.0] {
        for width in [0.0001, 0.05, 0.5] {
            for phase in [-0.3, 0.0, 0.37] {
                for step in 0..64 {
                    let angle = step as f32 * std::f32::consts::TAU / 64.0;
                    let along = [angle.cos() * scale, angle.sin() * scale];
                    let across = [-angle.sin() * width, angle.cos() * width];
                    let corners =
                        [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]].map(|[x, y]| {
                            [
                                phase + x * along[0] + y * across[0],
                                0.5 + x * along[1] + y * across[1],
                            ]
                        });
                    for count in [3, 4] {
                        for reverse in [false, true] {
                            let mut points = corners;
                            if reverse {
                                points[..count as usize].reverse();
                            }
                            inputs.push(Polygon {
                                points,
                                count,
                                padding: [0; 3],
                            });
                        }
                    }
                }
            }
        }
    }
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("skipping perimeter coverage GPU regression: no native adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let source = format!(
        "{}\n{}",
        include_str!("../src/polygon_coverage.wgsl"),
        r#"
struct Polygon {
    points: array<vec2<f32>, 4>, count: u32, padding: array<u32, 3>,
};
@group(0) @binding(0) var<storage, read> inputs: array<Polygon>;
@group(0) @binding(1) var<storage, read_write> outputs: array<f32>;
@compute @workgroup_size(64)
fn compute(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&inputs) { return; }
    let p = inputs[id.x];
    outputs[id.x] = polygon_perimeter_pixel_coverage(
        p.points[0], p.points[1], p.points[2], p.points[3], p.count);
}
"#
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Perimeter coverage regression"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("compute"),
        compilation_options: Default::default(),
        cache: None,
    });
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&inputs),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = inputs.len() as u64 * 4;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups((inputs.len() as u32).div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback.map_async(wgpu::MapMode::Read, .., move |result| {
        tx.send(result).unwrap()
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let data = readback.get_mapped_range(..).unwrap();
    for (input, bytes) in inputs.iter().zip(data.chunks_exact(4)) {
        let actual = f32::from_ne_bytes(bytes.try_into().unwrap());
        let expected = clipped_area(input);
        // Less than a quarter of one 8-bit channel, even for long thin edges.
        assert!(
            actual.is_finite() && (f64::from(actual) - expected).abs() < 1.0 / 1024.0,
            "{actual} != {expected} for {:?}",
            input.points
        );
    }
}

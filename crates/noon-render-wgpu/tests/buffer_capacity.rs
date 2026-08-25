use std::mem::size_of;

use noon_core::{GeometryRef, ObjectId, Style, Transform2D};
use noon_render_wgpu::{CircleInstance, FramePreparer, GpuRenderer};
use noon_runtime::{FrameChanges, FrameObjectState, FrameState};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[test]
fn analytic_instance_buffer_grows_geometrically_only_at_capacity_boundary() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let mut renderer = GpuRenderer::new(&device, FORMAT);
    let mut preparer = FramePreparer::new();

    let initial = circle_frame(1_000);
    let prepared = preparer.prepare(&initial);
    let first = renderer.upload(&device, &queue, &prepared);
    assert_eq!(first.buffer_reallocations, 1);

    let initial_capacity = renderer.circle_capacity_bytes();
    let instance_bytes = size_of::<CircleInstance>();
    assert!(initial_capacity.is_power_of_two());
    assert!(initial_capacity >= initial.objects.len() * instance_bytes);
    let max_instances_without_growth = initial_capacity / instance_bytes;
    assert!(max_instances_without_growth >= initial.objects.len());

    let at_capacity = circle_frame(max_instances_without_growth);
    let prepared = preparer.prepare(&at_capacity);
    let within = renderer.upload(&device, &queue, &prepared);
    assert_eq!(within.buffer_reallocations, 0);
    assert_eq!(renderer.circle_capacity_bytes(), initial_capacity);

    let crossing = circle_frame(max_instances_without_growth + 1);
    let prepared = preparer.prepare(&crossing);
    let growth = renderer.upload(&device, &queue, &prepared);
    assert_eq!(growth.buffer_reallocations, 1);
    assert_eq!(renderer.circle_capacity_bytes(), initial_capacity * 2);

    let prepared = preparer.prepare_incremental(&crossing, &FrameChanges::default());
    let steady = renderer.upload(&device, &queue, &prepared);
    assert_eq!(steady.buffer_reallocations, 0);
    assert_eq!(steady.bytes_uploaded, 0);
}

fn circle_frame(object_count: usize) -> FrameState {
    FrameState {
        time: 0.0,
        objects: (0..object_count)
            .map(|index| FrameObjectState {
                id: ObjectId::new(index as u64),
                geometry: GeometryRef::circle(0.5),
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            })
            .collect(),
        presences: vec![true; object_count],
        reveals: vec![1.0; object_count],
        morphs: vec![0.0; object_count],
        render_geometries: vec![None; object_count],
    }
}

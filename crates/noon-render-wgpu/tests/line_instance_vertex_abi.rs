use std::mem::{offset_of, size_of};

use noon_render_wgpu::{
    line_instance_layout, LineInstance, PackedStyle, PackedTransform,
};

#[test]
fn line_instance_vertex_layout_matches_packed_cpu_abi() {
    let layout = line_instance_layout();
    let attributes = layout.attributes;

    assert_eq!(layout.array_stride as usize, size_of::<LineInstance>());
    assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
    assert_eq!(attributes.len(), 10);

    let transform = offset_of!(LineInstance, transform);
    let style = offset_of!(LineInstance, style);
    let expected = [
        (
            transform + offset_of!(PackedTransform, translation),
            1,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            transform + offset_of!(PackedTransform, scale),
            2,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            transform + offset_of!(PackedTransform, rotation),
            3,
            wgpu::VertexFormat::Float32,
        ),
        (
            offset_of!(LineInstance, start),
            4,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            style + offset_of!(PackedStyle, fill),
            5,
            wgpu::VertexFormat::Float32x4,
        ),
        (
            style + offset_of!(PackedStyle, stroke),
            6,
            wgpu::VertexFormat::Float32x4,
        ),
        (
            style + offset_of!(PackedStyle, stroke_width),
            7,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            style + offset_of!(PackedStyle, fill_enabled),
            8,
            wgpu::VertexFormat::Uint32x2,
        ),
        (
            offset_of!(LineInstance, end),
            9,
            wgpu::VertexFormat::Float32x2,
        ),
        (
            transform + offset_of!(PackedTransform, padding),
            10,
            wgpu::VertexFormat::Float32,
        ),
    ];

    for (attribute, (offset, shader_location, format)) in attributes.iter().zip(expected) {
        assert_eq!(attribute.offset as usize, offset);
        assert_eq!(attribute.shader_location, shader_location);
        assert_eq!(attribute.format, format);
    }
}

#[test]
fn line_instance_partial_upload_stride_is_four_byte_aligned() {
    let stride = size_of::<LineInstance>();

    assert_eq!(stride, 88, "changing the line ABI requires updating GPU layout and diagnostics");
    assert_eq!(stride % wgpu::COPY_BUFFER_ALIGNMENT as usize, 0);
    assert_eq!(stride * 1, 88, "the second packed line starts at the frame-90 diagnostic offset");
}

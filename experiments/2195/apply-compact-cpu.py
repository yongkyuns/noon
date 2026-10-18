#!/usr/bin/env python3
"""Research-only, exact-base source transformation for issue #2195."""
import hashlib
from pathlib import Path

PATH = Path('crates/noon-render-wgpu/src/gpu/mod.rs')
ORIGINAL_BLOB = '6bcef9f03f53641a4e782289aa4437281f546da1'

METHOD = '''    /// Return `(logical_bytes, reserved_bytes)` for the CPU compact-vertex
    /// upload mirror owned by this renderer. Reserved bytes use the Vec's
    /// actual capacity, including retained high-water storage.
    ///
    /// This allocation is separate from the preparer's `path_cpu_buffer_bytes`
    /// and from the GPU handles in `geometry_buffer_residency`. This is not
    /// total CPU memory, physical VRAM, in-flight memory, or a resource budget.
    /// The read is O(1), allocates nothing and does not upload, shrink or wait.
    /// Ordinary empty uploads may retain capacity; explicit resident-buffer
    /// replacement replaces this CPU mirror through the existing lifecycle.
    ///
    /// ```no_run
    /// # fn example(renderer: &noon_render_wgpu::GpuRenderer) {
    /// let (logical_bytes, reserved_bytes) = renderer.compact_path_cpu_buffer_bytes();
    /// assert!(reserved_bytes >= logical_bytes);
    /// # }
    /// ```
    pub fn compact_path_cpu_buffer_bytes(&self) -> (u64, u64) {
        let stride = size_of::<CompactPathVertex>() as u64;
        (
            self.compact_path_vertices.len() as u64 * stride,
            self.compact_path_vertices.capacity() as u64 * stride,
        )
    }

'''

TESTS = '''    #[test]
    fn compact_path_cpu_accounting_reports_actual_capacity_without_mutation() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        assert_eq!(renderer.compact_path_cpu_buffer_bytes(), (0, 0));
        let gpu = renderer.geometry_buffer_residency();
        assert_eq!(gpu.compact_path_vertices, 4);

        renderer.compact_path_vertices.resize(3, CompactPathVertex::zeroed());
        renderer.compact_path_vertices.reserve_exact(17);
        let expected = (
            std::mem::size_of_val(renderer.compact_path_vertices.as_slice()) as u64,
            (renderer.compact_path_vertices.capacity() * size_of::<CompactPathVertex>()) as u64,
        );
        assert!(expected.1 > expected.0 && expected.0 > 0);
        let pointer = renderer.compact_path_vertices.as_ptr();
        let bytes = bytemuck::cast_slice::<_, u8>(&renderer.compact_path_vertices).to_vec();
        for _ in 0..3 {
            assert_eq!(renderer.compact_path_cpu_buffer_bytes(), expected);
            assert_eq!(renderer.compact_path_vertices.as_ptr(), pointer);
            assert_eq!(bytemuck::cast_slice::<_, u8>(&renderer.compact_path_vertices), bytes.as_slice());
            assert_eq!(renderer.geometry_buffer_residency(), gpu);
        }
        renderer.compact_path_vertices.clear();
        assert_eq!(renderer.compact_path_cpu_buffer_bytes(), (0, expected.1));
        assert_eq!(renderer.geometry_buffer_residency(), gpu);
    }

    #[test]
    fn compact_path_cpu_accounting_tracks_upload_reentry_and_compaction() {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let mut renderer = GpuRenderer::new(&device, &queue, FORMAT);
        let mut preparer = FramePreparer::new(&device.limits());
        let mut frame = test_frame_with_path();
        let empty_frame = test_frame();
        let cold = preparer.prepare(&frame);
        assert!(!cold.path_vertices.is_empty());
        let used = (cold.path_vertices.len() * size_of::<CompactPathVertex>()) as u64;
        renderer.upload(&device, &queue, &cold);
        let baseline = renderer.compact_path_cpu_buffer_bytes();
        assert_eq!(baseline.0, used);
        assert!(baseline.1 >= used);
        let pointer = renderer.compact_path_vertices.as_ptr();
        let gpu_compact = renderer.geometry_buffer_residency().compact_path_vertices;

        for cycle in 0..3 {
            let clean = preparer.prepare_incremental(&frame, &FrameChanges::default());
            let idle = renderer.upload(&device, &queue, &clean);
            assert_eq!((idle.bytes_uploaded, idle.buffer_reallocations), (0, 0));
            assert_eq!(renderer.compact_path_cpu_buffer_bytes(), baseline);
            assert_eq!(renderer.compact_path_vertices.as_ptr(), pointer);

            frame.objects[0].transform.translation = Vec2::new(0.1 * (cycle + 1) as f32, 0.0);
            let local = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
            assert!(local.path_vertex_dirty_ranges.is_empty());
            let mut writes = Vec::new();
            let upload = renderer.upload_with_trace(&device, &queue, &local, &mut writes);
            assert_eq!(upload.buffer_reallocations, 0);
            assert!(!writes.iter().any(|write| write.buffer == "compact_path_vertex"));
            assert_eq!(renderer.compact_path_cpu_buffer_bytes(), baseline);

            // An ordinary scene without paths clears length, not retained capacity.
            let hidden = preparer.prepare(&empty_frame);
            assert!(hidden.path_vertices.is_empty());
            renderer.upload(&device, &queue, &hidden);
            assert_eq!(renderer.compact_path_cpu_buffer_bytes(), (0, baseline.1));
            assert_eq!(renderer.compact_path_vertices.as_ptr(), pointer);
            assert_eq!(renderer.geometry_buffer_residency().compact_path_vertices, gpu_compact);

            let visible = preparer.prepare(&frame);
            renderer.upload(&device, &queue, &visible);
            assert_eq!(renderer.compact_path_cpu_buffer_bytes(), baseline);
            assert_eq!(renderer.compact_path_vertices.as_ptr(), pointer);
        }

        // Rejected replacement must not change either the CPU mirror or GPU handles.
        let gpu_before = renderer.geometry_buffer_residency();
        let nonempty = preparer.prepare(&frame);
        assert_eq!(
            renderer.replace_preloaded_path_buffers(&device, &queue, &nonempty).unwrap_err(),
            PathPreloadUploadError::NonEmptyDrawState,
        );
        assert_eq!(renderer.compact_path_cpu_buffer_bytes(), baseline);
        assert_eq!(renderer.compact_path_vertices.as_ptr(), pointer);
        assert_eq!(renderer.geometry_buffer_residency(), gpu_before);

        // Explicit empty compaction installs a fresh empty CPU mirror. GPU
        // placeholders remain; neither observation claims physical VRAM release.
        let empty_preparer = FramePreparer::new(&device.limits());
        let empty = empty_preparer.resident_upload_frame(crate::RenderStats::default());
        let replacement = renderer.replace_preloaded_path_buffers(&device, &queue, &empty).unwrap();
        assert_eq!(replacement.bytes_uploaded, 0);
        assert_eq!(renderer.compact_path_cpu_buffer_bytes(), (0, 0));
        assert_eq!(renderer.geometry_buffer_residency().compact_path_vertices, 4);

        preparer = FramePreparer::new(&device.limits());
        let reentered = preparer.prepare(&frame);
        assert!(renderer.upload(&device, &queue, &reentered).bytes_uploaded > 0);
        let after = renderer.compact_path_cpu_buffer_bytes();
        assert_eq!(after.0, used);
        assert!(after.1 >= after.0);
        let clean = preparer.prepare_incremental(&frame, &FrameChanges::default());
        let idle = renderer.upload(&device, &queue, &clean);
        assert_eq!((idle.bytes_uploaded, idle.buffer_reallocations), (0, 0));
        assert_eq!(renderer.compact_path_cpu_buffer_bytes(), after);
    }

'''

def main():
    data = PATH.read_bytes()
    blob = hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
    assert blob == ORIGINAL_BLOB, f'unexpected source blob {blob}'
    source = data.decode()
    changes = [
        ('    pub const fn circle_capacity_bytes(&self) -> usize {', METHOD),
        ('    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;', TESTS),
    ]
    for anchor, insert in changes:
        assert source.count(anchor) == 1, f'anchor changed: {anchor}'
        source = source.replace(anchor, insert + anchor)
    PATH.write_text(source)
    print(f'Applied getter and two regressions to {PATH}; no other source files touched.')

if __name__ == '__main__':
    main()

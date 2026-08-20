from pathlib import Path

path = Path("crates/noon-render-wgpu/src/gpu.rs")
text = path.read_text()


def replace_once(old: str, new: str) -> None:
    global text
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected source fragment not found:\n{old[:280]}")
    text = text.replace(old, new, 1)


replace_once(
    "    CircleInstance, LineInstance, PathInstance, PathVertex, PreparedFrame, RectangleInstance,\n",
    "    CircleInstance, LineInstance, PathBatch, PathInstance, PathVertex, PreparedFrame,\n    RectangleInstance,\n",
)
replace_once(
    "    path_instance_buffer: wgpu::Buffer,\n    circle_capacity_bytes: usize,\n",
    "    path_instance_buffer: wgpu::Buffer,\n    path_render_bundle: Option<wgpu::RenderBundle>,\n    path_render_bundle_batches: Vec<PathBatch>,\n    path_render_bundle_rebuilds: usize,\n    circle_capacity_bytes: usize,\n",
)
replace_once(
    "            path_instance_buffer,\n            circle_capacity_bytes: 0,\n",
    "            path_instance_buffer,\n            path_render_bundle: None,\n            path_render_bundle_batches: Vec::new(),\n            path_render_bundle_rebuilds: 0,\n            circle_capacity_bytes: 0,\n",
)
replace_once(
    "        buffer_reallocations += usize::from(path_instance_reallocated);\n\n        let bytes_uploaded = upload_dirty(\n",
    "        buffer_reallocations += usize::from(path_instance_reallocated);\n\n        self.prepare_path_render_bundle(\n            device,\n            prepared,\n            path_vertex_reallocated || path_index_reallocated || path_instance_reallocated,\n        );\n\n        let bytes_uploaded = upload_dirty(\n",
)
replace_once(
    "        UploadStats {\n            bytes_uploaded,\n            buffer_reallocations,\n        }\n    }\n\n    pub fn encode(\n",
    "        UploadStats {\n            bytes_uploaded,\n            buffer_reallocations,\n        }\n    }\n\n    fn prepare_path_render_bundle(\n        &mut self,\n        device: &wgpu::Device,\n        prepared: &PreparedFrame<'_>,\n        path_buffer_reallocated: bool,\n    ) {\n        if prepared.path_batches.is_empty() {\n            self.path_render_bundle = None;\n            self.path_render_bundle_batches.clear();\n            return;\n        }\n\n        let layout_changed = self.path_render_bundle_batches != prepared.path_batches;\n        if self.path_render_bundle.is_some() && !path_buffer_reallocated && !layout_changed {\n            return;\n        }\n\n        let color_formats = [Some(self.target_format)];\n        let mut bundle = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {\n            label: Some(\"Noon path render bundle encoder\"),\n            color_formats: &color_formats,\n            depth_stencil: None,\n            sample_count: PATH_SAMPLE_COUNT,\n            multiview: None,\n        });\n        bundle.set_bind_group(0, &self.camera_bind_group, &[]);\n        bundle.set_pipeline(&self.path_pipeline);\n        bundle.set_vertex_buffer(0, self.path_vertex_buffer.slice(..));\n        bundle.set_vertex_buffer(1, self.path_instance_buffer.slice(..));\n        bundle.set_index_buffer(self.path_index_buffer.slice(..), wgpu::IndexFormat::Uint32);\n        for batch in prepared\n            .path_batches\n            .iter()\n            .filter(|batch| !batch.index_range.is_empty())\n        {\n            bundle.draw_indexed(batch.index_range.clone(), 0, batch.instance_range.clone());\n        }\n        self.path_render_bundle = Some(bundle.finish(&wgpu::RenderBundleDescriptor {\n            label: Some(\"Noon path render bundle\"),\n        }));\n        self.path_render_bundle_batches.clear();\n        self.path_render_bundle_batches\n            .extend_from_slice(prepared.path_batches);\n        self.path_render_bundle_rebuilds += 1;\n    }\n\n    pub fn encode(\n",
)
replace_once(
    "            });\n            add_draw_stats(&mut stats, self.draw_paths(&mut pass, prepared));\n        }\n",
    "            });\n            if let Some(bundle) = &self.path_render_bundle {\n                pass.execute_bundles(std::iter::once(bundle));\n                add_draw_stats(&mut stats, path_draw_stats(prepared));\n            } else {\n                add_draw_stats(&mut stats, self.draw_paths(&mut pass, prepared));\n            }\n        }\n",
)
replace_once(
    "    pub const fn path_instance_capacity_bytes(&self) -> usize {\n        self.path_instance_capacity_bytes\n    }\n}\n\nfn add_draw_stats(total: &mut DrawStats, next: DrawStats) {\n",
    "    pub const fn path_instance_capacity_bytes(&self) -> usize {\n        self.path_instance_capacity_bytes\n    }\n\n    pub const fn path_render_bundle_rebuilds(&self) -> usize {\n        self.path_render_bundle_rebuilds\n    }\n}\n\nfn path_draw_stats(prepared: &PreparedFrame<'_>) -> DrawStats {\n    let mut stats = DrawStats::default();\n    for batch in prepared\n        .path_batches\n        .iter()\n        .filter(|batch| !batch.index_range.is_empty())\n    {\n        stats.draw_calls += 1;\n        stats.instances_drawn += batch.instance_range.len();\n    }\n    stats\n}\n\nfn add_draw_stats(total: &mut DrawStats, next: DrawStats) {\n",
)
replace_once(
    "        let upload = renderer.upload(&device, &queue, &prepared);\n        assert_eq!(upload.buffer_reallocations, 4);\n        assert!(upload.bytes_uploaded > size_of::<CircleInstance>());\n",
    "        let upload = renderer.upload(&device, &queue, &prepared);\n        assert_eq!(upload.buffer_reallocations, 4);\n        assert!(upload.bytes_uploaded > size_of::<CircleInstance>());\n        assert_eq!(renderer.path_render_bundle_rebuilds(), 1);\n",
)
replace_once(
    "        let unchanged_upload = renderer.upload(&device, &queue, &prepared);\n        assert_eq!(unchanged_upload.buffer_reallocations, 0);\n        assert_eq!(unchanged_upload.bytes_uploaded, 0);\n",
    "        let unchanged_upload = renderer.upload(&device, &queue, &prepared);\n        assert_eq!(unchanged_upload.buffer_reallocations, 0);\n        assert_eq!(unchanged_upload.bytes_uploaded, 0);\n        assert_eq!(renderer.path_render_bundle_rebuilds(), 1);\n",
)

path.write_text(text)

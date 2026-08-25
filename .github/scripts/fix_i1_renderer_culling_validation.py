from pathlib import Path

transport = Path('crates/noon-web/src/execution_transport.rs')
text = transport.read_text()
old = "        let patch = noon_ir::PatchBatch {\n            sequence: 0,\n            patches: vec![noon_core::ScenePatch::SetTransform {"
new = "        let patch = PatchBatch::new(0, vec![noon_core::ScenePatch::SetTransform {"
if old not in text:
    raise SystemExit('versioned PatchBatch test anchor missing')
text = text.replace(old, new, 1)
old = "            }],\n        };\n        let delta = engine"
new = "            }]);\n        let delta = engine"
if old not in text:
    raise SystemExit('PatchBatch closing test anchor missing')
transport.write_text(text.replace(old, new, 1))

gpu = Path('crates/noon-render-wgpu/src/gpu.rs')
text = gpu.read_text()
old = """        assert_eq!(ordered_render_sample_count(&[]), 1);
        assert_eq!(
            ordered_render_sample_count(&[PathBatch {
                index_range: 0..0,
                instance_range: 0..1,
            }]),
            1
        );
        assert_eq!(
            ordered_render_sample_count(&[PathBatch {
                index_range: 0..3,
                instance_range: 0..1,
            }]),
            PATH_SAMPLE_COUNT
        );
"""
new = """        assert_eq!(ordered_render_sample_count(&[]), 1);
        assert_eq!(
            ordered_render_sample_count(&[OrderedRenderBatch {
                primitive: RenderPrimitive::Circle,
                instance_range: 0..1,
            }]),
            1
        );
        assert_eq!(
            ordered_render_sample_count(&[OrderedRenderBatch {
                primitive: RenderPrimitive::Path { batch: 0 },
                instance_range: 0..1,
            }]),
            PATH_SAMPLE_COUNT
        );
"""
if old not in text:
    raise SystemExit('ordered sample-count test anchor missing')
gpu.write_text(text.replace(old, new, 1))

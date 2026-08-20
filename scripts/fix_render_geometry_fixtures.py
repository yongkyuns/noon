from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"expected render-geometry fixture not found in {path}: {old!r}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon-render-wgpu/src/gpu.rs",
    "            morphs: vec![0.0; 3],\n",
    "            morphs: vec![0.0; 3],\n            render_geometries: vec![None; 3],\n",
)
replace_once(
    "crates/noon-render-wgpu/src/gpu.rs",
    "            morphs: vec![0.0; 2],\n",
    "            morphs: vec![0.0; 2],\n            render_geometries: vec![None; 2],\n",
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    "        let morphs = vec![0.0; objects.len()];\n        FrameState {\n",
    "        let morphs = vec![0.0; objects.len()];\n        let render_geometries = vec![None; objects.len()];\n        FrameState {\n",
)
replace_once(
    "crates/noon-render-wgpu/src/lib.rs",
    "            morphs,\n        }\n",
    "            morphs,\n            render_geometries,\n        }\n",
)
replace_once(
    "crates/noon-render-wgpu/examples/frame_preparation_perf.rs",
    "        morphs: vec![0.0; object_count],\n",
    "        morphs: vec![0.0; object_count],\n        render_geometries: vec![None; object_count],\n",
)

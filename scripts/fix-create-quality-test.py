from pathlib import Path

path = Path('crates/noon-render-wgpu/src/lib.rs')
text = path.read_text()
old = '''        let cold = preparer.prepare(&frame);\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n        assert_eq!(cold.paths.len(), 1);\n        assert_eq!(cold.lines.len(), 1);\n        assert_eq!(cold.lines[0].start, cold.lines[0].end);\n        let head_before = cold.lines[0].start;\n'''
new = '''        let cold = preparer.prepare(&frame);\n        assert_eq!(cold.paths.len(), 1);\n        assert_eq!(cold.lines.len(), 1);\n        assert_eq!(cold.lines[0].start, cold.lines[0].end);\n        let head_before = cold.lines[0].start;\n        assert_eq!(preparer.cached_path_mesh_count(), 1);\n'''
if text.count(old) != 1:
    raise SystemExit('expected one path reveal borrow-test block')
path.write_text(text.replace(old, new, 1))

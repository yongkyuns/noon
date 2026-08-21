from pathlib import Path

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()
old = '        assert_eq!(steady.path_dirty_ranges, &[0..1]);\n'
new = '''        assert_eq!(steady.path_dirty_ranges.len(), 1);\n        assert_eq!(steady.path_dirty_ranges[0].start, 0);\n        assert_eq!(steady.path_dirty_ranges[0].end, 1);\n'''
if old not in text:
    raise SystemExit("filled renderer dirty-range assertion missing")
path.write_text(text.replace(old, new, 1))

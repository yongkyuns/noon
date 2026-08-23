from pathlib import Path

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()
old = "        assert_eq!(advanced.line_dirty_ranges, &[0..1]);\n"
new = "        assert_eq!(advanced.line_dirty_ranges.len(), 1);\n        assert_eq!(advanced.line_dirty_ranges[0], 0..1);\n"
if text.count(old) != 1:
    raise SystemExit("expected one line_dirty_ranges assertion")
path.write_text(text.replace(old, new, 1))

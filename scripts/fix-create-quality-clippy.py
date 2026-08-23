from pathlib import Path

path = Path('crates/noon-render-wgpu/src/lib.rs')
text = path.read_text()
replacements = {
    "        assert_eq!(prepared.path_dirty_ranges, &[0..1]);\n": "        assert_eq!(prepared.path_dirty_ranges.len(), 1);\n        assert_eq!(prepared.path_dirty_ranges[0], 0..1);\n",
    "        assert_eq!(prepared.line_dirty_ranges, &[0..1]);\n": "        assert_eq!(prepared.line_dirty_ranges.len(), 1);\n        assert_eq!(prepared.line_dirty_ranges[0], 0..1);\n",
    "        assert_eq!(advanced.line_dirty_ranges, &[1..2]);\n": "        assert_eq!(advanced.line_dirty_ranges.len(), 1);\n        assert_eq!(advanced.line_dirty_ranges[0], 1..2);\n",
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f'expected one match for {old.strip()}')
    text = text.replace(old, new, 1)
path.write_text(text)

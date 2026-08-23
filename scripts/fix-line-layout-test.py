from pathlib import Path

path = Path("crates/noon-render-wgpu/src/gpu.rs")
text = path.read_text()
old = '''        assert_eq!(line_layout.attributes.len(), 9);
        assert_eq!(line_layout.attributes[3].offset, 72);
        assert_eq!(line_layout.attributes[8].offset, 80);
        assert_eq!(line_layout.attributes[8].shader_location, 9);
'''
new = '''        assert_eq!(line_layout.attributes.len(), 10);
        assert_eq!(line_layout.attributes[3].offset, 72);
        assert_eq!(line_layout.attributes[8].offset, 80);
        assert_eq!(line_layout.attributes[8].shader_location, 9);
        assert_eq!(line_layout.attributes[9].offset, 20);
        assert_eq!(line_layout.attributes[9].shader_location, 10);
'''
if text.count(old) != 1:
    raise SystemExit("expected one line layout invariant block")
path.write_text(text.replace(old, new, 1))

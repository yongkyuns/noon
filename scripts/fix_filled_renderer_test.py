from pathlib import Path

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()
old = '        assert_eq!(steady.path_dirty_ranges, &[0..1]);\n'
new = '''        assert_eq!(steady.path_dirty_ranges.len(), 1);\n        assert_eq!(steady.path_dirty_ranges[0].start, 0);\n        assert_eq!(steady.path_dirty_ranges[0].end, 1);\n'''
if old not in text:
    raise SystemExit("filled renderer dirty-range assertion missing")
text = text.replace(old, new, 1)

old = '''        let mut state = object(7, GeometryRef::path(source));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
'''
new = '''        let mut state = object(7, GeometryRef::path(source));
        // This regression is specifically for the established stroke-only morph
        // path. `Style::default()` carries a fill, which now has real topology
        // semantics and would intentionally reject this open contour.
        state.style.fill = None;
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.2;
'''
if old not in text:
    raise SystemExit("stroke-only path morph test marker missing")
text = text.replace(old, new, 1)

old = '''                let mut state = object(index as u64, geometries[index % VARIANT_COUNT].clone());
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.02;
'''
new = '''                let mut state = object(index as u64, geometries[index % VARIANT_COUNT].clone());
                // Keep the 600-object stress regression scoped to stroke morphing;
                // filled morphs have their own topology/cache tests.
                state.style.fill = None;
                state.style.stroke = Some(Color::WHITE);
                state.style.stroke_width = 0.02;
'''
if old not in text:
    raise SystemExit("stroke-only morph stress test marker missing")
text = text.replace(old, new, 1)

path.write_text(text)

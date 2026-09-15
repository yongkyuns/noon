"""Finish SVG placement using the existing, distinct boundary/dimension authority."""
from pathlib import Path

def edit(name, before, after, count=1):
    p=Path(name); text=p.read_text()
    assert text.count(before)==count, (name,text.count(before),before[:80])
    p.write_text(text.replace(before,after))

p=Path('crates/noon-geometry/src/partial.rs')
s=p.read_text(); start=s.index('/// Bounds of the canonical cubic control points,')
end=s.index('#[derive(Clone, Copy, Debug)]',start)
s=s[:start]+s[end:];p.write_text(s)
edit('crates/noon/src/semantic_mobject/bounds.rs',
     'fn transformed_path_layout_bounds(', 'pub(crate) fn transformed_path_layout_bounds(')
edit('crates/noon/src/semantic_mobject.rs',
     'pub(crate) use bounds::{boundary_for_content, layout_for_content};',
     'pub(crate) use bounds::{boundary_for_content, layout_for_content, transformed_path_layout_bounds};')
edit('crates/noon/src/svg_authoring.rs',
     '    let bounds = aggregate_path_bounds(&leaves);\n    let transform = placement_transform(bounds, options);',
     '''    // Dimensions include canonical cubic handles; the centering boundary
    // contains anchors only. This is the same authority used by ordinary Mobjects.
    let dimensions = aggregate_path_bounds(&leaves, true);
    let boundary = aggregate_path_bounds(&leaves, false);
    let transform = placement_transform(dimensions, boundary, options);''')
edit('crates/noon/src/svg_authoring.rs',
     'fn aggregate_path_bounds(leaves: &[PreparedSvgLeaf]) -> Option<noon_core::Bounds2D64> {',
     'fn aggregate_path_bounds(leaves: &[PreparedSvgLeaf], include_handles: bool) -> Option<noon_core::Bounds2D64> {')
edit('crates/noon/src/svg_authoring.rs',
     '        let Some(bounds) = noon_geometry::cubic_control_point_bounds(&leaf.path) else {',
     '''        let Some(bounds) = crate::semantic_mobject::transformed_path_layout_bounds(
            &leaf.path, SemanticTransform2_5D::default(), include_handles,
        ) else {''')
edit('crates/noon/src/svg_authoring.rs',
     '''fn placement_transform(
    bounds: Option<noon_core::Bounds2D64>,
    options: SvgImportOptions,''',
     '''fn placement_transform(
    bounds: Option<noon_core::Bounds2D64>,
    boundary: Option<noon_core::Bounds2D64>,
    options: SvgImportOptions,''')
edit('crates/noon/src/svg_authoring.rs',
     '''    let center_x = (bounds.min_x + bounds.max_x) * 0.5;
    let center_y = (bounds.min_y + bounds.max_y) * 0.5;''',
     '''    let boundary = boundary.unwrap_or(bounds);
    let center_x = (boundary.min_x + boundary.max_x) * 0.5;
    let center_y = (boundary.min_y + boundary.max_y) * 0.5;''')
p=Path('crates/noon/src/svg_authoring.rs');s=p.read_text()
s=s.replace('assert!((bounds.height() - 1.5).abs() < 1e-6);', 'assert!((bounds.height() - 2.0).abs() < 1e-6);')
s=s.replace('assert!((bounds.min_y + 0.5).abs() < 1e-6);', 'assert!((bounds.min_y + 2.0).abs() < 1e-6);')
s=s.replace('assert!((bounds.max_y - 1.0).abs() < 1e-6);', 'assert!(bounds.max_y.abs() < 1e-6);')
p.write_text(s)
p=Path('scripts/tiger-manim-reference.py');s=p.read_text()
anchor='    tiger, rocket = make_objects()\n    saved = tiger.copy()'
assert s.count(anchor)==2
s=s.replace(anchor,'''    # Independent placement oracles: dimension bounds contain handles but center
    # is anchored at the endpoints. The visible bulge need not be centered.
    for name, commands, width in [
        ('cubic', 'M0 0 C0 12 12 12 12 0 Z', 2.0),
        ('quadratic', 'M0 0 Q6 12 12 0 Z', 3.0),
    ]:
        fixture = OUT / f'{name}-placement.svg'
        fixture.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="{commands}"/></svg>')
        shape = SVGMobject(str(fixture), height=2.0)
        assert abs(float(shape.width)-width) < 1e-8
        assert abs(float(shape.height)-2.0) < 1e-8
        assert np.max(np.abs(shape.get_center())) < 1e-8
        assert abs(float(shape.get_all_points()[:,1].max())) < 1e-8
        assert abs(float(shape.get_all_points()[:,1].min())+2.0) < 1e-8
    tiger, rocket = make_objects()
    saved = tiger.copy()''',1)
p.write_text(s)
print('SVG placement now shares ordinary Mobject dimension and boundary calculations; duplicate helper removed')

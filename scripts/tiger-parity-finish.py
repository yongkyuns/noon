"""Apply the reviewed parity corrections once, before exact-head validation."""
from pathlib import Path

def edit(name, before, after, count=1):
    p = Path(name)
    text = p.read_text()
    assert text.count(before) == count, (name, text.count(before), before[:80])
    p.write_text(text.replace(before, after))

edit('crates/noon-geometry/src/partial.rs',
     'const MANIM_LENGTH_SAMPLE_POINTS: usize = 10;',
     '''const MANIM_LENGTH_SAMPLE_POINTS: usize = 10;

/// Bounds of the canonical cubic control points, not the tight curve trace.
///
/// Manim's SVG centering and dimension fitting use its stored cubic point array.
/// Keep this explicit authoring query separate from tight layout/render bounds.
/// Lines and quadratic curves are promoted by the same canonical conversion used
/// for path interpolation. This is O(path commands) work at SVG import time.
pub fn cubic_control_point_bounds(path: &VectorPath) -> Option<noon_core::Bounds2D64> {
    if !path.is_finite() {
        return None;
    }
    let mut bounds: Option<noon_core::Bounds2D64> = None;
    let mut include = |point: SemanticVec3| {
        if let Some(bounds) = &mut bounds {
            bounds.include(point.x, point.y);
        } else {
            bounds = Some(noon_core::Bounds2D64::point(point.x, point.y));
        }
    };
    for curve in collect_curves(path) {
        for point in curve_controls(curve) {
            include(point);
        }
    }
    // A trailing moveto is a stored anchor even when it has no following curve.
    for command in path.commands() {
        if let PathCommand::MoveTo { to } = command {
            include(SemanticVec3::new(f64::from(to.x), f64::from(to.y), 0.0));
        }
    }
    bounds
}''')
edit('crates/noon/src/svg_authoring.rs',
     '    semantic_path_bounds, Color, SemanticMutationTransaction, SemanticNodeCreation,',
     '    Color, SemanticMutationTransaction, SemanticNodeCreation,')
edit('crates/noon/src/svg_authoring.rs',
     '        let Some(bounds) = semantic_path_bounds(&leaf.path, 0.0).layout else {',
     '        let Some(bounds) = noon_geometry::cubic_control_point_bounds(&leaf.path) else {')
edit('crates/noon/src/svg_authoring.rs',
     '    fn raw_options() -> SvgImportOptions {',
     '''    #[test]
    fn svg_cubic_fitting_uses_manim_control_points_not_tight_extrema() {
        // The curve reaches y=9, but its cubic handles reach y=12. Manim fits
        // the latter to height 2; visible curve height is therefore 1.5.
        let scene = Scene::new();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12">
            <path d="M0 0 C0 12 12 12 12 0 Z"/>
        </svg>"#;
        let family = scene.svg_from_str(svg).unwrap();
        let bounds = family.layout_bounds().unwrap().unwrap();
        assert!((bounds.width() - 2.0).abs() < 1e-6);
        assert!((bounds.height() - 1.5).abs() < 1e-6);
        assert!((bounds.min_y + 0.5).abs() < 1e-6);
        assert!((bounds.max_y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn svg_quadratic_fitting_uses_promoted_cubic_control_points() {
        // Q's handle at y=12 promotes to cubic handles at y=8. Fitting the
        // quadratic handle itself (12) or tight trace (6) would both be wrong.
        let scene = Scene::new();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12">
            <path d="M0 0 Q6 12 12 0 Z"/>
        </svg>"#;
        let family = scene.svg_from_str(svg).unwrap();
        let bounds = family.layout_bounds().unwrap().unwrap();
        assert!((bounds.width() - 3.0).abs() < 1e-6);
        assert!((bounds.height() - 1.5).abs() < 1e-6);
        assert!((bounds.min_y + 0.5).abs() < 1e-6);
        assert!((bounds.max_y - 1.0).abs() < 1e-6);
    }

    fn raw_options() -> SvgImportOptions {''')
edit('crates/noon-render-wgpu/tests/complex_filled_morph.rs',
     '.chunks_exact(3)', '.as_chunks::<3>().0.iter()')
for name in ['scripts/playground-python-family-transform-smoke.mjs',
             'scripts/playground-tiger-morph-smoke.mjs']:
    edit(name, 'JSON.stringify(result, null, 2)',
         "JSON.stringify(result, (_, value) => typeof value === 'bigint' ? value.toString() : value, 2)")
edit('scripts/tiger-manim-reference.py', '"background_color": "#111111"', '"background_color": "#000000"')
edit('scripts/tiger-manim-reference.py', 'pixels[:,:,:3].astype(int)-17', 'pixels[:,:,:3].astype(int)')
edit('scripts/tiger-manim-differential.mjs',
     "runtimeReference: '116d4003de85648559299960da9394195f619501'",
     "runtimeReference: JSON.parse(await readFile(path.join(root, 'web/runtime-build-identity.json'), 'utf8'))")
# Old one-shot staging data is not product code and must not enter the final PR.
for name in ['scripts/.tiger-fix.patch.part1', 'scripts/.apply-tiger-fix.py']:
    Path(name).unlink(missing_ok=True)
print('Applied SVG control-point placement, strict lint, and lossless diagnostic serialization')

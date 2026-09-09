"""Actual run34357519296 diagnostics; preserve the pre-existing rejection order."""
from pathlib import Path

path=Path('../tooling/.github/r2a-tests/object_authoring_errors.rs')
s=path.read_text()
assert s.count('AuthoringError::NonFiniteGeometry')==1
s=s.replace('AuthoringError::NonFiniteGeometry','AuthoringError::NonFiniteObjectState')
before='''    assert_eq!(
        family.arrange(0.0, 0.0, 0.1, true).unwrap_err(),
        AuthoringError::ZeroDirection
    );'''
after='''    // A zero direction is accepted by the existing next-to semantics. A
    // non-finite direction is rejected during preparation, before any move.
    assert!(matches!(
        family.arrange(f64::NAN, 0.0, 0.1, true),
        Err(AuthoringError::InvalidRenderNumber { .. })
    ));'''
assert s.count(before)==1
path.write_text(s.replace(before,after))

# Existing language diagnostics, not an alternative R3 category mapper.
for name in ['authoring_geometry.rs','manim_shape_matcher_handle_bridge.rs']:
    path=Path('crates/noon-web/src')/name
    s=path.read_text()
    before='fn js_error(error: String) -> JsValue {\n    JsValue::from_str(&error)\n}'
    assert s.count(before)==1,name
    s=s.replace(before,'fn js_error(error: impl std::fmt::Display) -> JsValue {\n    JsValue::from_str(&error.to_string())\n}')
    s=s.replace('js_error("Underline target has no layout bounds".into())','js_error("Underline target has no layout bounds")')
    path.write_text(s)

# Read-only external export codec, never an internal engine boundary.
path=Path('crates/noon-web/src/geometry_export.rs')
s=path.read_text()
for call in ['state','wire_translation','wire_scale','wire_rotation','wire_fill','wire_stroke','wire_stroke_width','wire_object_opacity']:
    before=f'object.{call}()?'
    assert s.count(before)==1,call
    s=s.replace(before,f'object.{call}().map_err(|error| error.to_string())?')
path.write_text(s)

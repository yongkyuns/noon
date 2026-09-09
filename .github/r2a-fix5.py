"""Only existing external/diagnostic String sinks; shared producers stay typed."""
from pathlib import Path

def replace(path, before, after, count=1):
    path = Path(path)
    source = path.read_text()
    assert source.count(before) == count, (str(path), before, source.count(before))
    path.write_text(source.replace(before, after))

replace('crates/noon-web/src/canonical_authoring_scene.rs',
        'let frame = self.scene.camera_frame()?;',
        'let frame = self.scene.camera_frame().map_err(|error| error.to_string())?;')
path = Path('crates/noon-web/src/determinism.rs')
source = path.read_text()
start = source.index('fn create_morph_fade_session()')
head, tail = source[:start], source[start:]
for expression, count in [
    ('scene.circle(0.75)', 3),
    ('entering.set_translation(-2.0, 0.0)', 1),
    ('scene.square(1.5)', 1),
    ('leaving.set_translation(2.0, 0.0)', 1),
]:
    before = expression + '?'
    assert tail.count(before) == count, (expression, tail.count(before))
    tail = tail.replace(before, expression + '.map_err(|error| error.to_string())?')
path.write_text(head + tail)

# This is the existing JsValue diagnostic sink, not a new error mapper. R3 owns
# replacement by the chosen typed mapper; there is no implicit Rust -> String.
replace('crates/noon-web/src/authoring_mobject.rs',
        'fn js_error(error: String) -> JsValue {\n        JsValue::from_str(&error)\n    }',
        'fn js_error(error: impl std::fmt::Display) -> JsValue {\n        JsValue::from_str(&error.to_string())\n    }')
replace('crates/noon-web/src/authoring_mobject.rs',
        'render_f64(field, value)? as f32',
        'render_f64(field, value).map_err(|error| error.to_string())? as f32')

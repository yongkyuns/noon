from pathlib import Path
from textwrap import dedent

rust_path = Path("crates/noon-web/src/authoring_mobject.rs")
rust = rust_path.read_text()

frontend_marker = "    pub fn next_to_handle(\n"
frontend_insert = dedent(
    '''
    pub fn disable_fill(&mut self) {
        self.snapshot.style.fill = None;
    }

    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = Color::rgba(
            finite_f32("fill.red", red)?,
            finite_f32("fill.green", green)?,
            finite_f32("fill.blue", blue)?,
            unit_alpha("fill.alpha", alpha)?,
        );
        let alpha = self.snapshot.style.fill.map_or(color.alpha, |current| current.alpha);
        self.snapshot.style.fill = Some(Color { alpha, ..color });
        Ok(())
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let alpha = unit_alpha("fill opacity", opacity)?;
        let mut color = self.snapshot.style.fill.unwrap_or(Color::WHITE);
        color.alpha = alpha;
        self.snapshot.style.fill = Some(color);
        Ok(())
    }

    pub fn fill_opacity(&self) -> f64 {
        self.snapshot.style.fill.map_or(0.0, |color| f64::from(color.alpha))
    }

    pub fn disable_stroke(&mut self) {
        self.snapshot.style.stroke = None;
    }

    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = Color::rgba(
            finite_f32("stroke.red", red)?,
            finite_f32("stroke.green", green)?,
            finite_f32("stroke.blue", blue)?,
            unit_alpha("stroke.alpha", alpha)?,
        );
        let alpha = self
            .snapshot
            .style
            .stroke
            .map_or(color.alpha, |current| current.alpha);
        self.snapshot.style.stroke = Some(Color { alpha, ..color });
        Ok(())
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        let width = finite_f32("stroke width", width)?;
        if width < 0.0 {
            return Err("stroke width must be non-negative".to_owned());
        }
        self.snapshot.style.stroke_width = width;
        if self.snapshot.style.stroke.is_none() {
            self.snapshot.style.stroke = Some(Color::WHITE);
        }
        Ok(())
    }

    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let alpha = unit_alpha("stroke opacity", opacity)?;
        let mut color = self.snapshot.style.stroke.unwrap_or(Color::WHITE);
        color.alpha = alpha;
        self.snapshot.style.stroke = Some(color);
        Ok(())
    }

    pub fn stroke_opacity(&self) -> f64 {
        self.snapshot
            .style
            .stroke
            .map_or(0.0, |color| f64::from(color.alpha))
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let alpha = unit_alpha("opacity", opacity)?;
        if let Some(fill) = &mut self.snapshot.style.fill {
            fill.alpha = alpha;
        }
        if let Some(stroke) = &mut self.snapshot.style.stroke {
            stroke.alpha = alpha;
        }
        Ok(())
    }

'''
)
frontend_insert = "".join(f"    {line}\n" if line else "\n" for line in frontend_insert.splitlines())
if rust.count(frontend_marker) != 1:
    raise SystemExit("frontend insertion marker mismatch")
rust = rust.replace(frontend_marker, frontend_insert + frontend_marker, 1)

helper_marker = "fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {\n"
helper_insert = dedent(
    '''
fn unit_alpha(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be between 0 and 1"));
    }
    Ok(value)
}

'''
)
if rust.count(helper_marker) != 1:
    raise SystemExit("alpha helper marker mismatch")
rust = rust.replace(helper_marker, helper_insert + helper_marker, 1)

wasm_marker = "        #[wasm_bindgen(js_name = nextToHandle)]\n"
wasm_insert = dedent(
    '''
#[wasm_bindgen(js_name = disableFill)]
pub fn disable_fill(&mut self) {
    self.0.disable_fill();
}

#[wasm_bindgen(js_name = setFillColor)]
pub fn set_fill_color(
    &mut self,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), JsValue> {
    self.0
        .set_fill_color(red, green, blue, alpha)
        .map_err(js_error)
}

#[wasm_bindgen(js_name = setFillOpacity)]
pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
    self.0.set_fill_opacity(opacity).map_err(js_error)
}

#[wasm_bindgen(getter, js_name = fillOpacity)]
pub fn fill_opacity(&self) -> f64 {
    self.0.fill_opacity()
}

#[wasm_bindgen(js_name = disableStroke)]
pub fn disable_stroke(&mut self) {
    self.0.disable_stroke();
}

#[wasm_bindgen(js_name = setStrokeColor)]
pub fn set_stroke_color(
    &mut self,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<(), JsValue> {
    self.0
        .set_stroke_color(red, green, blue, alpha)
        .map_err(js_error)
}

#[wasm_bindgen(js_name = setStrokeWidth)]
pub fn set_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
    self.0.set_stroke_width(width).map_err(js_error)
}

#[wasm_bindgen(js_name = setStrokeOpacity)]
pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
    self.0.set_stroke_opacity(opacity).map_err(js_error)
}

#[wasm_bindgen(getter, js_name = strokeOpacity)]
pub fn stroke_opacity(&self) -> f64 {
    self.0.stroke_opacity()
}

#[wasm_bindgen(js_name = setOpacity)]
pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
    self.0.set_opacity(opacity).map_err(js_error)
}

'''
)
wasm_insert = "".join(f"        {line}\n" if line else "\n" for line in wasm_insert.splitlines())
if rust.count(wasm_marker) != 1:
    raise SystemExit("wasm insertion marker mismatch")
rust = rust.replace(wasm_marker, wasm_insert + wasm_marker, 1)

test_marker = "    #[test]\n    fn json_round_trip_preserves_wire_snapshot() {\n"
test_insert = dedent(
    '''
#[test]
fn shared_style_mutations_preserve_independent_channels() {
    let mut value = snapshot(GeometryRef::circle(1.0));
    value.style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.4));
    value.style.stroke = Some(Color::rgba(0.0, 0.0, 1.0, 0.7));
    let mut handle = FrontendMobjectHandle::from_snapshot(value);

    handle.set_fill_color(0.0, 1.0, 0.0, 1.0).unwrap();
    assert!((handle.fill_opacity() - 0.4).abs() < 1e-6);
    handle.set_fill_opacity(0.25).unwrap();
    handle.set_stroke_width(3.5).unwrap();
    handle.set_stroke_opacity(0.6).unwrap();
    assert!((handle.fill_opacity() - 0.25).abs() < 1e-6);
    assert!((handle.stroke_opacity() - 0.6).abs() < 1e-6);
    assert!((handle.snapshot().style.stroke_width - 3.5).abs() < 1e-6);

    handle.set_opacity(0.2).unwrap();
    assert!((handle.fill_opacity() - 0.2).abs() < 1e-6);
    assert!((handle.stroke_opacity() - 0.2).abs() < 1e-6);
    handle.disable_fill();
    assert_eq!(handle.fill_opacity(), 0.0);
}

'''
)
test_insert = "".join(f"    {line}\n" if line else "\n" for line in test_insert.splitlines())
if rust.count(test_marker) != 1:
    raise SystemExit("test insertion marker mismatch")
rust = rust.replace(test_marker, test_insert + test_marker, 1)
rust_path.write_text(rust)

py_path = Path("web/python/_manim_semantic_handles.py")
py = py_path.read_text()
import_marker = "import _manim_compat as _compat\n"
if py.count(import_marker) != 1:
    raise SystemExit("python import marker mismatch")
py = py.replace(import_marker, import_marker + "import _manim_phase_b as _phase_b\n", 1)

original_marker = "_ORIGINAL_ALIGN_ON_FRAME = _base.Mobject._align_on_frame\n"
originals = dedent(
    '''
_ORIGINAL_SET_FILL = _compat.VMobject.set_fill
_ORIGINAL_SET_STROKE = _compat.VMobject.set_stroke
_ORIGINAL_SET_OPACITY = _compat.VMobject.set_opacity
_ORIGINAL_GET_FILL_OPACITY = _compat.VMobject.get_fill_opacity
_ORIGINAL_GET_STROKE_OPACITY = _compat.VMobject.get_stroke_opacity
'''
)
if py.count(original_marker) != 1:
    raise SystemExit("python original marker mismatch")
py = py.replace(original_marker, original_marker + originals, 1)

bounds_marker = "def _compat_bounds_for(value: object) -> tuple[_base.Vec2, _base.Vec2] | None:\n"
style_functions = dedent(
    '''
def _set_fill(
    self: _compat.VMobject,
    color: object = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_FILL(self, color=color, opacity=opacity, family=family)
    if color is not None:
        parsed = _phase_b._as_color("fill color", color)
        handle.setFillColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif opacity is None:
        handle.disableFill()
    if opacity is not None:
        handle.setFillOpacity(_phase_b._opacity("fill opacity", opacity))
    return self


def _set_stroke(
    self: _compat.VMobject,
    color: object = None,
    width: float | None = None,
    opacity: float | None = None,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_STROKE(
            self, color=color, width=width, opacity=opacity, family=family
        )
    if color is not None:
        parsed = _phase_b._as_color("stroke color", color)
        handle.setStrokeColor(parsed.red, parsed.green, parsed.blue, parsed.alpha)
    elif width is None and opacity is None:
        handle.disableStroke()
    if width is not None:
        value = _base._ir._finite_number("stroke width", width)
        if value < 0.0:
            raise ValueError("stroke width must be non-negative")
        handle.setStrokeWidth(value)
    if opacity is not None:
        handle.setStrokeOpacity(_phase_b._opacity("stroke opacity", opacity))
    return self


def _set_opacity(
    self: _compat.VMobject,
    opacity: float,
    family: bool = True,
) -> _compat.VMobject:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_SET_OPACITY(self, opacity, family=family)
    handle.setOpacity(_phase_b._opacity("opacity", opacity))
    return self


def _get_fill_opacity(self: _compat.VMobject) -> float:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_GET_FILL_OPACITY(self)
    return float(handle.fillOpacity)


def _get_stroke_opacity(self: _compat.VMobject) -> float:
    handle = _handle_for(self)
    if handle is None:
        return _ORIGINAL_GET_STROKE_OPACITY(self)
    return float(handle.strokeOpacity)


'''
)
if py.count(bounds_marker) != 1:
    raise SystemExit("python style insertion marker mismatch")
py = py.replace(bounds_marker, style_functions + bounds_marker, 1)

install_marker = "    _compat.VMobject.copy = _copy_mobject\n    _compat._bounds_for = _compat_bounds_for\n"
install_replace = dedent(
    '''
    _compat.VMobject.copy = _copy_mobject
    _compat.VMobject.set_fill = _set_fill
    _compat.VMobject.set_stroke = _set_stroke
    _compat.VMobject.set_opacity = _set_opacity
    _compat.VMobject.get_fill_opacity = _get_fill_opacity
    _compat.VMobject.get_stroke_opacity = _get_stroke_opacity
    _compat._bounds_for = _compat_bounds_for
'''
)
install_replace = "".join(f"    {line}\n" if line else "\n" for line in install_replace.splitlines())
if py.count(install_marker) != 1:
    raise SystemExit("python install marker mismatch")
py = py.replace(install_marker, install_replace, 1)
py_path.write_text(py)

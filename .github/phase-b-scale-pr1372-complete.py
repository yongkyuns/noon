from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    file.write_text(text.replace(old, new, 1))


def replace_between(path: str, start: str, end: str, replacement: str) -> None:
    file = Path(path)
    text = file.read_text()
    first = text.find(start)
    if first < 0:
        raise SystemExit(f"{path}: start marker not found: {start!r}")
    second = text.find(end, first)
    if second < 0:
        raise SystemExit(f"{path}: end marker not found: {end!r}")
    if text.find(start, first + 1) >= 0:
        raise SystemExit(f"{path}: start marker is not unique")
    file.write_text(text[:first] + replacement + text[second:])


def insert_before_once(path: str, marker: str, insertion: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{path}: expected one insertion marker, found {count}")
    file.write_text(text.replace(marker, insertion + marker, 1))


def write_new(path: str, content: str) -> None:
    file = Path(path)
    if file.exists():
        raise SystemExit(f"{path}: new file already exists")
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(content)


# Complete the existing #1372 shared Rust pivot surface through the WASM handle.
replace_once(
    "crates/noon-web/src/authoring_mobject.rs",
    '''        pub fn scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.manim_scale(x, y).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
''',
    '''        pub fn scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.manim_scale(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = scaleAboutPoint)]
        pub fn scale_about_point(
            &mut self,
            x: f64,
            y: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .manim_scale_about_point(x, y, point_x, point_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = scaleAboutEdge)]
        pub fn scale_about_edge(
            &mut self,
            x: f64,
            y: f64,
            edge_x: f64,
            edge_y: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .manim_scale_about_edge(x, y, edge_x, edge_y)
                .map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
''',
)

# Retain typed AuthoringFailure projection and reuse the one LiveSession operation.
insert_before_once(
    "crates/noon-web/src/semantic_execution_player.rs",
    '''    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_set_rotation(
''',
    '''    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_about_point(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.manim_scale_about_point(mobject, x, y, point_x, point_y)
        })
        .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn live_scale_about_edge(
        &mut self,
        mobject: &noon::Mobject,
        x: f64,
        y: f64,
        edge_x: f64,
        edge_y: f64,
    ) -> Result<(), AuthoringFailure> {
        self.with_live_session(|live| {
            live.manim_scale_about_edge(mobject, x, y, edge_x, edge_y)
        })
        .map(|_| ())
    }

''',
)

insert_before_once(
    "crates/noon-web/src/canonical_authoring_scene.rs",
    '''        #[wasm_bindgen(js_name = liveSetRotation)]
''',
    '''        #[wasm_bindgen(js_name = liveScaleAboutPoint)]
        pub fn live_scale_about_point(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_about_point(handle.semantic_mobject(), x, y, point_x, point_y)
                .map_err(typed_js_error)
        }

        #[wasm_bindgen(js_name = liveScaleAboutEdge)]
        pub fn live_scale_about_edge(
            &mut self,
            handle: &crate::WasmAuthoringMobjectHandle,
            x: f64,
            y: f64,
            edge_x: f64,
            edge_y: f64,
        ) -> Result<(), JsValue> {
            self.inner
                .active_live_player()
                .map_err(typed_js_error)?
                .live_scale_about_edge(handle.semantic_mobject(), x, y, edge_x, edge_y)
                .map_err(typed_js_error)
        }

''',
)

# Python owns only argument coercion/precedence. Shared Rust resolves geometry and commits.
replace_between(
    "web/python/_manim_semantic_handles.py",
    "def _scale(self: _base.Mobject, factor: object)",
    "def _rotate(\n",
    '''def _scale(
    self: _base.Mobject,
    factor: object,
    *,
    about_point: object | None = None,
    about_edge: object | None = None,
) -> _base.Mobject:
    handle = _handle_for(self)
    if handle is None:
        raise RuntimeError("Mobject edits require a current shared Rust semantic handle")
    scalar_factor = not isinstance(factor, (tuple, list, _base.Vec2))
    if scalar_factor:
        scalar = float(factor)
        value = _base.Vec2(scalar, scalar)
    else:
        scalar = None
        value = _base._as_vec2(factor)
    if (about_point is not None or about_edge is not None) and scalar is None:
        raise TypeError("Manim scale pivots require a scalar scale_factor")

    context = _live_mutation_context(self)
    try:
        if about_point is not None:
            pivot = _base._as_vec2(about_point)
            if context is None:
                engine_call(handle.scaleAboutPoint, scalar, scalar, pivot.x, pivot.y)
            else:
                engine_call(context.liveScaleAboutPoint, handle, scalar, scalar, pivot.x, pivot.y)
        elif about_edge is not None:
            edge = _base._as_vec2(about_edge)
            if context is None:
                engine_call(handle.scaleAboutEdge, scalar, scalar, edge.x, edge.y)
            else:
                engine_call(context.liveScaleAboutEdge, handle, scalar, scalar, edge.x, edge.y)
        elif context is None:
            engine_call(handle.scale, value.x, value.y)
        else:
            engine_call(context.liveScale, handle, value.x, value.y)
    except Exception as error:
        raise_engine_error(error)
    return self


''',
)

# Add a renderer-independent pinned-Manim semantic probe covering precedence too.
insert_before_once(
    "scripts/manim-differential.py",
    "def _noon_rotated_rectangle() -> Any:\n",
    '''def _scale_pivots(api) -> Any:
    centered = api.Square(side_length=1.0).shift(2.0 * api.RIGHT + api.UP)
    center_before = _point_observation(centered.get_center())
    centered.scale(1.5)

    point = api.Square(side_length=1.0).shift(api.RIGHT + api.DOWN)
    point.scale(1.5, about_point=api.ORIGIN)

    edge = api.Square(side_length=1.0).shift(3.0 * api.RIGHT + api.UP)
    edge_right_before = _point_observation(edge.get_right())
    edge.scale(1.5, about_edge=api.RIGHT)

    precedence = api.Square(side_length=1.0).shift(2.0 * api.RIGHT)
    precedence.scale(1.5, about_point=api.ORIGIN, about_edge=api.RIGHT)

    return {
        "center_before": center_before,
        "centered": _object_observation(centered),
        "point": _object_observation(point),
        "edge": _object_observation(edge),
        "edge_right_before": edge_right_before,
        "edge_right_after": _point_observation(edge.get_right()),
        "precedence": _object_observation(precedence),
    }


def _noon_scale_pivots() -> Any:
    return _scale_pivots(noon)


def _manim_scale_pivots() -> Any:
    return _scale_pivots(manim)


''',
)
replace_once(
    "scripts/manim-differential.py",
    '''    Fixture("scaled_square", _noon_scaled_square, _manim_scaled_square),
    Fixture("rotated_rectangle", _noon_rotated_rectangle, _manim_rotated_rectangle),
''',
    '''    Fixture("scaled_square", _noon_scaled_square, _manim_scaled_square),
    Fixture("scale_pivots", _noon_scale_pivots, _manim_scale_pivots),
    Fixture("rotated_rectangle", _noon_rotated_rectangle, _manim_rotated_rectangle),
''',
)

# Exact Manim-compatible source used by the canonical raster/timeline corpus.
parity_source = '''from manim import *


class ScalePivots(Scene):
    def construct(self):
        centered = Square(side_length=1).set_fill("#4488FF", opacity=1).set_stroke(width=0)
        point = Square(side_length=1).set_fill("#44CC88", opacity=1).set_stroke(width=0)
        edge = Square(side_length=1).set_fill("#FF8844", opacity=1).set_stroke(width=0)
        centered.shift(3 * LEFT + UP)
        point.shift(DOWN)
        edge.shift(3 * RIGHT + UP)
        self.add(centered, point, edge)
        self.wait(4 / 30)
        centered.scale(1.5)
        point.scale(1.5, about_point=ORIGIN)
        edge.scale(1.5, about_edge=RIGHT)
        self.wait(8 / 30)
'''
write_new("parity/manim-v0.21/core-examples/scale_pivots.py", parity_source)
write_new(
    "web/python/examples/manim_parity_scale_pivots.py",
    parity_source.replace("from manim import *", "from noon import *", 1),
)

manifest_path = Path("parity/manim-v0.21/manifest.json")
manifest = manifest_path.read_text()
manifest_marker = '''    }
  ],
  "sample_fractions": [
'''
position = manifest.rfind(manifest_marker)
if position < 0:
    raise SystemExit("parity manifest fixture-list terminator not found")
fixture = '''    },
    {
      "id": "scale-pivots",
      "scene": "ScalePivots",
      "source": "parity/manim-v0.21/core-examples/scale_pivots.py",
      "expected_duration": 0.4
  ],
  "sample_fractions": [
'''
manifest_path.write_text(manifest[:position] + fixture + manifest[position + len(manifest_marker):])

# Equivalent typed Rust scene, used by native and direct Rust/WASM renderer hosts.
write_new(
    "crates/noon/src/example_scenes/manim_scale_pivots.rs",
    '''//! Paired with manim_parity_scale_pivots.py and the pinned ManimCE scale-pivots fixture.
use crate::{ExecutionSession, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut centered = scene.square(1.0).map_err(|e| e.to_string())?;
    let mut point = scene.square(1.0).map_err(|e| e.to_string())?;
    let mut edge = scene.square(1.0).map_err(|e| e.to_string())?;
    for (object, color) in [
        (&mut centered, (68.0 / 255.0, 136.0 / 255.0, 1.0)),
        (&mut point, (68.0 / 255.0, 204.0 / 255.0, 136.0 / 255.0)),
        (&mut edge, (1.0, 136.0 / 255.0, 68.0 / 255.0)),
    ] {
        object
            .set_fill(color.0, color.1, color.2, 1.0)
            .map_err(|e| e.to_string())?;
        object.set_stroke_width(0.0).map_err(|e| e.to_string())?;
    }
    centered.shift(-3.0, 1.0).map_err(|e| e.to_string())?;
    point.shift(0.0, -1.0).map_err(|e| e.to_string())?;
    edge.shift(3.0, 1.0).map_err(|e| e.to_string())?;
    scene
        .add_many(&[(&centered).into(), (&point).into(), (&edge).into()])
        .map_err(|e| e.to_string())?;

    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let before = live.wait_segment(4.0 / 30.0).map_err(|e| e.to_string())?;
        live.advance_segment_to(before, before.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(before).map_err(|e| e.to_string())?;

        live.manim_scale(&centered, 1.5, 1.5)
            .map_err(|e| e.to_string())?;
        live.manim_scale_about_point(&point, 1.5, 1.5, 0.0, 0.0)
            .map_err(|e| e.to_string())?;
        live.manim_scale_about_edge(&edge, 1.5, 1.5, 1.0, 0.0)
            .map_err(|e| e.to_string())?;

        let after = live.wait_segment(8.0 / 30.0).map_err(|e| e.to_string())?;
        live.advance_segment_to(after, after.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(after).map_err(|e| e.to_string())?;
    }
    Ok(session)
}
''',
)
replace_once(
    "crates/noon/src/example_scenes.rs",
    "pub mod live_updater_lifecycle;\n",
    "pub mod live_updater_lifecycle;\npub mod manim_scale_pivots;\n",
)
write_new(
    "crates/noon-native/examples/manim_scale_pivots.rs",
    '''fn main() -> Result<(), Box<dyn std::error::Error>> {
    noon_native::run(noon::example_scenes::manim_scale_pivots::session()?)?;
    Ok(())
}
''',
)

replace_once(
    "crates/noon-web/src/direct_execution_smoke.rs",
    '''/// Shared family mutation semantics run directly through the Rust/WASM engine.
#[wasm_bindgen(js_name = createDirectFamilyPaintSmokeRenderer)]
''',
    '''/// Manim scale center/point/edge pivots run through the same typed Rust scene as native.
#[wasm_bindgen(js_name = createDirectManimScalePivotsSmokeRenderer)]
pub async fn create_direct_manim_scale_pivots_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::manim_scale_pivots::session().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

/// Shared family mutation semantics run directly through the Rust/WASM engine.
#[wasm_bindgen(js_name = createDirectFamilyPaintSmokeRenderer)]
''',
)
replace_once(
    "scripts/browser-smoke.mjs",
    '''  { name: "Family affine", factory: "createDirectFamilyAffineSmokeRenderer", objectCount: 2, duration: 0.2 },
''',
    '''  { name: "Family affine", factory: "createDirectFamilyAffineSmokeRenderer", objectCount: 2, duration: 0.2 },
  { name: "Manim scale pivots", factory: "createDirectManimScalePivotsSmokeRenderer", objectCount: 3, duration: 0.4 },
''',
)
replace_once(
    "scripts/shared-authoring-smoke.mjs",
    '''    { filename: "ordinary_family_affine.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
''',
    '''    { filename: "ordinary_family_affine.py", objectCount: 2, expectedDuration: 0.2, endpointTime: null },
    { filename: "manim_parity_scale_pivots.py", objectCount: 3, expectedDuration: 0.4, endpointTime: null },
''',
)

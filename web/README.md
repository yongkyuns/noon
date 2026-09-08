# Noon WebGPU demo

The direct Rust/WASM examples author, lower, execute, and render through typed Rust boundaries in one WASM context. The optional Pyodide authoring worker uses the same shared semantic operations; its separate execution worker is an explicit cross-context transport boundary.

From the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

| Direct Rust/WASM qualification | Shared Rust builder | Browser-owned input |
| --- | --- | --- |
| Sequential ordinary affine play | `noon::example_scenes::ordinary_affine_play()` | None; the typed Rust session owns both plays, the wait, and the authored edit |
| Flat ordinary composition | `noon::example_scenes::ordinary_composition_play()` | None; the typed Rust session owns Parallel/Sequence scheduling and completion |
| Default Succession | `noon::example_scenes::ordinary_succession_program()` | None; matches the four-dot Python tutorial with Smooth child curves under its linear sequence root |
| Flat composition continuation | `noon::example_scenes::ordinary_composition_continuation_program()` | None; the typed Rust program owns both composition segments and their renderer admission barriers |
| Ordinary FadeIn/FadeOut | `noon::example_scenes::ordinary_fade_continuation_program()` | None; the typed Rust program owns appearance, membership, detached wait, and same-handle re-entry |
| Parallel Create | `noon::example_scenes::ordinary_square_and_circle_create_continuation_program()` | None; matches Python `manim_parity_square_and_circle.py` with one atomic admission and reveal segment |
| MovingCameraCenter | `noon::example_scenes::ordinary_moving_camera_center_program()` | None; shared camera transforms and live admission after a wait match the literal Python scene |
| Ordinary Uncreate | `noon::example_scenes::ordinary_uncreate_continuation_program()` | None; detached admission, reverse reveal, and endpoint removal share the same runtime |
| Uncreate options | `noon::example_scenes::ordinary_uncreate_options::program()` / `python/examples/ordinary_uncreate_options.py` | None; shared Rust resolves easing, reveal direction and optional endpoint removal |
| Static Typst / MathTypst | `noon::example_scenes::typst_text_reference()` / `math_typst_text_reference()` | None; shared semantic text resources supply glyphs and vector geometry |
| Ordinary Create | `noon::example_scenes::ordinary_create_continuation_program()` | None; the typed Rust program owns introduction, reveal, endpoint reconciliation, and continuation admission |
| Create → SquareToCircle → FadeOut | `noon::example_scenes::ordinary_create_then_content_morph_program()` | None; native and direct-WASM hosts run the same typed content and lifecycle continuation |
| Ordinary style play | `noon::example_scenes::ordinary_style_play()` | None; the typed Rust session owns fill/object-opacity interpolation, completion, and the following authored style edit |
| Ordinary paint play | `noon::example_scenes::ordinary_paint_play()` | None; the typed Rust session owns fill/stroke color and paint-opacity interpolation, completion, and the following authored paint edit |
| Scene membership | `noon::example_scenes::ordinary_membership::program()` | None; shared Rust owns batch validation, ordered roots, family promotion, replacement and clear |
| Ordered subset display | `noon::example_scenes::ordinary_subset_display::program()` | None; shared Rust owns family preparation, detached admission, exact increasing/one-by-one thresholds, and paint endpoints |
| Mixed Text Write | `noon::example_scenes::text_write::program()` | None; shared Rust composes typed Text movement with ordered glyph Write and atomic admission |
| Native signals | `noon::example_scenes::live_native_signals()` | Typed pointer, Space-key, opacity-control, and ordered pointer-down occurrences; no scene JSON or semantic IDs |

## Curated examples

The **Example** picker is intentionally a teaching sequence rather than a feature dump. Each scene has one primary purpose and one unique source file:

1. **Getting started** — primitives plus semantic movement/rotation/opacity authoring.
2. **Analytic Transform** — circle radius, rectangle size, and line endpoint interpolation without path conversion.
3. **Lifecycle handoffs** — `ReplacementTransform` versus `TransformFromCopy` presence semantics.
4. **Fade & appearance** — `FadeOut`/`FadeIn` while preserving authored semantic opacity.
5. **Matching shapes** — deterministic `TransformMatchingShapes` pairing by shape signature.
6. **Path reveal** — one multi-contour path over the ordered reveal domain.
7. **Filled path Transform** — validated fixed-topology interpolation from a rounded loop to a star.
8. **Staggered timing** — identical motion with only timing varied.
9. **Instanced field · 180** — analytic batching and dirty instance uploads on a semantic grid.
10. **Morph stress · 1,000** — one deliberately dense profiling scene with twelve reusable morph targets.

Gallery scenes execute through shared Rust semantic sessions. Source-owned `play()`/`wait()` continuations determine their duration; they are not constrained to a fixed four-second loop. Browser qualification exercises this path in CI.

## Semantic Python authoring

Layout vocabulary is part of `noon` itself; there is no separate browser-only layout scene model:

```python
from noon import *

scene = Scene()

left = Circle(0.45, color=BLUE)
right = Square(0.9, color=PINK).next_to(left, RIGHT)
scene.add(left, right)

scene.play(
    left.animate.shift(UP),
    right.animate.rotate(45 * DEGREES),
    run_time=1.5,
)
scene.play(Transform(left, Square(1.0, color=PURPLE)), run_time=1.0)

result = scene
```

`Vec2`, `ORIGIN`, direction/corner constants, named colors, object-aware layout, `Group`/`VGroup`, `run_time`, `wait`, and `.animate` use shared Rust semantic operations. Python owns authoring ergonomics and arbitrary Python callback invocation; Rust owns semantic and execution state.

Sequential `.animate` operations are evaluated at semantic scene time. A later animation therefore starts from the exact endpoint authored by the previous animation rather than the object's original base snapshot.

Transform semantics are authored in Rust and lowered into renderer-independent execution data. Analytic primitives and fixed path geometry remain distinct execution representations.

## Live authoring

Edit **Python scene source** and click **Run** to author and attach a shared semantic execution session. A rerun replaces the session; it does not currently qualify incremental hot reload or identity preservation between independently authored scenes. Within a running session, shared Rust semantics own object identity and mutations.

Python loads lazily in a separate Pyodide worker. The normal playground does not serialize scene documents or allocate frontend semantic identities. Source continuations attach early so Rust execution can release each `play()`/`wait()` barrier. Direct Rust/WASM examples do not require Python.

The worker loads the pinned Pyodide distribution from jsDelivr, so first-time Python authoring requires network access. The Rust/WASM package remains local under `web/pkg/`.

`node scripts/authoring-perf.mjs` measures cold authoring, unchanged source reruns, one-object source edits, and static seek control round trips through shared sessions. Schema 2 explicitly reports session replacements and unavailable isolated CPU/GPU, incremental-mutation, and camera-uniform timings. It cannot be compared with the removed scene-document profiler. `node scripts/perf-corpus.mjs` measures representative authored scenes, including moving cameras, through shared execution.

## Vector paths

Generic paths are semantic command streams and remain distinct from analytic circle/rectangle/line fast paths. The Rust and Python APIs support move, line, quadratic, cubic, and close commands:

```python
from noon import BLUE, WHITE, Path, Scene, VectorPath

curve = (
    VectorPath()
    .move_to((-1.0, 0.0))
    .quadratic_to((0.0, 1.5), (1.0, 0.0))
    .line_to((0.0, -1.0))
    .close()
)

scene = Scene()
scene.add(Path(curve, fill=BLUE, stroke=WHITE, stroke_width=0.06))
```

Static path meshes are tessellated once per semantic path/topology cache key and reused across instances. Geometry-changing path Transforms and supported cross-kind analytic Transforms prepare a fixed source/target pair before playback; steady frames update only compact instance state rather than retessellating geometry.

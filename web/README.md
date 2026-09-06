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
| Flat composition continuation | `noon::example_scenes::ordinary_composition_continuation_program()` | None; the typed Rust program owns both composition segments and their renderer admission barriers |
| Ordinary FadeIn/FadeOut | `noon::example_scenes::ordinary_fade_continuation_program()` | None; the typed Rust program owns appearance, membership, detached wait, and same-handle re-entry |
| Parallel Create | `noon::example_scenes::ordinary_square_and_circle_create_continuation_program()` | None; matches Python `manim_parity_square_and_circle.py` with one atomic admission and reveal segment |
| Ordinary Create | `noon::example_scenes::ordinary_create_continuation_program()` | None; the typed Rust program owns introduction, reveal, endpoint reconciliation, and continuation admission |
| Create → SquareToCircle → FadeOut | `noon::example_scenes::ordinary_create_then_content_morph_program()` | None; native and direct-WASM hosts run the same typed content and lifecycle continuation |
| Ordinary style play | `noon::example_scenes::ordinary_style_play()` | None; the typed Rust session owns fill/object-opacity interpolation, completion, and the following authored style edit |
| Ordinary paint play | `noon::example_scenes::ordinary_paint_play()` | None; the typed Rust session owns fill/stroke color and paint-opacity interpolation, completion, and the following authored paint edit |
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

All picker scenes must execute through the Python authoring layer, compile through the native Rust `ScenePlayer`, and finish before the playground's four-second loop. The same validation runs in CI and in the Pages build.

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

`Vec2`, `ORIGIN`, direction/corner constants, named colors, object-aware layout, `Group`/`VGroup`, `run_time`, `wait`, and `.animate` all lower to the same versioned `SceneDocument`. The frontend does not introduce a second persistent semantic representation.

Sequential `.animate` operations are evaluated at semantic scene time. A later animation therefore starts from the exact endpoint authored by the previous animation rather than the object's original base snapshot.

Cross-kind `Circle <-> Rectangle/Square` Transform is also semantic: the `SceneDocument` retains analytic source/target snapshots. The Rust compiler creates temporary fixed path geometry only for rendering the active transition. Same-kind Circle/Rectangle/Line Transforms remain analytic.

## Live authoring

Open **Python scene source** and click **Run Python scene** to build a complete versioned `SceneDocument`. Explicit object and track `key` values retain runtime identity across Python reruns. Compatible style, transform, and timeline edits reconcile into semantic patches; unsafe geometry or draw-order changes fall back to transactional replacement. Both paths preserve the playhead and existing canvas/GPU resources and restart ordered patch sequencing at zero.

**Run Python patch** sends an incremental `PatchBatch` to that persistent runtime. The first Python action lazily downloads the pinned Pyodide runtime; playback continues while Python loads or runs, and deployed scenes still work without Pyodide when authoring controls are unused.

The worker loads Pyodide `314.0.5` from the official jsDelivr distribution, so Python authoring requires network access. The render/runtime wasm package remains local under `web/pkg/`.

The worker protocol carries Pyodide's already-encoded result JSON across the thread boundary and parses it once on the main thread. This avoids structured-cloning a large JavaScript object graph. Run `node web/scene-pipeline-perf.mjs` to benchmark transfer, parsing, validation, identity stabilization, diffing, and serialization independently at 1k/10k/100k objects.

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

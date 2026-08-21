# Noon WebGPU demo

This demo proves that a serialized Noon scene can compile, evaluate, and render in a browser without Python, while an optional Pyodide worker can author complete scenes or ordered live patches without blocking playback.

From the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

Open **Python scene source** and click **Run Python scene** to build a complete versioned `SceneDocument`. Explicit object and track `key` values retain runtime identity across Python reruns. Compatible style, transform, and timeline edits reconcile into semantic patches; unsafe geometry or draw-order changes fall back to transactional replacement. Both paths preserve the playhead and existing canvas/GPU resources and restart ordered patch sequencing at zero. **Run Python patch** then sends an incremental `PatchBatch` to that persistent runtime. The first Python action lazily downloads the pinned Pyodide runtime; playback continues while Python loads or runs, and deployed scenes still work without Pyodide when the authoring controls are unused.

The worker loads Pyodide `314.0.5` from the official jsDelivr distribution, so the Python control requires network access. The render/runtime wasm package remains local under `web/pkg/`.

The worker protocol carries Pyodide's already-encoded result JSON across the thread boundary and parses it once on the main thread. This avoids structured-cloning a large JavaScript object graph. Run `node web/scene-pipeline-perf.mjs` to benchmark transfer, parsing, validation, identity stabilization, diffing, and serialization independently at 1k/10k/100k objects.

For real-browser renderer profiling, open <http://localhost:8080/gpu-profile.html?objects=100000&warmup=30&frames=180>. The page reports synchronous CPU submission time, WebGPU render-pass timestamps when supported, and `requestAnimationFrame` cadence separately. Its static fixed-resolution circle grid measures instance/draw scaling rather than worst-case overdraw; full methodology and the dated baseline are in `docs/performance.md`.

Generic paths are semantic command streams and remain distinct from the analytic circle/rectangle/line fast paths. The Rust and Python APIs support move, line, quadratic, cubic, and close commands. Python example:

```python
from noon import Color, Scene, VectorPath

curve = (
    VectorPath()
    .move_to((-1.0, 0.0))
    .quadratic_to((0.0, 1.5), (1.0, 0.0))
    .line_to((0.0, -1.0))
    .close()
)
scene = Scene()
scene.path(
    curve,
    fill=Color(0.62, 0.38, 0.96),
    stroke=Color(1.0, 1.0, 1.0),
    stroke_width=0.06,
)
```

Static path meshes are tessellated once per exact path/stroke-width pair and reused across instances. Transform, fill, stroke color, and opacity changes update only instance data. Stroke-width changes select a separate cached tessellation because stroke width changes mesh geometry.

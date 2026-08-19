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

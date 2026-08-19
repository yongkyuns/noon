# Noon WebGPU demo

This demo proves that a serialized Noon scene can compile, evaluate, and render in a browser without Python, while an optional Pyodide worker can author ordered live patches without blocking playback.

From the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

Open **Python patch source** to edit the authoring code, then click **Run Python patch**. The first run lazily downloads the pinned Pyodide runtime, executes Python in a module worker, and sends a versioned `PatchBatch` back to the persistent Rust runtime. Playback continues while Python loads or runs, and deployed scenes still work without Pyodide when the authoring control is unused.

The worker loads Pyodide `314.0.5` from the official jsDelivr distribution, so the Python control requires network access. The render/runtime wasm package remains local under `web/pkg/`.

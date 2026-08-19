# Noon WebGPU demo

This demo proves that a serialized Noon scene can compile, evaluate, and render in a browser without Python.

From the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

Click **Apply live patch** to send an ordered, versioned `PatchBatch` JSON message from JavaScript. The demo alternates palettes, displays the sequence acknowledged by Rust, and shows the playhead value preserved by the transactional update.

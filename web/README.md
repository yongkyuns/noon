# Noon WebGPU demo

This demo proves that a serialized Noon scene can compile, evaluate, and render in a browser without Python.

From the repository root:

```bash
wasm-pack build crates/noon-web --target web --out-dir ../../web/pkg
python3 -m http.server --directory web 8080
```

Then open <http://localhost:8080> in a WebGPU-capable browser. The JavaScript `requestAnimationFrame` timestamp is converted to deterministic scene time in Rust; JavaScript only owns browser scheduling and canvas sizing.

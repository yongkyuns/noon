# Noon

Noon is a high-performance 2D animation system with **Manim-inspired authoring ergonomics** and a deterministic Rust/WebGPU execution core.

The project is being rebuilt around a language-neutral semantic model so Python, Rust, and future frontends can share one set of animation semantics without putting Python on the frame-critical path.

## Architecture

```text
Python / Rust / future frontends
              |
              v
      noon-core semantics
              |
              v
 SceneDefinition / ScenePatch
              |
              v
         noon-compile
              |
              v
        noon-runtime
              |
              v
      noon-render-wgpu
```

Key invariants:

- `noon-core` is renderer- and language-independent.
- `SceneDefinition` is the canonical semantic scene representation.
- `SceneDocument` is a versioned wire format, not a second scene model.
- playback is deterministic and supports arbitrary seek/rewind.
- Python can author or patch scenes, but compiled playback does not require Python.
- static geometry is cached; transform/style animation does not retessellate it.
- WebGPU rendering is optimized around analytic primitives, instancing, cached vector geometry, and compact dynamic state.

See [`docs/architecture-plan.md`](docs/architecture-plan.md), [`docs/implementation-plan.md`](docs/implementation-plan.md), and [`docs/manim-aligned-authoring-plan.md`](docs/manim-aligned-authoring-plan.md).

## Authoring direction

Noon intentionally reuses familiar Manim vocabulary where the semantics fit:

```python
from noon import *

scene = Scene()

circle = Circle(radius=0.6, color=BLUE).shift(LEFT * 2)
square = Square(side_length=1.2, color=PINK)
square.next_to(circle, RIGHT)

scene.add(circle, square)
scene.play(
    circle.animate.shift(RIGHT * 2),
    square.animate.rotate(45 * DEGREES),
    run_time=2,
)
scene.play(FadeOut(circle))
scene.wait(0.5)
```

This is the target ergonomic surface. The migration is being implemented incrementally while preserving the existing deterministic `SceneDefinition`/timeline architecture.

The important distinction from Manim is implementation: Noon does not intend to execute arbitrary Python callbacks every frame. High-level authoring lowers to deterministic semantic tracks evaluated by Rust/WASM.

## Browser playground

The browser demo combines:

- Rust/WASM scene compilation and evaluation;
- WebGPU rendering;
- a Pyodide worker for interactive Python authoring;
- semantic live scene reconciliation;
- runtime and GPU profiling counters.

Build it from the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open `http://localhost:8080` in a WebGPU-capable browser.

Every scene exposed by the playground picker is executed by Python and compiled through the native Rust `ScenePlayer` in CI before deployment.

## Workspace

The active implementation lives entirely under `crates/`:

- `noon-core` — renderer-independent semantic objects, styles, transforms, timeline and patches
- `noon-ir` — versioned scene/patch serialization
- `noon-compile` — semantic scene compilation
- `noon-runtime` — deterministic frame evaluation
- `noon-geometry` — vector tessellation, reveal and morph planning
- `noon-render-wgpu` — WebGPU renderer
- `noon-web` — WASM/browser runtime

## Development

The required CI gate runs:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
bash scripts/build-web-demo.sh
```

It also checks geometry correctness, browser-target compilation, Python playground execution, and native compilation of all picker scenes.

## Design priorities

In order:

1. ergonomic authoring;
2. deterministic correctness and direct-seek semantics;
3. high realtime performance;
4. language-neutral core behavior;
5. live/interpreted authoring without moving frame-critical work into Python;
6. compatibility with familiar Manim concepts where it improves usability.

Strict Manim API compatibility is not a goal. Matching its vocabulary and mental model is preferred when doing so does not weaken Noon's architecture.

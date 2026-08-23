# Noon

Noon is a high-performance 2D animation system targeting **Manim-compatible Python authoring** on top of a deterministic, language-neutral Rust/WebGPU execution core.

The project treats Manim's common 2D authoring semantics as a cross-language contract: Python should be source-compatible where Noon can reproduce the behavior without a fundamental design or performance regression, while Rust and future frontends expose the same concepts and observable semantics idiomatically. Python adapters should normalize Python syntax and types, not implement a second animation engine.

## Architecture

```text
Manim-compatible Python     idiomatic Rust     future frontends
          \                     |                    /
           \                    |                   /
            +------ shared authoring semantics ----+
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
- shared authoring semantics own lifecycle, animation defaults/options, composition, layout, style behavior, known rate functions, and lowering decisions used by every frontend.
- Python compatibility code is a thin adaptation layer for Python class behavior, argument normalization, vector/array conversion, and callable identification.
- `SceneDefinition` is the canonical semantic scene representation.
- `SceneDocument` is a versioned wire format, not a second scene model.
- playback is deterministic and supports arbitrary seek/rewind.
- Python can author or patch scenes, but compiled playback does not require Python.
- same-kind analytic transforms stay analytic; supported cross-kind transforms use compiler-only prepared geometry without changing semantic endpoints.
- static/prepared geometry is cached; steady transform/style animation does not retessellate it.
- WebGPU rendering is optimized around analytic primitives, instancing, cached vector geometry, and compact dynamic state.

The authoritative compatibility roadmap is [`docs/manim-aligned-authoring-plan.md`](docs/manim-aligned-authoring-plan.md). The lower-level architecture is described in [`docs/architecture-plan.md`](docs/architecture-plan.md); [`docs/implementation-plan.md`](docs/implementation-plan.md) records the original core/runtime milestone sequence, and focused status documents record later completed slices.

## Authoring

For supported 2D features, ordinary ManimCE source should require only the import change from `manim` to `noon`:

```python
from noon import *

class Demo(Scene):
    def construct(self):
        circle = Circle(radius=0.6, color=BLUE).shift(LEFT * 2)
        square = Square(side_length=1.2, color=PINK)
        square.next_to(circle, RIGHT)

        self.play(Create(circle), Create(square))
        self.play(
            circle.animate.shift(RIGHT * 2),
            square.animate.rotate(45 * DEGREES),
            run_time=2,
            rate_func=smooth,
        )
        self.play(Transform(circle, Square(1.4, color=PURPLE)), run_time=1.5)
        self.wait(0.5)
```

Equivalent Rust authoring is exposed by the user-facing facade with the same object, animation, timing, and layout model expressed idiomatically:

```rust
use noon::prelude::*;

let mut scene = Scene::new();
let circle = scene.add(Circle::new(0.6).color(BLUE).shift(LEFT * 2.0));
let square = scene.add(Square::new(1.2).color(PINK));
scene.edit(square)?.next_to(circle, RIGHT, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER)?;

scene
    .play((circle.animate().shift(RIGHT * 2.0), square.animate().rotate(45.0 * DEGREES)))
    .run_time(2.0)?;
scene
    .play(Transform::new(circle, Square::new(1.4).color(PURPLE)))
    .run_time(1.5)?;
```

The important distinction from Manim is implementation, not ordinary authoring semantics. Noon does not make arbitrary Python callbacks its normal frame execution model. High-level authoring lowers to deterministic semantic tracks evaluated by Rust/WASM. `Circle -> Circle`, `Rectangle -> Rectangle`, and `Line -> Line` transforms keep analytic fast paths. `Circle <-> Rectangle/Square` is canonicalized by the compiler to temporary fixed path geometry only while the cross-kind transform is active; the serialized scene and semantic endpoints stay analytic.

When exact Manim behavior would require a material architectural or performance regression, Noon should first look for a deterministic compiled equivalent. If none exists, the incompatibility must be explicit and documented rather than silently approximated.

## Browser playground

The browser demo combines:

- Rust/WASM scene compilation and evaluation;
- WebGPU rendering with automatic WebGL2 fallback;
- a Pyodide worker for interactive Python authoring;
- semantic live scene reconciliation;
- runtime and GPU profiling counters.

Build it from the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open `http://localhost:8080`.

Every scene exposed by the playground picker is executed by Python and compiled through the native Rust `ScenePlayer` in CI before deployment.

## Workspace

The active implementation lives entirely under `crates/`:

- `noon` — user-facing Rust authoring facade and prelude
- `noon-core` — renderer-independent semantic objects, styles, transforms, timeline and patches
- `noon-ir` — versioned scene/patch serialization
- `noon-compile` — semantic scene compilation and transform strategy selection
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

It also checks geometry correctness, browser-target compilation, Python playground execution, Manim compatibility smoke scenes, and native compilation of picker scenes.

## Design priorities

In order:

1. Manim-compatible Python ergonomics and semantics for the supported 2D surface;
2. one shared language-neutral authoring model so Rust and other frontends have consistent capabilities;
3. deterministic correctness and direct-seek semantics;
4. high realtime performance;
5. live/interpreted authoring without moving frame-critical work into Python;
6. explicit, measured deviations only where exact Manim behavior has a fundamental design or performance blocker.

Compatibility is an API/semantic goal, not an implementation constraint: Noon does not copy Manim's renderer, internal point-cloud representation, or Python-per-frame execution model.
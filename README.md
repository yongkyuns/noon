# Noon

Noon is a high-performance 2D animation system targeting **Manim-compatible Python authoring** on top of a deterministic, language-neutral Rust/WebGPU execution core.

The project treats Manim's common 2D authoring semantics as a cross-language contract: Python should be source-compatible where Noon can reproduce the behavior without a fundamental design or performance regression, while Rust and future frontends expose the same concepts and observable semantics idiomatically. Python adapters normalize syntax and types; they do not implement a second animation engine.

## Architecture

Noon exposes one expressive, mutable semantic scene and specializes it as aggressively as the program permits:

```text
Manim-compatible Python     idiomatic Rust     future frontends
          \                     |                    /
           \                    |                   /
            +-------- shared semantic scene -------+
                              |
                       analysis / lowering
                              |
               +--------------+--------------+
               |              |              |
               v              v              v
           static plan   reactive graph   host slots
               |              |              |
               +--------------+--------------+
                              |
                              v
                       mutable runtime
                              |
                     incremental dirty work
                              |
                              v
                         renderer
```

Key invariants:

- high-level object, lifecycle, layout, animation, signal, updater, and interaction semantics are implemented once and shared by every frontend;
- Python wrappers hold handles into shared semantic state rather than duplicating scene state, timing, layout, or scheduling logic;
- `noon-core` is renderer- and language-independent execution data;
- immutable and predetermined parts of a scene can be fully compiled and require no host interpreter during playback;
- native reactive dependencies remain live but reevaluate only affected state;
- arbitrary Python callbacks remain supported through explicit host callback slots and batched mutation transactions;
- a small interactive region does not make unrelated static content dynamic;
- playback is deterministic and supports arbitrary seek/rewind wherever the program semantics permit it;
- same-kind analytic transforms stay analytic; supported cross-kind transforms use compiler-prepared geometry without changing semantic endpoints;
- static/prepared geometry is cached; transform/style animation does not retessellate it;
- WebGPU rendering is optimized around analytic primitives, instancing, cached vector geometry, and compact dynamic state.

The authoritative architecture is [`docs/architecture-plan.md`](docs/architecture-plan.md). The Manim compatibility roadmap is [`docs/manim-aligned-authoring-plan.md`](docs/manim-aligned-authoring-plan.md). Existing historical implementation/status documents are not architectural constraints.

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

Equivalent Rust authoring uses the same object, animation, timing, layout, signal, and interaction model expressed idiomatically:

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

The API is intentionally mutable and interactive. The implementation is not forced to remain dynamic: predetermined animation lowers to compiled tracks, common reactive behavior lowers to a native dependency graph, and only semantics that genuinely require arbitrary host-language execution retain host callback slots.

For example, an arbitrary Python updater should remain possible, but the engine should cross the host boundary once per callback phase/transaction rather than once for every getter/setter. If a scene contains no host-dynamic behavior, Python can disappear entirely after construction.

When exact Manim behavior would require a material architectural or performance regression, Noon should first look for a deterministic or native-reactive equivalent. If none exists, the incompatibility must be explicit and documented rather than silently approximated.

## Browser playground

The browser demo combines:

- Rust/WASM scene compilation and evaluation;
- WebGPU rendering with automatic WebGL2 fallback;
- a Pyodide worker for interactive Python authoring;
- semantic live scene reconciliation;
- runtime and GPU profiling counters.

The target architecture keeps the shared semantic implementation beside Pyodide in the worker. Python wrappers call it synchronously through handles, while the render/runtime context receives compact scene or mutation transactions. Static playback requires no Pyodide participation.

Build the current demo from the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open `http://localhost:8080`.

Every scene exposed by the playground picker is executed by Python and compiled through the native Rust `ScenePlayer` in CI before deployment.

## Workspace

The active implementation lives under `crates/`:

- `noon` — user-facing Rust API and the intended home of shared semantic authoring behavior unless a real dependency boundary later justifies extraction
- `noon-core` — renderer-independent normalized scene, timeline, property, identity, and mutation data
- `noon-ir` — current versioned scene/patch serialization; the name/responsibility may be simplified as transport becomes a codec concern
- `noon-compile` — semantic specialization, lowering, geometry preparation, and execution-plan construction
- `noon-runtime` — deterministic mutable execution, reactive evaluation, and incremental updates
- `noon-geometry` — vector tessellation, reveal and morph planning
- `noon-render-wgpu` — WebGPU renderer
- `noon-web` — WASM/browser runtime integration

Crates should correspond to real dependency or compilation boundaries. Noon does not add `noon-authoring` or `noon-wire` merely to mirror conceptual layers.

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

The architecture reset adds additional required validation around mutation atomicity, reactive dirty propagation, cross-language semantic parity, host callback batching, and mixed static/dynamic performance.

## Design priorities

In order:

1. Manim-compatible Python ergonomics and semantics for the supported 2D surface;
2. one shared semantic scene so Rust and other frontends have consistent capabilities;
3. unrestricted interactivity and mutability without imposing dynamic overhead on static content;
4. deterministic correctness and direct-seek semantics where semantically possible;
5. automatic specialization for high realtime and offline-render performance;
6. explicit, measured deviations only where exact Manim behavior has a fundamental design or performance blocker.

Compatibility is an API/semantic goal, not an implementation constraint: Noon does not copy Manim's renderer, internal point-cloud representation, Python-side scene engine, or Python-per-frame execution model.
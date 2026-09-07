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
                         execution plan
                              |
                       mutable runtime
                              |
                     incremental dirty work
                              |
                              v
                         renderer
```

Key invariants:

- one Semantic Scene is the only authored scene authority;
- high-level object, lifecycle, layout, animation, signal, updater, and interaction semantics are implemented once and shared by every frontend;
- Python wrappers hold handles into shared semantic state rather than duplicating scene state, timing, layout, or scheduling logic;
- lowering specializes immutable, timeline, native-reactive, and host-dynamic dependencies independently;
- static/prepared geometry and resources remain retained and are not rebuilt for unrelated property changes;
- local semantic/runtime changes remain local through execution and rendering;
- arbitrary Python callbacks are explicit host-dynamic slots and do not put Python on the normal frame path;
- playback is deterministic and supports arbitrary seek/rewind wherever the program semantics permit it;
- serialization is a codec, not another scene architecture.

The single authoritative architecture and roadmap is [`docs/architecture.md`](docs/architecture.md). If code or an older document conflicts with it, `docs/architecture.md` wins. Noon is greenfield: migration compatibility is not a reason to preserve obsolete internal architecture.

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

The public Rust `Scene` and `Mobject` API authors directly into the same semantic store used by the WASM handles:

```rust
use noon::Scene;

let mut scene = Scene::new();
let mut circle = scene.circle(0.6)?;
circle.shift(-2.0, 0.0)?;
circle.set_fill(0.0, 0.0, 1.0, 0.5)?;
let mut square = scene.square(1.2)?;
square.next_to_handle(&circle, 1.0, 0.0, 0.25)?;
scene.add(&circle)?;
scene.add(&square)?;
let session = scene.execution_session()?;
assert_eq!(session.frame().objects.len(), 2);
```

Constructors are scene-bound factories, `Scene::add` attaches the existing node, and handle queries return errors for stale identities. Copies allocate independent nodes in the same store. See [`shared_authoring.rs`](crates/noon/examples/shared_authoring.rs) for typed lowering and runtime execution.

The older fluent snapshot authoring API is available explicitly through `noon::legacy`, including `noon::legacy::prelude`; it is migration code owned for deletion by #959. Its advanced animation examples have not yet all moved to the canonical public API. Initial membership changes prepare subsequent sessions; they do not implicitly mutate an already running session.

After creating a session, use `scene.live(&mut session)` for shared property edits, append-compatible membership changes, predeclared affine animations, and replacement with content already owned by the semantic store. Property and structural edits use `ExecutionSession::apply_semantic_transaction` to prepare semantic changes and typed execution publication together, so a failed edit leaves authored and live states unchanged.

Rust uses `live.effective(&object)` or `live.effective_layout(&object)` for coherent runtime values; ordinary Rust `Mobject` inspection explicitly reads authored/base state. Python `get_center`, `width`, and `height` route through the same effective layout while its canonical context owns a live session, fall back to authored layout before bootstrap, and reject reads while that session is transferred.

`live.complete_segment(segment)` in Rust and `live.complete()` in Python reconcile a supported affine endpoint into authored state before releasing its timeline driver, so later authored setters survive subsequent frames. Flat Parallel/Sequence compositions use the same prepared semantic transaction, shared schedule, runtime, and mapped completion barrier. Direct handle mutations after initial lowering make that session's scene revision stale. Live resource allocation, interleaved membership ordering, reactive-topology changes, instantaneous completion, overlapping-driver release, sequential duplicate-property drivers, and historical replay of unrecorded authored mutations remain unsupported. Original deterministic track intervals remain available for seek.

Python geometry and text scenes, including the explicit `live_execution()` facade, execute from their shared semantic handles. The browser sends execution deltas across the actual worker boundary. The native and direct Rust/WASM paths remain typed in-process. Some Manim timeline and callback features still use migration code while their shared continuation contracts are completed. Document-oriented tools can explicitly request `exportDocument: true` from `PythonAuthoringClient.run`.

Equivalent examples run through the native Rust renderer and the Python browser host:

| Feature | Rust | Python |
| --- | --- | --- |
| Geometry and text | [shared_text.rs](crates/noon-native/examples/shared_text.rs) | [shared_text.py](web/python/examples/shared_text.py) |
| Live membership | [live_semantic_scene.rs](crates/noon-native/examples/live_semantic_scene.rs) | [live_semantic_scene.py](web/python/examples/live_semantic_scene.py) |
| Affine animation | [live_affine_animation.rs](crates/noon-native/examples/live_affine_animation.rs) | [live_affine_animation.py](web/python/examples/live_affine_animation.py) |
| Affine completion | [live_affine_completion.rs](crates/noon-native/examples/live_affine_completion.rs) | [live_affine_completion.py](web/python/examples/live_affine_completion.py) |
| Sequential ordinary affine play | [ordinary_affine_play.rs](crates/noon-native/examples/ordinary_affine_play.rs) | [ordinary_affine_play.py](web/python/examples/ordinary_affine_play.py) |
| Ordinary FadeIn/FadeOut lifecycle | [ordinary_fade_play.rs](crates/noon-native/examples/ordinary_fade_play.rs) | [ordinary_fade_synchronous_continuation.py](web/python/examples/ordinary_fade_synchronous_continuation.py) |
| Ordinary affine callback continuation | [ordinary_affine_callback_continuation.rs](crates/noon-native/examples/ordinary_affine_callback_continuation.rs) | [ordinary_affine_callback_continuation.py](web/python/examples/ordinary_affine_callback_continuation.py) |
| Scoped scalar callback reads | [ordinary_callback_sparse_reads.rs](crates/noon-native/examples/ordinary_callback_sparse_reads.rs) | [ordinary_callback_sparse_reads.py](web/python/examples/ordinary_callback_sparse_reads.py) |
| Flat ordinary composition | [ordinary_composition_play.rs](crates/noon-native/examples/ordinary_composition_play.rs) | [ordinary_composition_play.py](web/python/examples/ordinary_composition_play.py) |
| Flat ordinary composition continuation | [ordinary_composition_continuation.rs](crates/noon-native/examples/ordinary_composition_continuation.rs) | [ordinary_composition_continuation.py](web/python/examples/ordinary_composition_continuation.py) |
| Point-correspondence and angular rotation | [ordinary_different_rotations.rs](crates/noon-native/examples/ordinary_different_rotations.rs) | [manim_parity_different_rotations.py](web/python/examples/manim_parity_different_rotations.py) |
| Construct Circle/Square after a wait | [ordinary_live_primitive_construction.rs](crates/noon-native/examples/ordinary_live_primitive_construction.rs) | [ordinary_live_primitive_construction.py](web/python/examples/ordinary_live_primitive_construction.py) |
| Affine Grow/Spin/Shrink lifecycle | [ordinary_affine_lifecycle.rs](crates/noon-native/examples/ordinary_affine_lifecycle.rs) | [manim_parity_affine_lifecycle.py](web/python/examples/manim_parity_affine_lifecycle.py) |
| Nested Add/Wait, staggered Fade and re-entry | [ordinary_timed_composition.rs](crates/noon-native/examples/ordinary_timed_composition.rs) | [ordinary_timed_composition.py](web/python/examples/ordinary_timed_composition.py) |
| Mixed scalar and object composition | [ordinary_mixed_scalar_composition.rs](crates/noon-native/examples/ordinary_mixed_scalar_composition.rs) | [ordinary_mixed_scalar_composition.py](web/python/examples/ordinary_mixed_scalar_composition.py) |
| Family transform and restoring Indicate | [ordinary_family_transform_indicate.rs](crates/noon-native/examples/ordinary_family_transform_indicate.rs) | [ordinary_family_transform_indicate.py](web/python/examples/ordinary_family_transform_indicate.py) |
| Scalar ValueTracker continuation | [ordinary_value_tracker_continuation.rs](crates/noon-native/examples/ordinary_value_tracker_continuation.rs) | [ordinary_value_tracker_continuation.py](web/python/examples/ordinary_value_tracker_continuation.py) |
| Ordered property callbacks | [live_affine_callbacks.rs](crates/noon-native/examples/live_affine_callbacks.rs) | [live_affine_callbacks.py](web/python/examples/live_affine_callbacks.py) |
| Shared callback paint | [live_callback_paint.rs](crates/noon-native/examples/live_callback_paint.rs) | [live_callback_paint.py](web/python/examples/live_callback_paint.py) |
| Analytic Line endpoint callbacks | [live_line_match_callback.rs](crates/noon-native/examples/live_line_match_callback.rs) | [live_line_match_callback.py](web/python/examples/live_line_match_callback.py) |
| Windowed Line rotation callbacks | [live_line_callback_rotation.rs](crates/noon-native/examples/live_line_callback_rotation.rs) | [renderer_observation_line_callbacks.py](web/python/examples/renderer_observation_line_callbacks.py) |
| Content replacement | [live_content_switch.rs](crates/noon-native/examples/live_content_switch.rs) | [live_content_switch.py](web/python/examples/live_content_switch.py) |

Run a Rust example with `cargo run -p noon-native --example live_content_switch`, or paste its paired Python source into the playground. The shared browser smoke executes the published Python files and checks their rendered output.

The callback examples run forward through compiler-selected barriers. The ordinary continuation pairs one affine transform with ordered transform/style updates and resumes authoring only after its exact endpoint publication. The broader callback example also includes a separate `dt` accumulator. Callback `set_color` and `set_fill` use the same shared Rust paint rules as ordinary authoring; callback `set_opacity` remains the independent object-composite property. Callbacks read phase-consistent object and scalar values and stage property writes for active callback targets. Analytic Line endpoint matching stages a transform and preserves source paint; its temporary endpoint operand cannot escape the callback phase. Family callbacks, structural callback edits, and seeking or looping opaque callbacks are not supported. A callback failure stops progression at the last coherent frame.

The API is intentionally mutable and interactive. The implementation is not forced to remain dynamic: predetermined animation lowers to compiled tracks, common reactive behavior lowers to a native dependency graph, and only semantics that genuinely require arbitrary host-language execution retain host callback slots.

When exact Manim behavior would require a material architectural or performance regression, Noon should first look for a deterministic or native-reactive equivalent. If none exists, the incompatibility must be explicit rather than silently approximated.

## Browser playground

The browser demo combines:

- Rust/WASM scene compilation and evaluation;
- WebGPU rendering with automatic WebGL2 fallback;
- a Pyodide worker for interactive Python authoring;
- semantic live scene reconciliation;
- runtime and GPU profiling counters.

The target architecture keeps the shared semantic implementation beside Pyodide in the authoring context. Python wrappers call it synchronously through handles, while the execution/render context receives typed scene or mutation data. Static playback requires no Pyodide participation.

Build the current demo from the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open `http://localhost:8080`.

Every scene exposed by the playground picker is executed by Python and compiled through the native Rust `ScenePlayer` in CI before deployment.

## Workspace

The active implementation lives under `crates/`. The target ownership is:

- `noon` — public Rust API, authoritative Semantic Scene, and shared authoring semantics;
- `noon-core` — normalized renderer-independent execution-plan data;
- `noon-compile` — semantic analysis, specialization, lowering, and geometry preparation;
- `noon-runtime` — deterministic mutable execution, reactive evaluation, scheduling, and incremental updates;
- `noon-render-wgpu` — retained WebGPU renderer;
- `noon-web` — WASM/browser integration;
- supporting geometry/text crates only where a real dependency or compilation boundary justifies them.

The current `noon-ir` and migration-era scene/transport models are transitional and are scheduled for removal by the architecture-consolidation phase. Serialization/transport is a codec concern, not a permanent scene layer.

Crates should correspond to real dependency or compilation boundaries. Prefer modules over crates until an independent build/dependency/reuse boundary exists.

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

The architecture roadmap requires additional validation around single-authority semantics, mutation atomicity, reactive dirty propagation, cross-language semantic parity, host callback batching, and mixed static/dynamic performance.

## Design priorities

In order:

1. one authoritative Semantic Scene and one lowering boundary;
2. Manim-compatible Python ergonomics and semantics for the supported 2D surface;
3. shared semantics across Python, Rust, and future frontends;
4. unrestricted interactivity and mutability without imposing dynamic overhead on static content;
5. deterministic correctness and direct-seek semantics where semantically possible;
6. automatic specialization and strict locality for high realtime and offline-render performance;
7. explicit, measured deviations only where exact Manim behavior has a fundamental design or performance blocker.

Compatibility is an API/semantic goal, not an implementation constraint: Noon does not copy Manim's renderer, internal point-cloud representation, Python-side scene engine, or Python-per-frame execution model.

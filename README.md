# Noon

Noon is a **Rust-native 2D animation and interactive graphics engine** with Manim-compatible Python authoring, built around a shared semantic scene, a deterministic runtime, and a retained GPU renderer.

Rust and Python expose the same observable scene semantics through shared Rust operations. Python supplies Manim-compatible syntax and arbitrary host callbacks where they are genuinely required; it does not implement a second scene, scheduler, runtime, or renderer.

## Architecture

The main data path is intentionally small. Focused diagrams carry the detailed publication, locality, callback, deployment, and ownership contracts instead of crowding them into one picture.

![Horizontal architecture: authoring feeds the semantic scene, lowering produces CompiledScene, runtime publishes frame changes to the renderer, and host services supply input/ticks and device/presentation.](docs/diagrams/overview.svg)

[D2 source](docs/diagrams/overview.d2) · [Domain projections](docs/architecture.md#domain-projections) · [Revision and lifetime model](docs/architecture.md#identity-generations-revisions-versions-and-sequences) · [Locality propagation](docs/architecture.md#runtime-complexity-contract) · [Callback transaction](docs/architecture.md#ordered-transactional-host-callback-overlay) · [Current ownership](docs/architecture.md#current-implementation-ownership) · [Python worker topology](docs/architecture.md#host-language-or-multi-worker-topology).

The overview separates responsibilities rather than advertising another framework:

- **Authoring** supplies Rust/Python ergonomics and invokes shared semantic operations.
- **Semantic scene** (`SemanticStore`) owns authored identity, structure, declarations, and persistent state.
- **Lowering** (`noon-compile`) derives replaceable execution data such as slots, tracks, reactive dependencies, and resource projections.
- **Runtime/session** owns effective time-varying state, ordering, completion, wake/sleep, and coherent publication.
- **Renderer** owns retained GPU resources and dirty draw work only; native/browser hosts own platform lifecycle.

Normal native and single-context Rust/WASM engine boundaries are typed in-process Rust boundaries. Serialization is reserved for explicit codecs or genuine cross-context transport. The [architecture guide](docs/architecture.md) is the single normative source for the contracts and roadmap.

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

### Live authoring and integration boundaries

The crate root and `noon::prelude` expose ordinary authoring handles, values, live operations, completion, and typed errors. `noon::integration` is the explicit raw semantic/resource and host plumbing boundary; `noon::diagnostics` is opt-in debug/export access. Neither namespace creates another scene or runtime.

After creating an execution session, `scene.live(&mut session)` applies supported persistent edits and animation/completion operations through the same staged semantic/execution publication path. Rust `Mobject` inspection reads authored/base state; live/effective queries read the latest coherent runtime state. Failed preparation leaves both authored and effective published state unchanged.

Raw integration-store access is deliberately not a live-mutation shortcut: edits outside coherent publication can stale a session and must be handled explicitly. See [`shared_authoring.rs`](crates/noon/examples/shared_authoring.rs) for the public typed path and `docs/architecture.md` for the authored/effective and publication contracts.

Equivalent examples run through the native Rust renderer and the Python browser host:

| Feature | Rust | Python |
| --- | --- | --- |
| Geometry and text | [shared_text.rs](crates/noon-native/examples/shared_text.rs) | [shared_text.py](web/python/examples/shared_text.py) |
| Live membership | [live_semantic_scene.rs](crates/noon-native/examples/live_semantic_scene.rs) | [live_semantic_scene.py](web/python/examples/live_semantic_scene.py) |
| Ordinary affine playback | [ordinary_affine_play.rs](crates/noon-native/examples/ordinary_affine_play.rs) | [ordinary_affine_play.py](web/python/examples/ordinary_affine_play.py) |
| Composition | [ordinary_composition_play.rs](crates/noon-native/examples/ordinary_composition_play.rs) | [ordinary_composition_play.py](web/python/examples/ordinary_composition_play.py) |
| Ordered callbacks | [live_affine_callbacks.rs](crates/noon-native/examples/live_affine_callbacks.rs) | [live_affine_callbacks.py](web/python/examples/live_affine_callbacks.py) |
| Content replacement | [live_content_switch.rs](crates/noon-native/examples/live_content_switch.rs) | [live_content_switch.py](web/python/examples/live_content_switch.py) |

The complete qualification corpus lives under [`crates/noon-native/examples`](crates/noon-native/examples) and [`web/python/examples`](web/python/examples).

Run a Rust example withRun a Rust example with `cargo run -p noon-native --example live_content_switch`, or paste its paired Python source into the playground. The shared browser smoke executes the published Python files and checks their rendered output.

The broader example corpus covers callback ordering, sparse reads, family operations, composition, transforms, text, and renderer behavior. Required host callbacks hold authored progress at their ordered barrier; deterministic segments do not require per-frame Python execution when no host-dynamic work is scheduled. Unsupported compatibility behavior remains explicit rather than silently approximated.

Current Phase A acceptance status is intentionally not duplicated here; use the [Phase A umbrella](https://github.com/yongkyuns/noon/issues/953) and the architecture guide for current ownership and invariants.

## Browser playground
## Browser playground

The browser demo combines:

- Rust/WASM scene compilation and evaluation;
- WebGPU rendering with automatic WebGL2 fallback;
- a Pyodide worker for interactive Python authoring;
- shared Rust live mutations and source-session replacement on rerun;
- runtime and GPU profiling counters.

The current semantic Python path keeps the shared Rust/WASM semantic context **and its execution session in the authoring worker** beside Pyodide. Python calls shared operations synchronously through handles; the semantic engine endpoint leases the existing player in that same context. A separate render owner receives derived resource bundles and execution deltas over the actual worker boundary, using transferable buffers or a shared mailbox. Rendering can fall back to the main thread without moving semantic/runtime authority. Deterministic playback without host callbacks does not require per-frame Python interpreter execution. See the [current worker diagram](docs/architecture.md#host-language-or-multi-worker-topology).

Build the current demo from the repository root:

```bash
bash scripts/build-web-demo.sh
python3 -m http.server --directory web 8080
```

Then open `http://localhost:8080`.

Every scene exposed by the playground picker is authored through the shared Rust semantic operations and executed by the common runtime in CI before deployment.

## Workspace

The active implementation lives under `crates/`. Current responsibilities are:

- `noon` — public Rust API, shared authoring operations, and execution-session orchestration;
- `noon-core` — shared semantic identity/store, declarations, resources, and renderer-independent data contracts;
- `noon-compile` — semantic analysis, specialization, lowering, `CompiledScene`, root-order publication preparation, and geometry preparation;
- `noon-runtime` — deterministic mutable execution, reactive evaluation, scheduling, and incremental updates;
- `noon-render-wgpu` — retained WebGPU renderer;
- `noon-native` — native window/event-loop/surface integration, isolated by its platform dependencies;
- `noon-web` — WASM/browser integration;
- supporting geometry/text crates only where a real dependency or compilation boundary justifies them.

`noon-ir`, migration scene/document models, and obsolete browser execution mirrors have been deleted. Module/crate normalization is complete: `SemanticStore` remains in `noon-core`, `CompiledScene` in `noon-compile`, effective execution in `noon-runtime`, and session orchestration in `noon`. Serialization remains only for explicit codecs and genuine cross-context transport.

The [current ownership diagram](docs/architecture.md#current-implementation-ownership) shows these settled responsibility boundaries. Remaining finite Phase A acceptance work is tracked from [#953](https://github.com/yongkyuns/noon/issues/953), rather than by reopening completed migration/module-normalization tracks. Diagram sources and checked-in SVGs are refreshed with `python3 scripts/architecture_diagrams.py`; CI checks that they agree.

Crates should correspond to real dependency or compilation boundaries.Crates should correspond to real dependency or compilation boundaries. Prefer modules over crates until an independent build/dependency/reuse boundary exists.

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

1. one authoritative Semantic Scene and one typed lowering/publication boundary;
2. first-class Rust authoring and execution on native and direct Rust/WASM targets;
3. Manim-compatible Python ergonomics over the same shared semantics;
4. unrestricted interactivity and mutability without imposing dynamic overhead on static content;
5. deterministic correctness, direct-seek semantics, and coherent authored/effective state;
6. automatic specialization and strict locality through validation, execution, rendering, and GPU publication;
7. explicit, measured deviations only where exact Manim behavior has a fundamental design or performance blocker.

Compatibility is an API/semantic goalCompatibility is an API/semantic goal, not an implementation constraint: Noon does not copy Manim's renderer, internal point-cloud representation, Python-side scene engine, or Python-per-frame execution model.

## Optional Rust text providers

The `noon` crate keeps native text, Typst layout and bundled fonts enabled by
convenience defaults. Geometry-only consumers can opt out explicitly:

```toml
[dependencies]
noon = { path = "path/to/noon/crates/noon", default-features = false }
```

`native-text` enables `Text`, `Scene::text`, `Mobject::from_text` and live text
construction. `typst` enables `Typst`/`MathTypst` and their corresponding
constructors. `bundled-fonts` supplies the existing font assets, but does not
activate either compiler by itself. Native shaping with supplied fonts needs
only `features = ["native-text"]`; Typst with supplied fonts needs only
`features = ["typst"]`. Add `bundled-fonts` for family-based convenience lookup.
Typst still requires its upstream base assets (including PDF standard-font data);
disabling bundles excludes its optional typography font families, not those base
compiler resources. Geometry-only and native-text-only builds exclude Typst's
assets package entirely.

Supply native font bytes via `NativeFontFace::new(family, bytes, face_index)` and
`Text::with_font_face(face)`. Supply Typst font buffers via
`Typst::with_fonts` or `MathTypst::with_fonts` (an iterator of `Arc<[u8]>`). The
`noon-typst` backend also exposes `compile_typst_resource_with_fonts` for direct
resource integrations. These inputs normalize into the existing shared resource
store; they do not introduce another registry or execution path. Selecting a
native family with `with_font` clears an earlier explicit face. An empty or
invalid explicit Typst font set is an error even when bundled fonts are enabled.

Without bundles, family lookup fails with `TextAuthoringError::FontUnavailable`
and the Typst convenience constructors report `TypstBackendError::FontsUnavailable`.
There is no provider substitution. Provider-specific APIs are absent when their
feature is disabled; bundled-font reference scenes additionally require the
asset feature. Shared semantic text/resource types, lowering and runtime support
remain available independently of those concrete compilers.

Cargo features are additive: another dependency enabling `noon` defaults can
re-enable providers. The [external consumer and qualification commands](fixtures/provider-consumer/README.md)
inspect the resolved active graph independently of Noon's workspace and record
native/WASM build and footprint evidence.

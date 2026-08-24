# Native reactive execution

## Purpose

Noon must support unrestricted interactive authoring without forcing every scene to remain dynamically interpreted. The semantic model therefore separates predetermined timeline behavior from native reactive dependencies and, later, arbitrary host callback slots.

The key rule is:

> Mutability is a semantic capability; dynamic execution cost is paid only by the dependencies that actually require it.

A large scene with one interactive tracker should keep unrelated objects static and compiled.

## Scene layers

The implementation distinguishes related representations:

- `SemanticScene`: the mutable high-level semantic scene. It owns the normalized deterministic `SceneDefinition` plus a native reactive graph. Future host callback slots and richer authoring state belong at this level.
- `SceneDefinition`: the normalized object/timeline program consumed by the existing compiler/runtime. Reactive dependencies are not encoded as fake or infinite timeline tracks.
- `SceneInstance`: the dense mutable execution state. Instances created with `SceneInstance::from_semantic` own a `ReactiveState` beside the compiled timeline and lower semantic bindings to dense frame targets once.

This distinction is intentional. Timeline animation and reactive state have different execution and invalidation rules and meet during lowering rather than being conflated in the authoring model.

## Native reactive graph

`ReactiveGraphDefinition` contains stable signals and property bindings.

Signals are either:

- inputs, whose values may be changed by interaction, trackers, application state, or host code; or
- derived expressions, which depend on other signals.

The first expression IR supports constants, signal references, addition, subtraction, multiplication, negation, sine, and cosine over bool/scalar/vector value types where the operation is meaningful. It is deliberately language-neutral and serializable so the same graph can later target Rust/WASM, SIMD, or GPU execution.

Bindings connect a signal to an object property. Compilation rejects:

- unknown signal or object identities;
- dependency cycles;
- non-finite values;
- expression type errors;
- signal/property type mismatches;
- multiple reactive drivers for one object property; and
- a timeline track and reactive binding driving the same property.

Different properties on the same object may use different execution strategies. For example, a position can remain a predetermined timeline track while rotation is driven by a live signal.

## Shared authoring surface

The Rust authoring facade now exposes native reactivity without replacing the established `Scene` API:

- `ReactiveScene` owns an ordinary authoring `Scene` plus one `ReactiveGraphDefinition` and dereferences to `Scene`, so existing shape, layout, style and animation calls remain available;
- `ValueTracker` is a stable scalar signal handle rather than a callback object;
- `VectorSignal` represents vector-valued input or derived signals;
- `position_from_tracker(tracker, direction, offset)` lowers the common `offset + tracker * direction` pattern directly to `ReactiveExpr`;
- convenience bindings cover position, rotation, opacity, appearance, reveal and morph; and
- `semantic_scene()` / `into_semantic_scene()` combine deterministic and reactive authoring into the single core `SemanticScene`.

The standard Rust `noon::prelude::*` exports `ReactiveScene`, `ValueTracker` and `VectorSignal` alongside the existing authoring vocabulary.

Python exposes the same semantic graph through a deliberately thin Manim-style adapter. `ValueTracker` supports `get_value`, `set_value`, `increment_value` and stable signal attachment; `Scene.value_tracker(...)` and property-binding helpers emit the same language-neutral signal and binding declarations used by Rust. Python does not evaluate dependencies, propagate dirtiness, schedule reactive callbacks or mutate renderer state.

`ValueTracker.animate` is intentionally not emulated yet. Manim's common `self.play(tracker.animate.set_value(...))` form needs a first-class timeline-driven signal representation so playback remains native and deterministic. Until that representation exists, the Python API raises an explicit `NotImplementedError` rather than falling back to per-frame Python execution.

## Semantic scene transport

`SemanticSceneDocument` extends the existing version-1 scene document additively with an optional `reactive` field containing public signal definitions and bindings. The private `ReactiveGraphDefinition::next_signal_id` counter is not serialized.

A scene with no reactive declarations serializes to the established object/track shape and old scene JSON remains valid when decoded as a semantic scene. Reactive documents are reconstructed with `ReactiveGraphDefinition::from_parts` and validated through `ReactiveProgram::compile` before execution.

CI validates the transport in both directions relevant to the frontend boundary:

- a Playwright/Pyodide test authors `ValueTracker` bindings and checks the exact emitted language-neutral graph; and
- a Rust integration test decodes that Python wire shape, builds `SceneInstance::from_semantic`, verifies initial bound values, changes an input through `set_reactive_input`, and checks localized `FrameChanges` and reactive evaluation statistics.

The existing browser canvas player still enters through the deterministic scene decoder. Wiring `SemanticSceneDocument` into that host and exposing live signal-input methods is the next browser integration slice; this document does not claim live browser interaction before that host boundary is connected.

## Incremental evaluation

`ReactiveProgram` validates and topologically orders the graph once. `ReactiveState` stores values for one execution.

Changing one input does not scan the whole graph. It schedules only direct dependents. A derived signal schedules its own dependents only if its evaluated value actually changed. Therefore an unchanged intermediate value terminates dirty propagation.

Each input update returns a `ReactiveUpdate` containing changed signals and invalidated object/property bindings. `SceneInstance` consumes that update directly:

1. semantic bindings are mapped once to dense `FrameState` object indices when the instance is built;
2. `set_reactive_input` evaluates only the affected dependency branch;
3. each invalidated binding writes directly to its precomputed dense property target;
4. changed object indices are inserted into `FrameChanges` for renderer-side incremental preparation; and
5. no `SceneDefinition` reconstruction, timeline recompilation, or whole-scene diff occurs on the native input path.

`ReactiveRuntimeStats` reports dependency work and dense-target writes separately from ordinary timeline `EvaluationStats`. CI includes a mixed scene with 50,000 unrelated static objects and verifies that one reactive input still evaluates only its two derived signals, applies one dense target, and invalidates one object.

## Timeline and seek interaction

Timeline and reactive ownership remains property-local. The compiler rejects a timeline track and reactive binding that drive the same property in the semantic scene, while different properties may coexist on one object.

A forward timeline step mutates only timeline-owned properties, so reactive values remain in place. A seek is different: `SceneInstance` reconstructs its base frame and evaluates the timeline from deterministic state, then reapplies current reactive values. Seek cost therefore includes the number of attached reactive bindings, not arbitrary host execution or a full reactive graph reevaluation.

Value-only live patches also reapply reactive values for the patched object after timeline-owned properties are reconciled. This preserves a reactive position/rotation/opacity when unrelated base transform or style fields are edited.

Reactive-aware structural/timeline graph mutation is intentionally a later slice: structural changes need the semantic reactive graph to be revalidated and re-lowered atomically rather than silently inventing precedence between drivers.

## Execution analysis

Compilation classifies every semantic object as:

- `Static`: no timeline or reactive dependency;
- `Timeline`: predetermined timeline work only;
- `Reactive`: native reactive work only; or
- `TimelineAndReactive`: different properties require both.

This is intentionally object-local rather than a scene-wide mode switch. Later execution analysis will add host-dynamic participation without weakening the static/native categories of unrelated objects.

## Mutation transactions

Arbitrary host callbacks and editor actions converge on `MutationTransaction`. Property-only transactions use a preflight-and-commit path that avoids cloning the entire scene. Timeline or structural transactions retain staged rollback until they have specialized incremental commit paths.

Native reactive evaluation is not itself expressed as generic scene patches. It produces typed property invalidations consumed directly by the dense runtime representation. This avoids paying transport/patch machinery costs for every native signal change.

## Next implementation slices

1. Wire `SemanticSceneDocument` into the persistent browser/canvas player and expose host methods for changing native input signals without scene replacement.
2. Add timeline-driven signal tracks so `ValueTracker.animate.set_value(...)` remains deterministic and runs in Rust rather than invoking Python per frame.
3. Add input/event signals for pointer, keyboard, viewport and user controls.
4. Add reactive-aware structural/timeline mutation that revalidates and re-lowers affected bindings atomically.
5. Add host callback slots that read a coherent frame snapshot and submit one batched mutation transaction per callback phase.
6. Extend execution analysis with host-dynamic dependencies and report static/native/host participation and costs.
7. Profile mixed scenes where a small reactive region sits beside tens or hundreds of thousands of static objects; add explicit performance budgets in CI once stable runner variance is characterized.

## Non-goals

- encoding native reactivity as fake or infinite object timeline tracks;
- running arbitrary Python on every frame by default;
- requiring a separate `noon-authoring` crate;
- forcing a whole scene into an "interactive mode" when only a small region is mutable; or
- silently defining precedence when two execution mechanisms drive the same property.

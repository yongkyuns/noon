# Native reactive execution

## Purpose

Noon must support unrestricted interactive authoring without forcing every scene to remain dynamically interpreted. The semantic model therefore separates predetermined timeline behavior from native reactive dependencies and, later, arbitrary host callback slots.

The key rule is:

> Mutability is a semantic capability; dynamic execution cost is paid only by the dependencies that actually require it.

A large scene with one interactive tracker should keep unrelated objects static and compiled.

## Scene layers

The initial implementation distinguishes two related representations:

- `SemanticScene`: the mutable high-level semantic scene. It owns the normalized deterministic `SceneDefinition` plus a native reactive graph. Future host callback slots and richer authoring state belong at this level.
- `SceneDefinition`: the normalized object/timeline program consumed by the existing compiler/runtime. Reactive dependencies are not encoded as fake or infinite timeline tracks.

This distinction is intentional. Timeline animation and reactive state have different execution and invalidation rules and should meet during lowering rather than being conflated in the authoring model.

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

## Incremental evaluation

`ReactiveProgram` validates and topologically orders the graph once. `ReactiveState` stores values for one execution.

Changing one input does not scan the whole graph. It schedules only direct dependents. A derived signal schedules its own dependents only if its evaluated value actually changed. Therefore an unchanged intermediate value terminates dirty propagation.

Each input update returns a `ReactiveUpdate` containing:

- changed signals;
- invalidated object/property bindings;
- affected object identities; and
- evaluation statistics.

This result is the boundary that the mutable runtime will consume. The runtime should map object/property changes directly to compact runtime slots and renderer dirty ranges rather than reconstructing or diffing a complete scene.

## Execution analysis

Compilation classifies every semantic object as:

- `Static`: no timeline or reactive dependency;
- `Timeline`: predetermined timeline work only;
- `Reactive`: native reactive work only; or
- `TimelineAndReactive`: different properties require both.

This is intentionally object-local rather than a scene-wide mode switch. Later execution analysis will add host-dynamic participation without weakening the static/native categories of unrelated objects.

## Mutation transactions

Arbitrary host callbacks and editor actions converge on `MutationTransaction`. Property-only transactions now use a preflight-and-commit path that avoids cloning the entire scene. Timeline or structural transactions retain staged rollback until they have specialized incremental commit paths.

Native reactive evaluation is not itself expressed as generic scene patches. It produces typed property invalidations so the runtime can update the dense execution representation directly. This avoids paying transport/patch machinery costs for every native signal change.

## Next implementation slices

1. Integrate `ReactiveProgram`/`ReactiveState` with `SceneInstance`, including direct property-slot updates and renderer dirty propagation.
2. Add semantic signal/property APIs to the shared Rust authoring facade and thin Python bindings.
3. Add input/event signals for pointer, keyboard, viewport, and user controls.
4. Add host callback slots that read a coherent frame snapshot and submit one batched mutation transaction per callback phase.
5. Extend execution analysis with host-dynamic dependencies and report static/native/host participation and costs.
6. Profile mixed scenes where a small reactive region sits beside tens or hundreds of thousands of static objects; CI performance tests should ensure reactive cost scales with affected dependencies rather than total scene size.

## Non-goals of this slice

- encoding native reactivity as timeline tracks;
- running arbitrary Python on every frame by default;
- requiring a separate `noon-authoring` crate;
- forcing a whole scene into an "interactive mode" when only a small region is mutable; or
- silently defining precedence when two execution mechanisms drive the same property.

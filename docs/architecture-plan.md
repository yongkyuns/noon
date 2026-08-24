# Noon Architecture

## Status

This is the authoritative architecture for Noon. The implementation is being migrated toward it. Existing internal APIs, wire formats, Python implementation details, and compatibility aliases are not constraints and may be removed rather than preserved.

Noon targets Manim-compatible Python authoring for supported common 2D behavior, while exposing the same semantic capabilities idiomatically from Rust and future frontends. Compatibility is a semantic/API goal, not an implementation constraint.

## 1. Core principle

> Noon exposes one expressive, mutable semantic scene and specializes it as aggressively as the program permits.

Interactivity is not a separate mode or a patch layered on top of a static animation compiler. A scene may contain immutable content, predetermined animation, native reactive dependencies, host callbacks, and live structural mutation at the same time.

The engine determines which portions can be compiled away and which must remain live.

```text
                     language facades
              Python      Rust      future
                  \         |         /
                   \        |        /
                    v       v       v
                semantic scene API
                        |
                 analysis / lowering
                        |
          +-------------+-------------+
          |             |             |
          v             v             v
      static plan   reactive graph  host slots
          |             |             |
          +-------------+-------------+
                        |
                        v
                  mutable runtime
                        |
              incremental dirty work
                        |
                        v
                     renderer
```

The semantic API is dynamically expressive. The execution representation is specialized and data-oriented.

## 2. One implementation of semantics

High-level behavior must be implemented once.

The shared semantic implementation owns:

- object and group identity;
- detached versus scene-owned objects;
- scene membership and lifecycle;
- transforms, styles, geometry and bounds;
- layout operations;
- animation construction and target-state semantics;
- animation option precedence;
- animation composition and scheduling;
- known rate functions;
- signals, trackers, bindings and derived values;
- updater registration;
- event handlers and interaction semantics;
- lowering into executable state.

Python must not maintain a second scene model, scheduler, timing evaluator, layout engine, or lifecycle implementation. Python-specific code may provide class hierarchy behavior, argument normalization, Python collection/vector conversion, callable identity, exceptions, and Scene subclass discovery.

The Rust API is the native expression of the same semantics rather than a separate implementation.

Whether the semantic implementation ultimately occupies its own crate is a dependency-management decision, not an architectural requirement. We should not create a crate boundary unless a consumer genuinely needs one side without the other.

## 3. Semantic scene versus execution scene

The architecture needs a real abstraction boundary, but it is not "Python authoring versus Rust runtime." It is semantic intent versus specialized execution.

### 3.1 Semantic scene

The semantic scene is mutable, hierarchical and ergonomic. It can represent concepts that are useful to users even when they do not exist directly at runtime:

- `Mobject` and groups;
- detached objects;
- `.animate` targets;
- `AnimationGroup`, `Succession`, and `LaggedStart`;
- animation options before inheritance/override resolution;
- signals and bindings;
- updater callbacks;
- user events;
- interactive controls.

The semantic scene owns authoritative current authoring state. Language wrappers hold stable handles into it; they do not duplicate object state.

### 3.2 Specialized execution plan

Analysis/lowering converts the semantic scene into the cheapest representation that preserves behavior. It may contain:

- immutable object/geometry tables;
- resolved timeline tracks;
- precomputed morph/reveal plans;
- native reactive dependency graphs;
- mutable property slots;
- host callback slots;
- event subscriptions;
- packed instance data;
- invalidation metadata.

The execution plan is not required to preserve authoring structure when doing so adds runtime cost.

### 3.3 Runtime instance

A runtime instance owns current execution state:

- playhead/time;
- mutable property values;
- signal values;
- active timeline cursors;
- dirty/invalidation sets;
- bounds/culling state;
- event/input state;
- host callback requests;
- GPU upload state.

Static content should disappear from ordinary per-frame CPU work.

## 4. Execution classes

Every dependency should be classifiable by the optimizer.

### Static

No runtime mutation is possible after lowering.

Examples:

- immutable geometry;
- constant styles;
- predetermined animations whose values depend only on timeline time.

These should be fully compiled, cached, packed, instanced, or GPU-evaluated as appropriate.

### Native reactive

The value changes at runtime, but its dependency graph is understood by Noon.

Examples:

- `ValueTracker` / `Signal`;
- pointer position;
- viewport dimensions;
- a property bound to another property;
- built-in constraints;
- engine-native updaters;
- derived expressions.

Only affected dependencies should be reevaluated.

### Host dynamic

Correct behavior requires arbitrary host-language execution.

Examples:

```python
def update(mob, dt):
    if arbitrary_python_condition():
        mob.rotate(dt * speed)
        mob.set_fill(compute_color())

circle.add_updater(update)
```

Host dynamic behavior is supported intentionally. It is not the default frame path, but it is not treated as an unsupported architectural escape hatch either.

The presence of ten host-dynamic nodes must not cause 100,000 unrelated static nodes to become dynamic.

## 5. Signals and bindings

Noon should provide native reactive primitives that can express common updater behavior without host-language execution.

Conceptually:

```text
Value<T>
+-- Constant(T)
+-- Timeline(track)
+-- Signal(SignalId)
+-- Derived(ExpressionId)
```

This need not be the literal public type layout, but the execution model should distinguish these cases.

Example dependency graph:

```text
pointer.x ---> tracker ---> circle.position.x
                         
                          ---> label/value expression
```

Changing `pointer.x` should invalidate only the dependent chain.

Manim-compatible `ValueTracker`, `always_redraw`, and updater APIs may lower to native reactive constructs when semantics are known. Arbitrary Python remains a host callback.

## 6. Host callback protocol

A callback must not require a language/process crossing for every property access.

The callback protocol is frame/transaction oriented:

```text
runtime frame
    |
    +-- time / dt
    +-- relevant input state
    +-- coherent dynamic snapshot
    |
    v
host callback phase
    |
    +-- callback
    +-- callback
    +-- callback
    |
    v
mutation transaction
    |
    v
runtime commit
    |
    +-- validate atomically
    +-- classify impact
    +-- propagate dirtiness
    +-- perform minimum recompilation
    |
    v
render
```

Host callbacks observe a coherent frame snapshot. Their writes are accumulated and committed atomically as a mutation transaction.

The normal bridge should cross once per callback phase/transaction, not once for every getter and setter. Native bindings may optimize further where calls are in-process.

If no host callback slots exist, the host interpreter is not required during playback.

## 7. Mutation is intrinsic runtime behavior

Live edits and callback writes should use the same mutation machinery. Replacing a complete `SceneDefinition` and diffing it is useful as an editor fallback, but it is not the fundamental interaction mechanism.

The semantic mutation vocabulary should evolve toward operations such as:

```text
MutationTransaction
+-- SetProperty
+-- SetSignal
+-- ReplaceGeometry
+-- AddNode
+-- RemoveNode
+-- ReparentNode
+-- AddAnimation
+-- RemoveAnimation
+-- ChangeSubscription
```

Each mutation has an impact class. Examples:

```text
translation/color change   -> property slot update
signal change              -> reactive dirty propagation
path vertex change         -> retessellate affected geometry
object add/remove          -> local structural allocation/rebuild
large hierarchy change     -> potentially wider relowering
```

A transaction is validated before commit so callbacks cannot leave runtime and semantic state half-updated.

## 8. Animation model

Animations are semantic objects before they become tracks.

Composition should be represented structurally:

```text
AnimationNode
+-- Leaf
+-- Parallel
+-- Sequence
+-- Lagged
```

Animation options remain unresolved at this level. A single resolver applies animation defaults, animation/builder options and `Scene.play` overrides, then scheduling lowers the tree to explicit runtime timing.

`.animate` constructs a target state in the shared semantic implementation. Python must not clone and mutate a parallel Python-side object snapshot to determine the result.

Predetermined animations should compile completely. Animations driven by live signals remain reactive. Host callbacks remain host dynamic.

## 9. Interactivity and input

Input is data entering the same semantic/reactive system rather than browser-specific patches.

Conceptually:

```text
InputState
+-- pointer position/buttons
+-- keyboard state/events
+-- viewport
+-- time
+-- user-defined controls
```

High-frequency interactions such as drag constraints, pointer following, pan/zoom and hit testing should execute natively when possible.

Semantic events can invoke host callbacks when arbitrary user code is required.

Example:

```text
pointer sample -> native hit test -> Click(NodeId)
                                      |
                                      v
                                host handler
                                      |
                                      v
                           mutation transaction
```

## 10. Browser topology

The browser should keep authoring/host execution away from the render main loop while avoiding unnecessary message traffic.

Recommended topology:

```text
Python worker
+-- Pyodide
+-- thin Python facade
+-- shared Noon semantic implementation compiled to WASM
+-- host callback execution
          |
          | transactions / scene payloads
          | transferable binary buffers
          v
main/render context
+-- Noon runtime WASM
+-- input collection
+-- runtime evaluation
+-- WebGPU / WebGL2 renderer
```

The semantic WASM module may live beside Pyodide in the worker so ordinary Python semantic calls are synchronous within one worker. Python object wrappers contain handles into this module.

For frame callbacks, the runtime sends one coherent callback request to the worker and receives one mutation transaction. Static playback requires no Pyodide participation.

The exact transport encoding is an implementation detail. JSON may remain useful for debugging, but the performance path should support compact transferable binary payloads. A separate `noon-wire` crate is unnecessary unless multiple consumers later justify that dependency boundary.

## 11. Optimized execution is automatic

Do not expose a semantic split such as `InteractiveMode` versus `OptimizedMode`.

The same program should specialize automatically:

```text
semantic scene
 |
 +-- constants ----------------------> fold/cache
 +-- predetermined animation --------> timeline tracks
 +-- native reactive dependencies ---> runtime graph
 +-- arbitrary callbacks ------------> host slots
```

An execution report/profiler should eventually expose what prevented specialization, for example:

```text
static nodes:              99,840
native dynamic nodes:         150
host-callback nodes:           10
host callback required:       yes
```

This makes performance explainable rather than surprising.

## 12. Geometry and rendering

Semantic geometry should remain analytic where useful. Compilation chooses the cheapest representation:

- circles/rectangles/lines: analytic/instanced paths where possible;
- static arbitrary paths: cached tessellation;
- morphs: precomputed compatible geometry;
- path reveal: cached arc-length metadata;
- text: shaped glyph runs by default, outlines only when semantics require them.

Transform/style updates must not retessellate static geometry. Reactive changes invalidate only the representations they actually affect.

## 13. Crate boundaries

Crates should correspond to real dependency or compilation boundaries, not conceptual labels.

Current intended responsibilities:

```text
noon
  public Rust API + shared semantic implementation
        |
        v
noon-core
  normalized renderer-independent execution data
        |
        v
noon-compile
  specialization/lowering
        |
        v
noon-runtime
  mutable execution + incremental updates
        |
        v
noon-render-wgpu

noon-web
  browser/WASM integration
```

This is intentionally not a commitment to a separate `noon-authoring` or `noon-wire` crate. Extract those only if the dependency graph later demonstrates real value.

The existing `noon-ir` name should be reconsidered because `SceneDefinition` is already an intermediate representation; serialization/transport is a codec concern, not another semantic scene model.

## 14. Correctness invariants

Core invariants:

1. High-level semantics are implemented once and shared by all language facades.
2. Direct evaluation at time `t` agrees with sequential evaluation when no history-dependent host callback semantics make that impossible.
3. A mutation transaction is atomic.
4. Static regions are not invalidated by unrelated dynamic changes.
5. Reactive evaluation visits only affected dependencies.
6. Host callbacks observe a coherent snapshot and their mutations become visible only at transaction commit.
7. If a program contains no host-dynamic behavior, playback requires no host interpreter.
8. Offline and realtime rendering use the same semantic/runtime behavior.
9. Unsupported Manim behavior is explicit; silent approximation is not acceptable.

## 15. Validation strategy

CI should verify architecture behavior without relying mainly on screenshots:

- cross-language semantic parity tests;
- animation option/scheduling snapshots;
- dependency graph tests;
- dirty-propagation tests proving unrelated objects remain untouched;
- transaction rollback/atomicity tests;
- static-scene tests proving zero host callbacks and minimal mutable state;
- mixed 100k-static + small-dynamic performance tests;
- host-callback batching tests;
- direct-seek versus sequential timeline tests;
- geometry invariants and deterministic tessellation;
- browser interactive smoke tests;
- WebGPU and fallback rendering tests.

Performance regressions should be measured per execution class rather than only as total FPS.

## 16. Migration order

The architecture reset should happen before broadening the API surface.

1. Establish the semantic-scene versus execution-plan boundary.
2. Replace ad-hoc patch assumptions with atomic mutation transactions and impact classification.
3. Add native signal/reactive primitives and dependency tracking.
4. Define host callback slots and the batched snapshot/transaction protocol.
5. Move Rust high-level authoring semantics onto the shared semantic scene.
6. Bind Python objects directly to the same semantic implementation.
7. Delete Python-owned scene state, scheduling, easing and layout implementations.
8. Add automatic specialization analysis and execution diagnostics.
9. Resume broader Manim surface expansion on top of the new architecture.

The migration does not preserve the legacy Noon API or internal serialization merely for compatibility. Git history is the archive.
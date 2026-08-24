# Manim-compatible cross-language authoring plan

This is the authoritative roadmap for Noon's user-facing authoring model.

## Product target

Noon's Python API targets source compatibility with common 2D Manim Community scenes. For a supported feature, normal ManimCE source should ideally require only replacing:

```python
from manim import *
```

with:

```python
from noon import *
```

The compatibility reference is Manim Community v0.21.x. Compatibility is the default goal; Noon should diverge only when matching Manim would impose a demonstrated architectural or material performance cost.

The target is broader than Python compatibility: useful Manim object and animation semantics form a language-neutral authoring contract. Rust and future frontends should expose the same concepts and observable behavior idiomatically.

## Architecture contract

```text
Manim-compatible Python          idiomatic Rust          future frontends
          |                           |                       |
          +------------- thin syntax/type adapters ----------+
                                      |
                                      v
                         shared Rust authoring semantics
                                      |
                   +------------------+------------------+
                   | lifecycle / membership              |
                   | animation options/defaults          |
                   | target-state animation              |
                   | groups/composition/nested time maps |
                   | layout/style semantics              |
                   | deterministic rate functions        |
                   | signals/reactive dependencies       |
                   | unsupported-feature policy          |
                   +------------------+------------------+
                                      |
                                      v
                      semantic scene / timed semantic scene
                                      |
                                      v
                                 noon-compile
                                      |
                                      v
                                 noon-runtime
                                      |
                                      v
                            WebGPU / WebGL2 renderer
```

The semantic scene is authoritative. Frontends may keep transient wrapper state needed for language ergonomics, but they must not implement a second scheduler, lifecycle engine, reactive evaluator, or playback model.

## Shared-semantics rule

Rust owns behavior that must agree across languages:

- scene membership and lifecycle;
- detached-object introduction;
- `.animate` target-state lowering;
- animation option defaults and precedence;
- creation/fading/remover/introducer semantics;
- group/family scheduling and nested composition;
- deterministic rate functions;
- signal/tracker graphs and signal timelines;
- layout/style/bounds semantics as they are consolidated;
- decisions about deterministic lowering versus host-dynamic execution.

Python may own language-specific adaptation only: class hierarchy behavior, argument normalization, Python iterable/vector protocols, wrapper metadata, Scene subclass discovery, callable-to-semantic-ID mapping, and Python-appropriate exceptions.

## Blocker policy

An imperative Manim implementation is not itself a blocker. Use this order:

1. reproduce the observable behavior using existing deterministic semantics;
2. add a deterministic core representation when needed;
3. compile/sample authoring-time behavior when that remains correct and bounded;
4. use a host callback slot when arbitrary host execution is semantically required;
5. only then declare a compatibility gap.

Silent approximation is not acceptable.

## Completed foundation — 2026-08-24

The cross-language authoring consolidation milestone is complete through A5.

### A1 — shared deterministic rate functions

Complete:

- exact shared `linear`, `smooth`, `rush_into`, `rush_from`, and `there_and_back`;
- Rust runtime owns numerical evaluation;
- Python maps known callables to shared semantic IDs;
- Manim-compatible animation defaults use `smooth`.

### A2 — shared animation options

Complete:

- shared `AnimationOptions`, defaults, validation, and precedence;
- `Scene.play` overrides animation-local options consistently;
- Python mobject and `ValueTracker` animation paths use the Rust/WASM resolver.

### A3 — shared lifecycle and membership

Complete:

- shared lifecycle planning for add/remove/reintroduction;
- detached `.animate`, Create, FadeIn/FadeOut, introducer/remover behavior;
- presence-chain validation and source/target requirements.

### A4 — shared composition scheduling

Complete:

- shared parallel/lagged/succession scheduling;
- `AnimationGroup`, `LaggedStart`, and `Succession` in Python and Rust;
- unequal child durations and explicit group runtime rescaling;
- deterministic nested `CompositionTimeMap` data for nonlinear and reversing outer rate functions;
- identity fast path for ordinary tracks.

### A5 — cross-language semantic parity

Current PR establishes this as a permanent CI contract:

- equivalent Rust and Python scenes are authored independently;
- both emit canonical semantic documents;
- CI compares the documents recursively with numeric tolerance only;
- the initial corpus covers target-state animation/options, lifecycle, nonlinear composition/time maps, and timed reactive `ValueTracker` authoring.

New shared features should extend this corpus when equivalent Rust and Python APIs exist.

## Native reactive/interactivity foundation already complete

Noon already has:

- `SemanticScene` with a native typed reactive graph;
- `ValueTracker` and derived signal expressions;
- dependency-local dirty propagation;
- dense runtime binding targets;
- deterministic signal timelines and `tracker.animate.set_value(...)`;
- browser-native reactive scene/canvas players;
- live signal mutation without scene recompilation;
- static/timeline/reactive execution classification.

The remaining architectural interactivity gap is arbitrary host-language behavior.

## Next milestone — host callbacks and general interaction

Implement host-dynamic execution without putting Python on the normal frame path.

Required model:

```text
runtime frame
    |
    +-- coherent time/dt/input/dynamic snapshot
    |
    v
host callback phase
    |
    +-- updater/event callbacks
    |
    v
one mutation transaction
    |
    +-- validate atomically
    +-- apply semantic mutations
    +-- propagate only affected dirtiness
    +-- recompile only when mutation impact requires it
    |
    v
render
```

Priorities:

1. define host callback slots and callback-phase request/response data;
2. add native pointer/keyboard/viewport input signals so common interaction avoids host callbacks entirely;
3. expose Manim-compatible `add_updater`, `remove_updater`, `clear_updaters`, and `always_redraw` with native lowering where possible and host fallback otherwise;
4. batch callback reads/writes so Python↔WASM does not cross once per getter/setter;
5. prove a few host-dynamic nodes do not make unrelated static/reactive nodes dynamic;
6. add browser interaction tests and mixed static/reactive/host execution metrics.

## Following milestone — remaining core 2D semantic parity

Resolve high-impact compatibility differences before expanding breadth:

- stroke-width units and scaling semantics;
- remaining Mobject/VMobject family/bounds/layout methods;
- animation metadata/default details;
- exact option/error behavior for supported APIs.

Stroke width needs an explicit semantic mode. Manim's normal VMobject behavior is display-space-like: scaling an object generally does not scale stroke width. Noon should support that efficiently for compatibility, while a world-space stroke mode may remain useful as a Noon-native option.

## Feature breadth after architectural closure

### Core shapes and animations

- `Uncreate` and `Write` groundwork;
- `Dot`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Triangle`;
- `Arrow`, `Vector`, `DashedLine`.

### Text and mathematical authoring

- `Text`, `MarkupText`, `Tex`, `MathTex`;
- shaped glyph-run/cache/atlas steady-state representation;
- outline extraction only where path-level Write/Create/matching requires it;
- `TransformMatchingTex`.

### Values and plotting

`ValueTracker` and the signal model already exist. Continue with:

- `DecimalNumber`;
- `NumberLine`, `Axes`, `NumberPlane`;
- `FunctionGraph`, `ParametricFunction`;
- braces and labels.

### Advanced VMobject/import surface

- public path-building methods;
- `point_from_proportion`;
- `SVGMobject`;
- `ImageMobject`;
- broader matching transforms.

## CI compatibility contract

For shared features, CI should verify as applicable:

- ordinary Manim-style Python source executes;
- equivalent Rust/Python authoring emits equivalent semantic documents;
- object/track/lifecycle state is correct;
- direct seek agrees with forward playback;
- deterministic rate-function values match reference behavior;
- native reactive updates remain dependency-local;
- WebGPU and WebGL2 browser rendering remain valid;
- unsupported behavior fails with stable, explicit errors.

The cross-language parity corpus is the regression gate against semantic drift between frontends; per-feature smoke tests remain useful for Python-specific ergonomics and browser behavior.

## Relationship to other docs

- `docs/architecture-plan.md` owns the overall static/reactive/host-dynamic execution architecture.
- `docs/reactive-execution.md` owns native reactive/runtime details.
- `docs/manim-animate-semantics.md` and `docs/manim-style-semantics.md` record detailed compatibility behavior.
- older implementation/handoff documents are historical and are not current roadmap sources.

## Non-goals

- copying Manim's renderer or internal point-array implementation;
- putting Python on the normal frame-critical path;
- maintaining separate semantic engines per language;
- forcing analytic geometry into generic path arrays solely for compatibility;
- silently claiming unsupported camera/3D/host-dynamic behavior is compatible.

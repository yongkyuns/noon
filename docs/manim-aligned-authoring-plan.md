# Manim-compatible cross-language authoring plan

This document is the authoritative current roadmap for Noon's user-facing authoring model.

## Product target

Noon's Python API targets **source compatibility with the common 2D Manim Community API**. For a supported feature, ordinary ManimCE source should ideally require only:

```python
from noon import *
```

instead of:

```python
from manim import *
```

The compatibility reference is Manim Community v0.21.x.

Compatibility is the default goal. Noon should intentionally diverge only when reproducing Manim semantics would create a fundamental design constraint or a material performance regression. A suspected blocker should be measured or demonstrated rather than assumed.

The target is broader than Python compatibility: **Manim's useful object/animation semantics are the language-neutral authoring contract.** Rust and future frontends should expose the same concepts, defaults, lifecycle behavior, timing model, and observable results in idiomatic forms.

## Architecture contract

The intended layering is:

```text
Manim-compatible Python          idiomatic Rust          future frontends
          |                           |                       |
          +------------- thin syntax/type adapters ----------+
                                      |
                                      v
                         shared Rust authoring semantics
                                      |
                       +--------------+--------------+
                       | lifecycle / scene membership |
                       | animation options/defaults   |
                       | target-state animation       |
                       | groups / composition         |
                       | layout / style semantics     |
                       | deterministic rate funcs     |
                       | unsupported-feature policy   |
                       +--------------+--------------+
                                      |
                                      v
                              SceneDefinition
                              / ScenePatch
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

`SceneDefinition` remains the one canonical serialized semantic scene. The shared authoring layer may hold transient state while resolving fluent operations, but it must not become a second persistent scene model.

### What belongs in shared Rust authoring semantics

Rust should own semantics that must agree across languages:

- `Scene.add/remove/clear` and object presence/lifecycle;
- detached-object introduction by animations;
- `.animate` target-state construction and lowering;
- animation defaults and option precedence (`run_time`, `rate_func`, `lag_ratio`, etc.);
- `Transform`, creation/fading animations, removers and introducers;
- group/family expansion and animation composition;
- layout operations such as `next_to`, `align_to`, `arrange`, `to_edge`, and bounds rules;
- style semantics and defaults;
- known deterministic Manim rate functions;
- declarative trackers/signals when introduced;
- decisions about whether a feature has a compiled representation or is explicitly unsupported.

The Python implementation should not independently define these rules.

### What may remain Python-specific

A thin Python layer is still required for language behavior:

- Python class hierarchy, subclassing and `isinstance` behavior;
- positional/keyword argument normalization;
- accepting tuples, lists, NumPy-like vectors and other Python protocols;
- adapting Python iterables and copying Python wrapper metadata;
- discovering `Scene` subclasses and invoking `setup()/construct()/tear_down()`;
- mapping a known Python callable such as `smooth` to a shared semantic rate-function identifier;
- reporting Python-appropriate exceptions.

These adapters should translate into shared semantics rather than implement an independent scheduler or animation engine.

## Compatibility rule

For Python:

> Supported common 2D ManimCE source should run unchanged except for the import.

For Rust and other languages:

> Expose the same semantic vocabulary and behavior idiomatically rather than copying Python syntax.

Compatibility includes public names, constructor forms, common defaults, lifecycle behavior, timing/animation behavior, layout/style semantics, and observable results. It does not require copying Manim's Cairo/OpenGL renderer, internal point-array representation, or imperative Python frame loop.

## Blocker policy

A Manim feature is not considered blocked merely because Manim implements it imperatively.

Use this order:

1. reproduce the observable behavior with existing deterministic semantic tracks;
2. add a deterministic/core representation when doing so preserves Noon's architecture and performance;
3. compile or sample authoring-time Python behavior into a deterministic representation when reasonable;
4. only then declare an explicit compatibility gap.

Examples:

- known rate functions are not blockers; evaluate them in Rust;
- display-space stroke width should be evaluated as a renderer/core semantic option before accepting an approximation;
- `ValueTracker` should prefer a declarative signal/expression representation;
- arbitrary stateful `add_updater(lambda ...)` may be a real blocker for the normal compiled playback path if it cannot be compiled or safely sampled.

Unsupported behavior must fail clearly. Silent semantic drift is not acceptable.

## Current state — 2026-08-23

The original architecture migration and most of the early Manim compatibility foundation are already complete.

Implemented today:

- canonical deterministic `SceneDefinition`/track/runtime pipeline;
- analytic circles/rectangles/lines plus vector paths;
- generic transforms and prepared cross-geometry morphing;
- path reveal/Create support and lifecycle tracks;
- `Scene` subclass discovery and normal `construct()` execution in the browser;
- public Python `Mobject`/`VMobject`/shape classes;
- detached `Create`, `FadeIn`, and detached `.animate` introduction;
- Manim-style 2D acceptance of 3-component vectors with explicit nonzero-Z rejection;
- `Scene.add/remove/clear/mobjects` behavior;
- `Group`/`VGroup` authoring semantics and grouped transforms;
- independent fill/stroke opacity;
- generalized chained `.animate` proxying and builder option precedence;
- Rust `Scene`, `Mobject`, shapes, `.animate`, `Transform`, `Create`, fades, layout and direct lowering into the canonical core scene;
- WebGPU renderer with WebGL2 browser fallback;
- browser compatibility smoke coverage.

The important remaining architectural problem is that the Python compatibility modules still contain too much semantic scheduling/lifecycle/group logic. The next milestone is therefore **cross-language authoring consolidation**, not simply adding more Python wrappers.

## Current milestone — cross-language authoring consolidation

### A1. Shared deterministic rate functions

Make Manim's known rate functions first-class semantic/runtime behavior.

Initial set:

- `linear`;
- exact Manim `smooth` (normalized logistic, default inflection 10);
- `rush_into`;
- `rush_from`;
- `there_and_back`;
- retain Noon's low-level `ease_in_out_cubic` for backwards compatibility.

Requirements:

- Rust runtime owns numerical evaluation;
- Python maps known callable identity/name to the semantic identifier only;
- Rust authoring exposes Manim vocabulary (`rate_func`) while keeping low-level compatibility aliases if useful;
- Manim-compatible animations default to `smooth`, not Noon's cubic approximation;
- numerical tests cover endpoints and representative interior values.

### A2. Shared animation options

Introduce one authoring representation for animation options/default resolution, conceptually:

```text
AnimationOptions
  run_time
  rate_func
  lag_ratio
  path_arc
  reverse_rate_function
  remover / introducer metadata where applicable
```

Both Rust and Python frontends should lower through the same option precedence rules. `Scene.play` overrides per-animation values where Manim does.

### A3. Shared lifecycle and scene membership

Move reusable rules for:

- implicit addition of animated detached objects;
- introducers/removers;
- presence track creation;
- reintroduction;
- scene membership transitions;
- replacement semantics;

out of Python-specific scheduling code and into shared authoring semantics.

### A4. Shared group/composition scheduling

Centralize family expansion, lag interval calculation and composition so Python does not own Manim timing geometry.

This becomes the foundation for:

- `AnimationGroup`;
- `Succession`;
- `LaggedStart`;
- grouped `Create`/`Write`;
- `VGroup.animate(..., lag_ratio=...)`.

### A5. Cross-language parity tests

For every shared semantic feature, test that equivalent Python and Rust authoring produce the same semantic tracks or equivalent compiled scenes.

The goal is not textually identical APIs; it is one semantic implementation.

## Next compatibility milestones

### B — remaining core 2D semantic parity

Resolve the remaining high-impact differences before expanding breadth:

- stroke-width units and scaling semantics;
- remaining `Mobject`/`VMobject` bounds/layout methods such as `set_x`, `set_y` and family behavior where not already complete;
- animation metadata/remover/introducer defaults;
- exact option/error behavior for supported APIs.

Stroke width deserves an explicit design choice. Manim's common VMobject behavior is display-space-like: object scaling generally does not scale stroke width unless requested. Noon currently treats stroke width as local/world geometry. The preferred direction is to support a display/screen-space semantic mode for Manim compatibility while retaining a world-space Noon mode if the renderer can do so efficiently.

### C — animation composition and core shape breadth

Add shared Rust semantics first, then thin Python wrappers:

- `AnimationGroup`;
- `Succession`;
- `LaggedStart`;
- `Uncreate`;
- `Write` groundwork;
- `Dot`, `Ellipse`, `Arc`, `Polygon`, `RegularPolygon`, `Triangle`, `Arrow`, `Vector`, `DashedLine`.

### D — text and mathematical authoring

Implement text as a core subsystem rather than a Python-only compatibility layer:

- `Text`;
- `MarkupText`;
- `Tex`;
- `MathTex`;
- shaped glyph-run/cache/atlas representation for normal text;
- outline extraction for path-level `Create`/`Write`/matching behavior;
- `TransformMatchingTex`.

Do not make vector outlines the permanent representation for all steady-state text.

### E — signals, values and plotting

Build the declarative signal/expression model before updater-like APIs:

- `ValueTracker`;
- deterministic expressions driven by time/signals;
- `DecimalNumber`;
- `NumberLine`, `Axes`, `NumberPlane`;
- `FunctionGraph`, `ParametricFunction`;
- braces and labels.

This is the preferred architectural answer to many Manim scenes that currently use per-frame Python updaters.

### F — advanced VMobject/import surface

- public path-building VMobject APIs (`start_new_path`, `add_line_to`, cubic builders, corner helpers);
- `point_from_proportion`;
- `SVGMobject`;
- `ImageMobject`;
- broader matching transforms and compatibility corpus.

### G — hardening and performance

Continue throughout development, with focused later expansion:

- malformed-path/transform/seek/live-patch property and fuzz coverage;
- larger path/morph performance baselines;
- cache policy refinement if measured workloads require it;
- small controlled visual/golden tests where structural/numerical tests cannot prove correctness;
- browser authoring transport improvements when profiling shows complete-scene JSON remains a material bottleneck.

## Compatibility levels

### Level 1 — common 2D Manim syntax

Target near-source-compatibility for:

- `Scene`, lifecycle and membership;
- `Mobject`, `VMobject`, `Group`, `VGroup`;
- common 2D geometry;
- `Create`, `Uncreate`, fades, transforms;
- layout and style operations;
- `.animate`;
- constants/colors;
- named deterministic rate functions.

### Level 2 — typical mathematical animation

- text/Tex/MathTex;
- writing animations;
- animation composition;
- axes/graphs;
- trackers/declarative expressions;
- braces/labels/decimals.

### Level 3 — rich vector authoring

- broad VMobject path API;
- SVG/image import;
- richer matching transforms.

### Level 4 — compatibility requiring special handling

Examples include arbitrary Python per-frame updaters, arbitrary stateful callback animations, camera internals and 3D behavior. These are evaluated feature-by-feature under the blocker policy rather than categorically rejected in advance.

## CI compatibility contract

Compatibility tests should include ordinary Manim-style source such as:

```python
from noon import *

class BasicCreate(Scene):
    def construct(self):
        circle = Circle(color=BLUE)
        square = Square().next_to(circle, RIGHT)
        self.play(Create(circle), Create(square))
        self.play(circle.animate.shift(UP), run_time=2, rate_func=smooth)
```

Tests should verify:

- source execution succeeds;
- object/track counts and lifecycle are correct;
- final semantic state matches expectations;
- direct seek agrees with forward playback;
- known rate-function values match Manim numerically within defined tolerance;
- Rust and Python equivalents produce semantic parity;
- WebGPU and WebGL2 browser smoke remain valid;
- unsupported features fail with stable, specific messages.

## Relationship to older docs

`docs/implementation-plan.md` is the original low-level architecture milestone plan. Several of its "remaining" subsections have been superseded by completed work; use focused status docs such as `docs/vector-geometry-status.md` for those subsystems.

`docs/manim-animate-semantics.md` and `docs/manim-style-semantics.md` record detailed compatibility behavior/decisions. This document owns the current cross-language authoring roadmap and priority order.

`docs/codex-handoff.md` is historical implementation handoff material and must not be treated as the current roadmap.

## Non-goals

- copying Manim's renderer or internal scene implementation;
- forcing analytic primitives into generic point arrays solely for API compatibility;
- putting Python on the normal frame-critical playback path;
- maintaining separate semantic behavior in each language binding;
- silently pretending unsupported 3D/camera/updater behavior is compatible.
# Manim-aligned, language-neutral authoring plan

## Goal

Make Noon pleasant to author from Python, Rust, and future frontends without making the core language-bound and without adding a second semantic scene representation.

The target is **Manim-like authoring ergonomics over Noon's existing deterministic semantic/runtime architecture**:

```text
Python / Rust / future frontends
              |
              v
      noon-core semantic API
              |
              v
 SceneDefinition / ScenePatch
              |
              v
          compiler
              |
              v
           runtime
              |
              v
            WebGPU
```

`SceneDefinition` remains the single canonical semantic scene model. `SceneDocument` remains only its versioned wire representation. There is no additional AuthoringDocument or frontend-specific semantic IR.

## Execution status — 2026-08-22

PR #20 implements the planned migration through the generic-Transform phase while preserving the single-model architecture.

Completed on the branch:

- Phase 0: legacy implementation and legacy CI lane removed; one canonical `CI` gate remains.
- Phase 1: shared vector/constants vocabulary, Manim Community palette, Python parity, and removal of `noon_layout.py`.
- Phase 2: semantic bounds, object operations, relative layout, `Square`, and lightweight `Group`/`VGroup` authoring.
- Phase 3: transient scene cursor, parallel `play`, `run_time`, `wait`, and curated timing cleanup.
- Phase 4: Python `.animate`, a user-facing Rust `noon` facade/prelude, and semantic-time chaining of sequential animations.
- Phase 5: same-kind analytic Transform fast paths retained; `Circle <-> Rectangle/Square` now lowers through compiler-only fixed path geometry while keeping exact analytic semantic endpoints. Open/closed cross-kind transitions such as `Circle -> Line` remain explicitly unsupported.
- Phase 6: curated examples and root/playground/Transform docs have been rewritten around the final vocabulary; low-level timeline APIs remain as explicit escape hatches rather than the normal teaching path.

Validation before this final status-only commit passed the single canonical CI gate end-to-end, including format, workspace compile, strict Clippy, geometry correctness suites, all Rust workspace tests, native compilation of every picker scene, both browser/WebGPU WASM targets, Python authoring tests, and the browser package build. Cross-kind Transform has dedicated Python, Rust-facade, compiler, and runtime regression coverage.

Remaining work is intentionally outside this migration's acceptance boundary: broader open/closed cross-kind policies, more general filled-polygon topology, text/MathTex architecture, richer animation composition primitives, and other future feature expansion.

## Design rules

1. **One semantic authority.** Layout, bounds, style mutation, lifecycle, transform behavior, timing rules, and other authoring semantics belong in Rust core/shared lowering rather than being independently reimplemented in each frontend.
2. **Thin frontends.** Python and Rust may provide idiomatic syntax, but they should use the same names, concepts, defaults, and semantic rules wherever practical.
3. **No unnecessary persistent abstractions.** Transient builders are acceptable for ergonomics; new serialized semantic layers are not.
4. **Manim vocabulary where it fits.** Reuse familiar names such as `Circle`, `Rectangle`, `Square`, `Group`, `VGroup`, `shift`, `move_to`, `next_to`, `arrange`, `FadeIn`, `Transform`, `run_time`, `UP`, `RIGHT`, `BLUE`, `DEGREES` when Noon can support the same mental model.
5. **Do not copy implementation constraints.** Noon keeps deterministic tracks, compiled playback, Rust/WASM execution, WebGPU rendering, live patching, and renderer-independent semantics.
6. **Explicit low-level escape hatches remain.** Direct tracks, explicit start times, raw colors, and raw `SceneDefinition` mutation remain available for advanced/internal use but are not the normal examples/API path.
7. **Cross-language parity is tested.** Equivalent Python and Rust authoring must normalize to equivalent `SceneDefinition` semantics.

## Current problems at plan creation

### Repository / CI

- The old `noon/` and `examples/` trees are excluded from the current workspace but still retained and separately tested by `Full legacy compatibility`.
- CI naming still distinguishes `Fast architecture gate` and legacy even though the new architecture is now the project.
- The root README still describes the old nannou/Bevy implementation and says the project is no longer actively maintained.

### Python authoring

- Many examples use raw RGB tuples such as `Color(0.34, 0.68, 0.96)`.
- Layout helpers currently live in a browser-only `noon_layout.py` side module rather than the public Noon vocabulary.
- Objects are often created through `scene.circle(...)` and `scene.rectangle(...)` rather than reusable object constructors with familiar methods.
- Animation frequently exposes `from`, `to`, `start_time`, and `duration`, which are closer to timeline IR than user intent.
- Some lifecycle/animation semantics are implemented directly in Python and therefore risk diverging from a Rust frontend.

### Rust semantic API

- `noon-core` currently exposes useful renderer-independent primitives but is intentionally low level: `ObjectId`, `GeometryRef`, `ObjectSnapshot`, `TrackTiming`, and explicit `animate_position(from, to, timing)`.
- It lacks the ergonomic semantic vocabulary users need: named colors, vector arithmetic/constants, object transformations, bounds, relative layout, implicit scene timing, groups, and high-level animation helpers.

## Target user experience

### Python

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

result = scene
```

### Rust

```rust
use noon::prelude::*;

let mut scene = Scene::new();
let circle = scene.add(Circle::new(0.6).color(BLUE).shift(LEFT * 2.0));
let square = scene.add(Square::new(1.2).color(PINK));
scene.edit(square)?.next_to(circle, RIGHT, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER)?;

scene.play((
    circle.animate().shift(RIGHT * 2.0),
    square.animate().rotate(45.0 * DEGREES),
)).run_time(2.0)?;
scene.play(FadeOut::new(circle)).run_time(1.0)?;
scene.wait(0.5)?;
```

Exact Rust syntax differs where ownership/type-system constraints make literal parity awkward, but concepts and names match.

## Canonical vocabulary

### Geometry / objects

Initial aligned set:

- `Circle`
- `Rectangle`
- `Square`
- `Line`
- `Path` / `VectorPath`
- `Group`
- `VGroup`

Text is intentionally deferred until the text architecture exists.

### Constants

Expose directly from Noon rather than a separate layout module:

- `ORIGIN`, `UP`, `DOWN`, `LEFT`, `RIGHT`
- `UL`, `UR`, `DL`, `DR`
- `PI`, `TAU`, `DEGREES`
- `DEFAULT_MOBJECT_TO_EDGE_BUFFER`
- `DEFAULT_MOBJECT_TO_MOBJECT_BUFFER`

### Colors

Provide Manim-style named colors and shade families where useful:

- base names: `WHITE`, `BLACK`, `RED`, `GREEN`, `BLUE`, `YELLOW`, `PURPLE`, `PINK`, `ORANGE`, `TEAL`, `GRAY`/`GREY`
- common shade variants such as `BLUE_A` .. `BLUE_E`, etc., using Manim's palette values
- ergonomic custom color parsing from hex while preserving raw float constructors for low-level use

Color definitions must have one canonical source of truth so Python and Rust cannot drift.

### Object semantic methods

Implement in shared semantics / core where geometry permits:

- position: `move_to`, `shift`, `set_x`, `set_y`, `center`
- transform: `scale`, `rotate`
- style: `set_color`, `set_fill`, `set_stroke`, `set_opacity`
- layout: `next_to`, `align_to`, `to_edge`, `to_corner`
- queries: center/bounds/width/height

These operations mutate or derive `ObjectSnapshot` / object state; they do not create another scene model.

### Groups

Start with lightweight authoring collections of stable object handles/IDs. Do **not** add runtime hierarchy unless a concrete semantic or performance need requires it.

Group operations apply semantic operations to members. `arrange` and `arrange_in_grid` use bounds and buffers, not hardcoded coordinates.

## Bounds and layout

Correct Manim-style layout requires real semantic bounds.

Implement renderer-independent local/world bounds for currently supported geometry:

- circle
- rectangle/square
- line
- vector path

World bounds include translation, rotation, and scale. Group bounds are the union of member bounds.

Relative layout rules are then deterministic functions over these bounds:

```text
next_to(a, b, direction, buff)
align_to(a, b, direction)
to_edge(a, direction, buff)
to_corner(a, corner, buff)
arrange(group, direction, buff)
arrange_in_grid(group, rows/cols, buff)
```

Frame-edge helpers require one canonical logical frame size/aspect definition in core/config rather than browser-specific magic values.

## Scene timing

Add an authoring cursor to the high-level scene API, but keep it transient/non-serialized. The serialized scene still contains explicit tracks.

Rules:

- `scene.play(...)` schedules animations at the current cursor.
- animations in one `play` are parallel by default.
- cursor advances by the play's effective duration.
- `scene.wait(t)` advances the cursor.
- explicit `start_time` remains a low-level override/escape hatch.
- use Manim-style `run_time` in user-facing APIs; internally it maps to track duration.

Composition primitives can follow later in this order:

1. parallel `play`
2. `Succession`
3. `AnimationGroup`
4. `LaggedStart`

## `.animate`

`.animate` is a transient target-state builder, not a persistent semantic layer.

Conceptually:

```text
current ObjectSnapshot
      |
      | apply shift / rotate / style operations once
      v
target ObjectSnapshot
      |
      v
existing Transform/timeline lowering
```

Python and Rust expose idiomatic syntax while sharing snapshot semantics. Bound Python objects evaluate their current snapshot at the scene cursor so sequential `.animate` calls compose from exact prior endpoints.

## Transform semantics

Keep optimized interpolation when source and target geometry are naturally compatible:

- Circle -> Circle: analytic
- Rectangle -> Rectangle: analytic
- Line -> Line: analytic
- compatible Path -> Path: precomputed path morph

Cross-geometry `Circle <-> Rectangle/Square` is implemented by canonicalizing only the active transition through temporary vector geometry while steady-state objects remain analytic before and after the transition. The compiler's prepared `PathPair` is renderer-only; `TrackValues::Object` retains the exact analytic source/target snapshots.

Open/closed topology changes such as Circle -> Line remain intentionally unsupported until Noon has an explicit semantic policy for that collapse/expansion.

This is a compiler/geometry enhancement, not a reason to convert every object to paths permanently.

## Frontend ownership of semantics

### Short term browser Python

The dependency-free Python module remains a thin public facade while shared semantic rules are mirrored by Rust reference behavior and parity tests. New persistent semantic concepts must not be invented only in Python.

### Native Python direction

Long term, PyO3/maturin can expose the Rust semantic layer directly for native Python.

### Browser Python direction

Pyodide can eventually call Rust/WASM semantic operations through a thin bridge for complex shared semantics. Pure Python wrappers may remain for constructor ergonomics and transient builders.

## CI and validation

After legacy removal, there is one required CI job named `CI`.

It runs:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- strict workspace clippy
- geometry correctness tests
- full workspace tests
- wasm/WebGPU checks
- browser package build
- playground Python execution
- every picker `SceneDocument` compiled by native `ScenePlayer`

Authoring-specific validation includes:

- named color/constants parity
- bounds/layout tests
- scene cursor/timing determinism
- semantic-time chained `.animate` regression
- Python/Rust semantic cross-kind Transform parity
- compiler and runtime cross-kind Transform endpoint/seek tests
- curated examples contain no raw RGB literals unless the example is specifically about custom colors
- curated examples minimize absolute positions; exceptions are intentional

## Delivery sequence

### Phase 0 - remove legacy and reset project identity

Changes:

- remove `legacy-compat` CI job
- rename `Fast architecture gate` to `CI`
- remove legacy `noon/` and root `examples/` trees after checking that useful API concepts are captured by this plan/tests
- remove workspace `exclude` entries
- replace obsolete root README with current architecture, playground, and API direction
- update stale planning docs that still describe legacy compatibility as mandatory

Acceptance:

- one CI job is green
- no nannou/Bevy legacy crate remains in the active repository tree
- README describes current Noon accurately

### Phase 1 - canonical math, constants, and colors

Changes:

- extend core `Vec2` arithmetic and direction/corner constants
- add angle and layout buffer constants
- add canonical named color palette and hex parsing
- expose equivalent Python constants from `noon`
- absorb `noon_layout.py` functionality into `noon.py`, then delete the side module/worker loading path
- rewrite examples away from raw RGB constants where a named color communicates intent

Acceptance:

- Python and Rust color/constants parity tests
- no normal gallery example contains `Color(r, g, b...)` literals

### Phase 2 - semantic object operations and bounds

Changes:

- implement geometry/world bounds
- implement object snapshot/state helpers for move/shift/scale/rotate/style operations
- add relative layout functions in core
- expose thin Python methods with Manim-aligned names
- add `Square`
- introduce lightweight group collections and arrange/grid layout

Acceptance:

- numerical bounds/layout tests
- layout examples do not calculate slots manually

### Phase 3 - high-level scene timing and animations

Changes:

- add transient scene authoring cursor
- expose `play(..., run_time=...)`, `wait(...)`
- make explicit start time a low-level path
- centralize Fade/Transform/ReplacementTransform/TransformFromCopy/TransformMatchingShapes scheduling semantics as far into Rust/shared logic as practical without changing the canonical scene format

Acceptance:

- sequential play timing is deterministic
- direct seek behavior remains unchanged
- gallery examples contain no manually chained `start_time` arithmetic except where staggered timing itself is the feature

### Phase 4 - `.animate` and Rust facade

Changes:

- add transient target snapshot builder
- Python `obj.animate.<ops>` syntax
- create a user-facing Rust facade crate so `use noon::prelude::*` exposes ergonomic authoring while low-level crates remain available
- provide equivalent Rust behavior/tests

Acceptance:

- Python and Rust reference scenes normalize to equivalent object/track semantics
- sequential `.animate` starts from the prior semantic endpoint
- `.animate` creates no runtime Python callback dependency

### Phase 5 - generic Transform parity

Changes:

- add compiler-selected cross-geometry transform strategy
- preserve analytic fast paths when possible
- canonical temporary path morph for supported incompatible analytic geometry

Acceptance:

- canonical Circle <-> Square/Rectangle transform works
- endpoints are exact
- direct seek matches forward playback
- no permanent path conversion for static analytic objects
- open/closed topology changes remain explicit compile-time errors

### Phase 6 - final gallery/docs/API cleanup

Changes:

- rewrite curated examples with the final vocabulary
- make each example intentionally demonstrate one feature
- update README/docs with matched Python/Rust examples
- remove obsolete low-level authoring examples or clearly label low-level methods as internals/escape hatches

Acceptance:

- gallery is copyable and semantic
- minimal magic values
- Python/Rust terminology matches
- all picker examples execute, compile, and fit the playground loop

## Non-goals for this migration

- exact Manim API compatibility
- arbitrary Python per-frame updater callbacks
- full Manim class coverage
- 3D
- text/MathTex parity before Noon's text architecture
- replacing deterministic tracks with imperative frame callbacks
- creating a second serialized authoring representation

## Historical execution order

The branch deliberately implemented Phases 0-3 first because they removed repository debt and eliminated most hardcoded demo values/timing without depending on cross-geometry Transform. Phase 4 then added transient ergonomic frontends over the existing snapshot model. Phase 5 was taken only after the ergonomic baseline was green, and reused the existing `PathPair` runtime/render path instead of adding a new semantic layer. Phase 6 documentation/parity cleanup completed the migration.

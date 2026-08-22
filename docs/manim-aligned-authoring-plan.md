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

## Design rules

1. **One semantic authority.** Layout, bounds, style mutation, lifecycle, transform behavior, timing rules, and other authoring semantics belong in Rust core/shared lowering rather than being independently reimplemented in each frontend.
2. **Thin frontends.** Python and Rust may provide idiomatic syntax, but they should use the same names, concepts, defaults, and semantic rules wherever practical.
3. **No unnecessary persistent abstractions.** Transient builders are acceptable for ergonomics; new serialized semantic layers are not.
4. **Manim vocabulary where it fits.** Reuse familiar names such as `Circle`, `Rectangle`, `Square`, `Group`, `VGroup`, `shift`, `move_to`, `next_to`, `arrange`, `FadeIn`, `Transform`, `run_time`, `UP`, `RIGHT`, `BLUE`, `DEGREES` when Noon can support the same mental model.
5. **Do not copy implementation constraints.** Noon keeps deterministic tracks, compiled playback, Rust/WASM execution, WebGPU rendering, live patching, and renderer-independent semantics.
6. **Explicit low-level escape hatches remain.** Direct tracks, explicit start times, raw colors, and raw `SceneDefinition` mutation remain available for advanced/internal use but are not the normal examples/API path.
7. **Cross-language parity is tested.** Equivalent Python and Rust authoring must normalize to equivalent `SceneDefinition` semantics.

## Current problems

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
let square = scene.add(Square::new(1.2).color(PINK).next_to(&circle, RIGHT, DEFAULT_MOBJECT_TO_MOBJECT_BUFFER));

scene.play((
    circle.animate().shift(RIGHT * 2.0),
    square.animate().rotate(45.0 * DEGREES),
)).run_time(2.0)?;
scene.play(FadeOut::new(circle))?;
scene.wait(0.5);
```

Exact Rust syntax may differ where ownership/type-system constraints make literal parity awkward, but concepts and names should match.

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
- common shade variants such as `BLUE_A` .. `BLUE_E`, etc., if we adopt Manim's palette values
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

Python and Rust can expose different implementation syntax while sharing snapshot semantics.

## Transform semantics

Keep optimized interpolation when source and target geometry are naturally compatible:

- Circle -> Circle: analytic
- Rectangle -> Rectangle: analytic
- Line -> Line: analytic
- compatible Path -> Path: precomputed path morph

To align with Manim expectations, later support cross-geometry `Transform` by canonicalizing the transition through temporary vector geometry when necessary while allowing the steady-state objects to remain analytic before/after the transition.

This is a compiler/geometry enhancement, not a reason to convert every object to paths permanently.

## Frontend ownership of semantics

### Short term browser Python

The current dependency-free Python module can remain while the core API is being enriched, but new semantics should not be added only there. Every new operation needs a Rust semantic reference implementation and parity tests.

### Native Python direction

Long term, PyO3/maturin should expose the Rust semantic layer directly for native Python.

### Browser Python direction

Pyodide should eventually call Rust/WASM semantic operations through a thin bridge for complex shared semantics. Pure Python wrappers may remain for constructor ergonomics and transient builders.

## CI and validation

After legacy removal, there is one required CI job named `CI`.

It must continue to run:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- strict workspace clippy
- geometry correctness tests
- full workspace tests
- wasm/WebGPU checks
- browser package build
- playground Python execution
- every picker `SceneDocument` compiled by native `ScenePlayer`

Add authoring-specific validation:

- named color/constants parity
- bounds property tests
- layout relation tests (`next_to`, `align_to`, edge/corner placement)
- scene cursor/timing determinism
- Python/Rust equivalent scenes lower to equivalent normalized scene documents
- curated examples contain no raw RGB literals unless the example is specifically about custom colors
- curated examples minimize absolute positions; exceptions should be intentional and tested/documented

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
- gallery examples contain no manually chained `start_time` arithmetic

### Phase 4 - `.animate` and Rust facade

Changes:

- add transient target snapshot builder
- Python `obj.animate.<ops>` syntax
- create/rename a user-facing Rust facade crate if necessary so `use noon::prelude::*` exposes ergonomic authoring while low-level crates remain available
- provide equivalent Rust example(s)

Acceptance:

- Python and Rust reference scenes normalize to equivalent objects/tracks
- `.animate` creates no runtime Python callback dependency

### Phase 5 - generic Transform parity

Changes:

- add compiler-selected cross-geometry transform strategy
- preserve analytic fast paths when possible
- canonical temporary path morph for incompatible analytic geometry

Acceptance:

- canonical Circle <-> Square/Rectangle transform works
- endpoints are exact
- no permanent path conversion for static analytic objects

### Phase 6 - final gallery/docs/API cleanup

Changes:

- rewrite all curated examples with the final vocabulary
- make each example intentionally demonstrate one feature
- update README/docs with matched Python/Rust examples
- remove obsolete low-level authoring examples or clearly label them as internals

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

## Immediate execution order on this branch

Implement Phases 0-3 first because they remove repository debt and eliminate most hardcoded demo values/timing without depending on cross-geometry Transform. Then implement Phase 4 where it can be done cleanly on the existing snapshot model. Treat Phase 5 as the first larger compiler/geometry slice and do not destabilize already-green runtime behavior merely to imitate Manim syntax.

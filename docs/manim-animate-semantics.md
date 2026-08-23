# Manim `.animate` compatibility semantics

Noon's public Python surface targets the observable authoring behavior of Manim Community v0.21.x while keeping playback compiled and deterministic. The same timing and target-state semantics should be available through the Rust authoring facade; Python should adapt syntax, not own a separate scheduler.

## Supported builder contract

The preferred Python form is ordinary Manim syntax:

```python
self.play(
    square.animate(run_time=2, rate_func=linear)
        .shift(RIGHT)
        .rotate(PI / 4)
)
```

The compatibility surface mirrors these Manim builder rules:

- animation kwargs are supplied by calling `.animate(...)` before any method access;
- animation kwargs can be supplied only once;
- mutating Mobject methods can be chained;
- all chained mutations build one detached target state and lower to one Noon `Transform` track per runtime object;
- `.animate` works for Mobjects that have not previously been added to the scene; `Scene.play` binds them automatically;
- `Scene.play(..., run_time=..., rate_func=..., lag_ratio=...)` overrides corresponding builder values;
- animations in one `Scene.play` may have different builder-level run times, and the scene cursor advances by the longest animation;
- grouped/family animation uses Manim-compatible lag interval geometry while the runtime scene may remain flat.

Equivalent Rust should express the same semantics idiomatically, for example:

```rust
scene
    .play(square.animate().shift(RIGHT).rotate(PI / 4.0))
    .rate_func(RateFunction::Linear)
    .run_time(2.0)?;
```

The Rust and Python forms should lower through the same shared option/default and scheduling implementation as that consolidation lands.

## Deterministic lowering

`.animate` remains an authoring-time target-state feature:

```text
Mobject.animate(...).method(...).method(...)
                |
                v
       detached target snapshot
                |
                v
          Transform track
                |
                v
       Rust/WASM evaluation
```

No mutator or Python rate-function callback executes once playback begins.

## Animation arguments

Represented or intentionally accepted today:

- `run_time`;
- `rate_func` for known deterministic functions;
- `lag_ratio` for grouped/family lowering;
- `path_arc=0` as the straight-path case;
- `reverse_rate_function=False`;
- `suspend_mobject_updating` and `name` as accepted metadata/no-op values while Noon has no Python playback updater loop.

Currently rejected explicitly:

- nonzero `path_arc`, until curved transform paths are represented in shared semantics;
- `reverse_rate_function=True`, until reversed rate functions are represented in the semantic timing model;
- arbitrary Python rate functions that cannot yet be compiled/sampled into a deterministic representation.

These are implementation-state gaps, not blanket non-goals. They should be revisited under the compatibility blocker policy in `manim-aligned-authoring-plan.md`.

## Rate-function semantics

Manim's animation default is `smooth`, not linear. ManimCE v0.21.x defines `smooth` as a normalized logistic sigmoid with default inflection 10.

The shared semantic/runtime layer should own numerical evaluation for known functions. The initial compatibility set is:

- `linear`;
- `smooth`;
- `rush_into`;
- `rush_from`;
- `there_and_back`;
- Noon's existing low-level `ease_in_out_cubic` retained as a backwards-compatible non-Manim easing.

Python should only identify a known callable and emit/select the shared semantic identifier. It must not approximate `smooth` with a different curve.

## Shared timing/default resolution

The end state is one shared `AnimationOptions`/timing implementation that resolves:

```text
animation defaults
    + animation/builder options
    + Scene.play overrides
    -> explicit child timing/rate-function tracks
```

This shared layer should also own grouped lag timing. The current Python implementation contains some of this scheduling logic; moving it into Rust is part of the active authoring-consolidation milestone.

## Runtime hierarchy

Groups may remain authoring-time structure when runtime hierarchy is unnecessary. Grouped method animations can lower to member transforms while preserving batching, analytic primitive fast paths, cached path geometry, and stable runtime object identities. The flattening/scheduling rule itself belongs in shared authoring semantics so every frontend observes the same behavior.
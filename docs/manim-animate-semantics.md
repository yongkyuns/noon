# Manim `.animate` compatibility semantics

Noon's public Python surface targets the observable authoring behavior of Manim Community v0.21.x while keeping playback compiled and deterministic.

## Supported builder contract

The preferred form is ordinary Manim syntax:

```python
self.play(
    square.animate(run_time=2, rate_func=linear)
        .shift(RIGHT)
        .rotate(PI / 4)
)
```

The compatibility layer mirrors these Manim builder rules:

- animation kwargs are supplied by calling `.animate(...)` before any method access;
- animation kwargs can be supplied only once;
- mutating Mobject methods can be chained;
- all chained mutations build one detached target state and lower to one Noon `Transform` track per runtime object;
- `.animate` works for Mobjects that have not previously been added to the scene; `Scene.play` binds them automatically;
- `Scene.play(..., run_time=..., rate_func=..., lag_ratio=...)` overrides the corresponding builder values;
- animations in one `Scene.play` may have different builder-level run times, and the scene cursor advances by the longest animation;
- `VGroup.animate(..., lag_ratio=x)` lowers to staggered member tracks using Manim's `get_sub_alpha` interval geometry while the runtime scene remains flat.

For example:

```python
self.play(
    left.animate(run_time=0.5).shift(UP),
    right.animate(run_time=2.0).shift(DOWN),
)
```

starts both animations together and advances the scene by two seconds.

## Deterministic lowering

`.animate` remains an authoring-time compiler feature:

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

Currently represented directly:

- `run_time`
- `rate_func=linear`
- `rate_func=smooth` through Noon's current deterministic smooth mapping
- `lag_ratio` for grouped/family lowering
- `path_arc=0` as the straight-path case
- `reverse_rate_function=False`
- `suspend_mobject_updating` and `name` are accepted metadata/no-op values because Noon has no Python playback updaters

Currently rejected explicitly:

- nonzero `path_arc`, until curved transform paths are represented in the canonical track model;
- `reverse_rate_function=True`, until reversed easing is represented in the easing IR;
- arbitrary Python rate functions, because they would require Python execution during playback.

## Remaining rate-function parity

Manim's default `smooth` is a normalized logistic sigmoid with inflection 10. Noon's current deterministic compatibility mapping uses the existing `ease_in_out_cubic` easing. The API and scheduling behavior are aligned, but the exact curve is not yet numerically identical.

The intended follow-up is to add Manim's named rate functions as first-class deterministic runtime easings/curves rather than invoke Python per frame.

## Runtime hierarchy

Groups remain authoring-time structure. A grouped method animation is flattened into member transforms, preserving Noon's existing batching, analytic primitive fast paths, cached path geometry, and stable runtime object identities.

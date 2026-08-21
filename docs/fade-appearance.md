# Fade and appearance semantics

Noon models fade visibility as a dedicated timeline channel rather than rewriting an object's semantic style opacity.

## Why appearance is separate from opacity

`Style.opacity` belongs to the authored object. It can be part of an object's design, can be animated directly, and can participate in generic `Transform` interpolation.

`Appearance` is a normalized runtime visibility multiplier used by lifecycle-style animations such as `FadeIn` and `FadeOut`.

The renderer composes them as:

```text
effective opacity = semantic Style.opacity * Appearance
```

This means an object authored with opacity `0.4` remains semantically `0.4` throughout a fade. Halfway through a fade from appearance `1.0` to `0.0`, its packed renderer opacity is `0.2`. Fading it back in restores the rendered opacity to `0.4`, not `1.0`.

Appearance is clamped to `[0, 1]` by the runtime. It does not change path tessellation, mesh-cache identity, or GPU instance layout.

## FadeOut

For `FadeOut(object)` starting at `t0` with duration `d`:

1. the object must be present at `t0`;
2. an `Appearance` track runs from the evaluated appearance at `t0` to `0.0` over `[t0, t0 + d]`;
3. a `Presence` event changes the object from present to absent exactly at `t0 + d`.

The semantic object, stable ID, geometry, transform, and style are retained while absent.

## FadeIn

For `FadeIn(object)` starting at `t0` with duration `d`:

1. the object must be absent at `t0` when it already participates in a lifecycle chain;
2. a `Presence` event changes it from absent to present exactly at `t0`;
3. an `Appearance` track runs from the evaluated appearance at `t0` to `1.0` over `[t0, t0 + d]`.

A first-use `FadeIn` establishes the initial lifecycle state by making the first Presence event `false -> true` and starts Appearance at `0.0`. Therefore direct seek before `t0` sees the object as absent and direct seek during/after the fade agrees with forward playback.

## Chaining and authoring rules

Fade operations for the same object are authored chronologically and may not overlap. The next fade starts from the appearance produced by the previous timeline state rather than assuming a fixed endpoint.

A normal chain is therefore:

```text
FadeOut: Appearance 1 -> 0; Presence true -> false at end
FadeIn:  Presence false -> true at start; Appearance 0 -> 1
```

Presence continuity is still compiler-validated. Failed high-level Fade authoring is transactional: object/track identity and scheduler state are restored if any animation in a `Scene.play(...)` call fails.

## Composition with other animation

Appearance is deliberately independent of generic `Transform`, Position, Rotation, Opacity, Reveal, and Morph channels. A Transform may change semantic style opacity while a Fade changes appearance; the renderer combines the current values rather than allowing either animation to overwrite the other.

Changing appearance only repacks the affected instance record. It does not retessellate vector geometry or invalidate the path mesh cache.

## Validation coverage

The implementation is covered at four layers:

- core/compiler tests verify `Appearance` is a distinct scalar property with independent dynamic-property accounting;
- runtime tests verify normalized evaluation, seek/rewind determinism, and independence from semantic opacity;
- renderer tests verify packed opacity is `Style.opacity * Appearance`;
- Python tests verify `FadeIn`, `FadeOut`, Presence handoffs, chaining, overlap rejection, and transactional rollback.

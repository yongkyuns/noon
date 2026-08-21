# TransformMatchingShapes composition

## Goal

`TransformMatchingShapes` is a deterministic authoring primitive for moving a collection of stable source objects into a collection of stable target objects by shape identity. It is deliberately a frontend composition feature: matching happens while authoring, then each matched pair lowers to the same generic `Transform` plus `Presence` handoff already used by `ReplacementTransform`.

No runtime, compiler, or renderer special case is introduced.

## Authoring form

```python
scene.play(
    TransformMatchingShapes(
        [left_circle, left_square, right_circle],
        [new_square, new_circle_a, new_circle_b],
        key="rearrange",
    ),
    duration=2.0,
)
```

Sources and targets are ordered collections of scene-owned `Object` handles. They must be non-empty, internally unique, and disjoint from one another.

## Matching contract

Matching uses each source's evaluated geometry at animation start and each target's evaluated geometry at handoff. Style, position, rotation, scale, and opacity do not participate in the match key; they remain ordinary generic-Transform channels after a pair is selected.

The initial deterministic shape signatures are intentionally conservative:

- `Circle`: all circles share one signature; radius is a transformable size parameter.
- `Line`: all line segments share one signature.
- `Rectangle`: rectangles match when their normalized aspect ratio is equal; width/height ordering is ignored so a rotated aspect-equivalent rectangle remains matchable.
- `VectorPath`: paths match only when their local renderer-independent command geometry is exactly equal. This is sufficient for repeated glyph-like/path objects authored from the same outline while avoiding approximate contour heuristics.

For duplicate signatures, stable input order is the tie-breaker: the first unmatched source of a signature pairs with the first unmatched target of that signature.

Every source and target must be matched exactly once. A signature/cardinality mismatch is rejected transactionally. Noon does not emulate unmatched-part fades with opacity tracks because Fade does not yet have first-class lifecycle semantics.

## Lowering

After matching, pair `i` lowers exactly as:

```python
ReplacementTransform(source_i, target_i, key=f"{root}.match:{i}")
```

All pairs share the caller's start time, duration, and easing. The existing lifecycle state machine therefore supplies:

- source-present validation;
- target-absent validation;
- evaluated snapshots;
- exact end-time source hide / target show;
- chained lifecycle composition;
- transactional rollback;
- direct-seek and rewind determinism.

The default root key is derived deterministically from the ordered source and target authoring keys. Explicit keys remain preferable for long or externally reconciled scenes.

## Safety boundaries

The first version rejects:

- foreign objects;
- duplicate source or target handles;
- any object appearing in both source and target collections;
- empty source or target collections;
- unmatched shape signatures or counts;
- vector paths that are merely geometrically similar rather than exactly equal in local command form;
- any pair that violates the existing `ReplacementTransform` snapshot, precedence, or lifecycle rules.

These are authoring errors rather than renderer fallbacks.

## Validation

Coverage should verify:

- deterministic pairing independent of target shape ordering;
- stable tie-breaking for duplicate shapes;
- analytic aspect-ratio matching;
- exact local-vector-path matching;
- deterministic generated track identities;
- transactional mismatch/duplicate/foreign-object rejection;
- simultaneous multi-pair runtime handoff and direct-seek/rewind parity through the existing lifecycle runtime.

## Follow-up

When text outlines and richer groups land, they can feed the same matching primitive. A later canonical path-signature layer may normalize contour start, winding, translation, scale, and compatible curve representations. Unmatched-part behavior should wait for explicit Fade/appearance semantics rather than being hidden inside this feature.

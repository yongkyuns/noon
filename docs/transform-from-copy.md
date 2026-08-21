# TransformFromCopy lifecycle semantics

## Goal

`TransformFromCopy` preserves the original source object while a distinct transient copy moves through Noon's existing generic Transform pipeline toward a stable target object. Object lifetime remains an explicit semantic concern expressed with `Presence`; playback does not create or destroy objects.

This design builds directly on [transform-lifecycle.md](transform-lifecycle.md), reuses the geometry/interpolation rules in [generic-transform.md](generic-transform.md), samples timeline state according to [evaluated-authoring-snapshots.md](evaluated-authoring-snapshots.md), and participates in the sequential state machine described in [chained-lifecycle.md](chained-lifecycle.md).

## Contract

The authoring form is:

```python
scene.play(
    TransformFromCopy(source, target, key="source-copy-to-target"),
    duration=2.0,
    start_time=1.0,
)
```

The scene owns three stable identities:

1. `source`: visible throughout the animation;
2. `target`: absent before the end-time handoff, visible afterward;
3. an internally authored transient copy: absent before `start_time`, visible only during the transform interval, absent afterward.

The transient copy is a snapshot of the source evaluated at copy start and receives a deterministic authoring identity so equivalent Python reruns map it back to the same stable runtime object.

## Lowering

One `TransformFromCopy` lowers to a normal generic Transform plus discrete lifecycle events:

```text
Presence(copy):   false -> true   at start
Transform(copy):  evaluated source snapshot -> evaluated target snapshot   over [start, end]
Presence(copy):   true -> false   at end
Presence(target): false -> true   at end
```

`source` receives no lifecycle event and therefore remains present. At the exact start time both source and copy are visible. At the exact end time the copy disappears and target appears. The copy and target never need playback-time insertion/removal.

Position, Rotation, and Opacity are sampled from their latest-started tracks at the relevant snapshot time using runtime-equivalent interpolation and easing. Thus the copy visually starts from the source's actual snapshot-representable state, and the Transform destination agrees with the target at handoff.

## Presence-chain invariant

The transient copy has a continuous two-event Presence chain:

```text
false -> true
true  -> false
```

Adjacent events for one object must agree (`previous.to == next.from`). Python authoring validates the chain as it is built. The Rust compiler independently validates sorted `Presence` tracks before runtime playback, and add/replace/remove-track live patches are validated transactionally before they replace the compiled track set. An inconsistent later `from` value is therefore rejected rather than silently ignored.

## Reuse and chaining

Lifecycle participation is no longer one-shot.

A source that is still present may seed multiple sequential `TransformFromCopy` operations because each moving copy is a fresh stable transient object. A target that became present through an earlier lifecycle handoff can later act as a lifecycle source. For example:

```python
scene.play(TransformFromCopy(a, b), duration=1.0, start_time=0.0)
scene.play(ReplacementTransform(b, c), duration=1.0, start_time=1.0)
```

A target with existing lifecycle history may be reused as a target only when its current Presence state is absent. Lifecycle events for each participant must be authored chronologically; this prevents a later authoring call from inserting ownership into the middle of an already-authored Presence timeline.

## Safety boundaries

The implementation rejects composition that cannot be captured exactly:

- source and target must be distinct objects owned by the same `Scene`;
- lifecycle operations for an existing participant must be authored chronologically;
- the source must be present at copy start;
- a reused target must be absent at copy start and remains so until the generated handoff;
- a snapshot taken inside an active generic Transform is rejected;
- snapshot state outside `ObjectSnapshot` (`Reveal` and standalone `Morph`) is rejected when active at the relevant source/target snapshot time;
- transient copy and generated track keys are deterministic and duplicate keys are rejected within one scene.

Presence itself is handled by the lifecycle state machine rather than embedded in `ObjectSnapshot`.

Completed generic Transform endpoints are supported. Future generic Transforms that have not started at the source-copy or target-handoff time do not contaminate the snapshot and can coexist with the lifecycle operation when their timeline ordering is otherwise valid.

## Transactionality

`Scene.play(...)` is transactional. If TransformFromCopy lowering fails after the transient copy has been allocated—for example because a generated track key collides—the entire play operation rolls back. Objects, tracks, identity allocation, and scheduled Transform state return to their exact pre-call state.

See [transactional-authoring.md](transactional-authoring.md).

## Validation

The implementation is covered end to end:

- Python lowering tests verify the stable transient copy and exact presence/Transform tracks;
- explicit and generated transient-copy identities are deterministic across equivalent reruns;
- evaluated source/target tests verify Position and Opacity state at copy start/handoff;
- lifecycle-chain tests verify source reuse, target-as-later-source composition, absent-source rejection, present-target rejection, and chronological authoring;
- runtime tests verify direct seek, sequential forward playback, and rewind at pre-start, start, midpoint, and end;
- renderer incremental preparation verifies visible instance IDs across all lifecycle phases;
- compiler tests reject discontinuous presence chains before runtime playback;
- patch tests reject add/remove operations that would break a presence chain without mutating the previously valid compiled tracks;
- transactional authoring tests verify failed compound lowering leaves no transient IDs or scheduled state behind.

## Follow-up

Sequential lifecycle composition now removes the artificial one-use restriction. The next higher-value Transform work is matching-shape correspondence; genuinely overlapping lifecycle graphs should be added only if their ownership and precedence semantics are made explicit.

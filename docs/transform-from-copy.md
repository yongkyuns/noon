# TransformFromCopy lifecycle semantics

## Goal

`TransformFromCopy` preserves the original source object while a distinct transient copy moves through Noon's existing generic Transform pipeline toward a stable target object. Object lifetime remains an explicit semantic concern expressed with `Presence`; playback does not create or destroy objects.

This design builds directly on [transform-lifecycle.md](transform-lifecycle.md) and reuses the geometry/interpolation rules in [generic-transform.md](generic-transform.md).

## Initial bounded contract

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

The transient copy is a snapshot of the source at scheduling time and receives a deterministic authoring identity so equivalent Python reruns map it back to the same stable runtime object.

## Lowering

One `TransformFromCopy` lowers to a normal generic Transform plus discrete lifecycle events:

```text
Presence(copy):   false -> true   at start
Transform(copy):  source snapshot -> target snapshot   over [start, end]
Presence(copy):   true -> false   at end
Presence(target): false -> true   at end
```

`source` receives no lifecycle event and therefore remains present. At the exact start time both source and copy are visible. At the exact end time the copy disappears and target appears. The copy and target never need playback-time insertion/removal.

## Presence-chain invariant

This is the first lifecycle primitive that naturally gives one object multiple presence events. The copy's chain is continuous:

```text
false -> true
true  -> false
```

Adjacent events for one object must agree (`previous.to == next.from`). Python authoring validates the chain as it is built. The Rust compiler independently validates sorted `Presence` tracks before runtime playback, and add/replace/remove-track live patches are validated transactionally before they replace the compiled track set. An inconsistent later `from` value is therefore rejected rather than silently ignored.

## Initial safety boundaries

The bounded implementation rejects ambiguous composition rather than synthesizing hidden precedence rules:

- source and target must be distinct objects owned by the same `Scene`;
- source or target objects already participating in another lifecycle animation are rejected;
- source generic Transform state overlapping the copy start is rejected;
- target generic Transform state overlapping the copy interval is rejected;
- source narrow-property state at or before copy start is rejected until source state can be evaluated into an exact snapshot;
- target narrow-property state at or before handoff is rejected until target state can be evaluated into an exact snapshot;
- transient copy and generated track keys are deterministic and duplicate keys are rejected within one scene.

Completed generic Transform tracks that end before the relevant snapshot time are supported: the scheduled Transform target snapshot is used as the source or target semantic snapshot.

## Validation

The implementation is covered end to end:

- Python lowering tests verify the stable transient copy and exact presence/Transform tracks;
- explicit and generated transient-copy identities are deterministic across equivalent reruns;
- runtime tests verify direct seek, sequential forward playback, and rewind at pre-start, start, midpoint, and end;
- runtime tests verify the source remains present, the copy is visible only during the interval, and the target appears exactly at the end;
- renderer incremental preparation verifies visible instance IDs across all lifecycle phases;
- compiler tests reject discontinuous presence chains before runtime playback;
- patch tests reject add/remove operations that would break a presence chain without mutating the previously valid compiled tracks.

## Follow-up

The next useful lifecycle work is to generalize composition without weakening determinism: repeated copy transforms, chained replacements, evaluated source/target snapshots, and matching-shape Transform variants can build on the same stable-ID/presence machinery. Generated lifecycle operations should also become fully transactional at the authoring layer so any late validation error leaves the `Scene` unchanged.

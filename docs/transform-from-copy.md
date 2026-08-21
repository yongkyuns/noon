# TransformFromCopy lifecycle semantics

## Goal

`TransformFromCopy` should preserve the original source object while a distinct transient copy moves through Noon's existing generic Transform pipeline toward a stable target object. Object lifetime remains an explicit semantic concern expressed with `Presence`; playback does not create or destroy objects.

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

The transient copy is a snapshot of the source at scheduling time and receives a deterministic authoring identity so Python reruns map it back to the same stable runtime object.

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

This is the first lifecycle primitive that naturally gives one object multiple presence events. The copy's chain must be continuous:

```text
false -> true
true  -> false
```

Adjacent events for one object must agree (`previous.to == next.from`). The implementation should validate this invariant rather than silently ignoring an inconsistent later `from` value. This makes arbitrary seek and forward playback describe the same state transition sequence.

## Initial safety boundaries

The first implementation should reject ambiguous composition rather than synthesize hidden precedence rules:

- source and target must be distinct objects owned by the same `Scene`;
- source/target objects already participating in incompatible lifecycle replacement state are rejected;
- target generic Transform state overlapping the copy interval is rejected;
- target narrow-property state at or before handoff is rejected until lifecycle lowering can snapshot evaluated target state exactly;
- transient copy keys must be deterministic and collision-free within one scene.

## Validation

The first slice should prove:

- Python lowering creates a stable transient copy and exact presence/Transform tracks;
- transient copy identity is deterministic across equivalent reruns;
- direct seek, sequential forward playback, and rewind agree at pre-start, start, midpoint, and end;
- source remains present throughout;
- copy is visible only during the interval;
- target appears exactly at the end;
- renderer preparation contains the correct visible instance count at each lifecycle phase;
- inconsistent presence chains are rejected before runtime playback.

## Follow-up

After this bounded contract is green, lifecycle composition can be generalized: repeated copy transforms, chained replacements, evaluated target snapshots, and eventually matching-shape Transform variants can build on the same stable-ID/presence machinery.

# Transform lifecycle semantics

## Status

Noon models object lifetime independently from opacity. `Presence` is a first-class, renderer-independent boolean timeline property represented as a zero-duration event. This lets lifecycle animations preserve stable `ObjectId` values while arbitrary seek, forward playback, live reconciliation, and rendering all agree on whether an object exists in the visible scene at a given time.

The lifecycle animations built on this primitive are `ReplacementTransform` and `TransformFromCopy`. The geometry/interpolation contract they reuse is documented in [generic-transform.md](generic-transform.md), while authoring-time state evaluation is documented in [evaluated-authoring-snapshots.md](evaluated-authoring-snapshots.md).

## Presence contract

A presence track is discrete rather than interpolated:

```text
Property::Presence
TrackValues::Bool { from, to }
TrackTiming { start_time, duration: 0, easing: Linear }
```

The first event's `from` value defines the object's pre-event presence state. Once an event time is reached, its `to` value becomes active. Ordinary interpolated tracks still require positive duration; zero duration is accepted only for instant properties such as `Presence`.

`FrameState` retains every semantic object and exposes a parallel presence vector. An absent object therefore keeps its stable identity and semantic state, but is excluded from renderer preparation and produces no GPU instance or draw work. When presence changes, renderer preparation treats the slot topology as structural and rebuilds the affected prepared layout rather than pretending that appearance/disappearance is an instance-only value update.

## ReplacementTransform

The authoring API is:

```python
source = scene.circle(1.0, key="source")
target = scene.circle(1.5, position=(2.0, 1.0), key="target")
scene.play(
    ReplacementTransform(source, target, key="source-to-target"),
    duration=2.0,
)
```

Both `source` and `target` are stable scene-owned objects with distinct IDs. The target is semantically present in the scene definition but hidden by its first presence event until the handoff.

The authoring frontend lowers one replacement into three normal language-neutral tracks:

1. a generic `Transform` on the source from the source state evaluated at the transform start to the target state evaluated at the handoff time;
2. `Presence(source, true -> false)` at the exact transform end time;
3. `Presence(target, false -> true)` at the same time.

Before the handoff, only the source renders. During the interval, the source identity carries the normal generic Transform interpolation. At the exact end time the source becomes absent and the target becomes present in the same Position/Rotation/Opacity state used as the Transform destination. Neither object is created or destroyed during playback.

This contract makes direct seek and sequential playback equivalent. Seeking backward before the handoff restores source presence and hides the target without reconstructing identities or replaying authoring code.

## Evaluated handoff state

Python authoring evaluates snapshot-representable timeline state at the relevant lifecycle time instead of requiring the object to have no prior narrow-property animation.

For `ReplacementTransform`, target `Position`, `Rotation`, and `Opacity` use their runtime-equivalent values at the exact handoff. Completed generic Transforms are also reflected in the snapshot. A generic Transform that is still active at the snapshot time remains rejected because an arbitrary in-progress path morph cannot be represented faithfully by one `ObjectSnapshot`.

Channels that are independent of `ObjectSnapshot`, such as `Presence`, `Reveal`, and standalone `Morph` state, remain explicit safety boundaries for lifecycle snapshots. Noon rejects them rather than pretending the Transform endpoint captures state it does not encode.

## Current safety boundaries

Lifecycle composition remains deliberately bounded where exact semantics are not yet defined:

- source and target must be different objects owned by the same `Scene`;
- an object may participate in only one lifecycle animation in the current composition model;
- a lifecycle snapshot cannot be taken inside an active generic Transform;
- lifecycle snapshot state that is not representable by `ObjectSnapshot` is rejected;
- narrow `Position`, `Rotation`, and `Opacity` tracks are evaluated with the same latest-started-track precedence and easing rules as runtime playback.

These are explicit authoring restrictions, not renderer fallbacks. Noon should not silently produce a discontinuous handoff.

## Validation

Coverage spans the semantic pipeline:

- core: zero-duration boolean presence validation and deterministic IDs;
- IR: human-readable presence/bool JSON round trip;
- compiler: presence dynamic classification, chain continuity, and patch validation;
- runtime: direct seek, forward playback, rewind, and live-patch presence parity;
- renderer: absent objects keep semantic slots but create no GPU instances; toggling presence rebuilds the prepared slot layout;
- Python: lifecycle lowering, evaluated Position/Rotation/Opacity snapshots, deterministic transient-copy identity, transactional rollback, and rejection of active/unrepresentable snapshot state.

## Next work

The next lifecycle composition step is to support repeated/chained lifecycle operations with explicit state-machine semantics rather than the current one-lifecycle-animation-per-object guard. Matching-shape transforms can then build on the same evaluated snapshot and stable-identity machinery.

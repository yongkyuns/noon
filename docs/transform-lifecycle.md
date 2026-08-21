# Transform lifecycle semantics

## Status

Noon models object lifetime independently from opacity. `Presence` is a first-class, renderer-independent boolean timeline property represented as a zero-duration event. This lets lifecycle animations preserve stable `ObjectId` values while arbitrary seek, forward playback, live reconciliation, and rendering all agree on whether an object exists in the visible scene at a given time.

The first lifecycle animation built on this primitive is `ReplacementTransform`.

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

The initial authoring API is deliberately bounded:

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

1. a generic `Transform` on the source from the source snapshot to the target snapshot;
2. `Presence(source, true -> false)` at the exact transform end time;
3. `Presence(target, false -> true)` at the same time.

Before the handoff, only the source renders. During the interval, the source identity carries the normal generic Transform interpolation. At the exact end time the source becomes absent and the target becomes present in its own exact semantic state. Neither object is created or destroyed during playback.

This contract makes direct seek and sequential playback equivalent. Seeking backward before the handoff restores source presence and hides the target without reconstructing identities or replaying authoring code.

## Current safety boundaries

The first `ReplacementTransform` contract rejects cases whose handoff state would otherwise be ambiguous:

- source and target must be different objects owned by the same `Scene`;
- an object may participate in only one lifecycle replacement in this initial slice;
- overlapping generic Transform state on the target is rejected;
- target narrow-property state (`Position`, `Rotation`, `Opacity`, reveal/morph, etc.) at or before the handoff is rejected because it is not represented by the target object snapshot the source transforms toward.

These are explicit authoring restrictions, not renderer fallbacks. Noon should not silently produce a discontinuous handoff.

## Validation

Coverage spans the semantic pipeline:

- core: zero-duration boolean presence validation and deterministic IDs;
- IR: human-readable presence/bool JSON round trip;
- compiler: presence dynamic classification and patch validation;
- runtime: direct seek, forward playback, rewind, and live-patch presence parity;
- renderer: absent objects keep semantic slots but create no GPU instances; toggling presence rebuilds the prepared slot layout;
- Python: `ReplacementTransform` lowers to Transform plus the atomic two-object presence handoff, with foreign/self/reused/ambiguous targets rejected.

## Next work

`TransformFromCopy` should reuse the same presence machinery without changing source identity: a dedicated copy object is present only for the animation interval, the original source remains visible, and the destination lifecycle is explicit. After that, lifecycle composition restrictions can be relaxed deliberately, with tests for chained replacements and matching-shape transforms rather than by weakening the current deterministic contract.

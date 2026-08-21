# Transform lifecycle semantics

## Status

Noon models object lifetime independently from opacity. `Presence` is a first-class, renderer-independent boolean timeline property represented as a zero-duration event. This lets lifecycle animations preserve stable `ObjectId` values while arbitrary seek, forward playback, live reconciliation, and rendering all agree on whether an object exists in the visible scene at a given time.

The lifecycle animations built on this primitive are `ReplacementTransform` and `TransformFromCopy`. The geometry/interpolation contract they reuse is documented in [generic-transform.md](generic-transform.md), authoring-time state evaluation is documented in [evaluated-authoring-snapshots.md](evaluated-authoring-snapshots.md), and repeated lifecycle composition is documented in [chained-lifecycle.md](chained-lifecycle.md).

## Presence contract

A presence track is discrete rather than interpolated:

```text
Property::Presence
TrackValues::Bool { from, to }
TrackTiming { start_time, duration: 0, easing: Linear }
```

The first event's `from` value defines the object's pre-event presence state. Once an event time is reached, its `to` value becomes active. Ordinary interpolated tracks still require positive duration; zero duration is accepted only for instant properties such as `Presence`.

Multiple Presence events on one object form a state machine. Adjacent events must be continuous (`previous.to == next.from`), and the compiler independently enforces that invariant before playback and after live track patches.

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

Both `source` and `target` are stable scene-owned objects with distinct IDs. A first-use target receives `Presence(false -> true)` at handoff, which establishes that it is absent before the handoff.

The authoring frontend lowers one replacement into three normal language-neutral tracks:

1. a generic `Transform` on the source from the source state evaluated at the transform start to the target state evaluated at the handoff time;
2. `Presence(source, true -> false)` at the exact transform end time;
3. `Presence(target, false -> true)` at the same time.

Before the handoff, only the source renders. During the interval, the source identity carries the normal generic Transform interpolation. At the exact end time the source becomes absent and the target becomes present in the same Position/Rotation/Opacity state used as the Transform destination. Neither object is created or destroyed during playback.

This contract makes direct seek and sequential playback equivalent. Seeking backward before the handoff restores source presence and hides the target without reconstructing identities or replaying authoring code.

## Chained lifecycle composition

Lifecycle participation is no longer one-shot. Python derives whether an object can be reused from its existing Presence timeline.

This permits exact-boundary chains such as:

```python
scene.play(ReplacementTransform(a, b), duration=1.0, start_time=0.0)
scene.play(ReplacementTransform(b, c), duration=1.0, start_time=1.0)
```

`b` receives a continuous Presence chain:

```text
false -> true  @ 1.0
true  -> false @ 2.0
```

An object hidden by an earlier operation may also be a later target when its latest Presence state is false. Likewise, a target made present by `TransformFromCopy` may become a later replacement source.

The initial composition contract is sequential per participant. Existing Presence events for a source or target must not lie after the new animation start. Sources must be present at start; targets with Presence history must be absent at start. This rejects retroactive or overlapping ownership of one object's lifecycle while allowing deterministic chains. See [chained-lifecycle.md](chained-lifecycle.md) for the full state-machine rules.

## Evaluated handoff state

Python authoring evaluates snapshot-representable timeline state at the relevant lifecycle time instead of requiring the target object to have no prior narrow-property animation.

For `ReplacementTransform`, target `Position`, `Rotation`, and `Opacity` use their runtime-equivalent values at the exact handoff. Completed generic Transforms are also reflected in the snapshot. A generic Transform that is still active at the snapshot time remains rejected because an arbitrary in-progress path morph cannot be represented faithfully by one `ObjectSnapshot`.

The source side is stricter. Runtime narrow-property groups override generic Transform channels. Therefore a source Position, Rotation, or Opacity track that starts before the handoff would continue overriding the replacement Transform and could make the disappearing source disagree with the appearing target. `ReplacementTransform` rejects those source tracks. Tracks that begin at or after handoff are allowed because the source is already absent. `TransformFromCopy` does not have this restriction because its moving transient copy has no pre-existing narrow tracks.

Presence is handled separately by the lifecycle state machine and is not folded into `ObjectSnapshot`. Other independent channels that cannot be represented by `ObjectSnapshot`, currently `Reveal` and standalone `Morph` state, remain explicit snapshot safety boundaries.

## Current safety boundaries

Lifecycle composition remains explicit where exact semantics are not defined:

- source and target must be different objects owned by the same `Scene`;
- lifecycle operations for one participant are authored chronologically;
- a lifecycle source must be present at animation start;
- a reused lifecycle target must be absent at animation start;
- a lifecycle snapshot cannot be taken inside an active generic Transform;
- Reveal/Morph snapshot state that is not represented by `ObjectSnapshot` is rejected;
- target and TransformFromCopy-source `Position`, `Rotation`, and `Opacity` tracks are evaluated with the same latest-started-track precedence and easing rules as runtime playback;
- `ReplacementTransform` source narrow-property tracks that begin before handoff are rejected because they would override the replacement Transform.

These are authoring restrictions, not renderer fallbacks. Noon should not silently produce a discontinuous handoff.

## Validation

Coverage spans the semantic pipeline:

- core: zero-duration boolean presence validation and deterministic IDs;
- IR: human-readable presence/bool JSON round trip;
- compiler: presence dynamic classification, chain continuity, and patch validation;
- runtime: direct seek, forward playback, rewind, live-patch presence parity, and multi-handoff `A -> B -> C` chains;
- renderer: absent objects keep semantic slots but create no GPU instances; toggling presence rebuilds the prepared slot layout;
- Python: lifecycle lowering, exact-boundary chains, valid source/target reuse, absent-source and present-target rejection, chronological composition, evaluated Position/Rotation/Opacity snapshots, deterministic transient-copy identity, and transactional rollback.

## Next work

The higher-value Transform milestone after sequential lifecycle composition is matching-shape correspondence. Truly overlapping lifecycle graphs can be considered later only with explicit ownership and precedence rules rather than implicit event insertion.

# Chained lifecycle composition

## Goal

Lifecycle participation is a property of an object's timeline, not a one-shot authoring flag. An object that becomes present through one lifecycle animation may later become the source of another; an object that becomes absent may later be shown again when its Presence history makes that transition continuous.

Noon therefore derives lifecycle eligibility from the existing `Presence` tracks instead of maintaining a separate set of objects that have already been used.

## Presence state machine

For an object with no Presence tracks, runtime presence defaults to `true`. A lifecycle target is the deliberate exception at first use: its first generated event is `false -> true`, which establishes that object as absent from time zero until the handoff.

Once Presence history exists, it is authoritative. Events are ordered by `(start_time, track_id)` and adjacent events must remain continuous:

```text
previous.to == next.from
```

A normal replacement chain therefore looks like:

```text
A: true  -> false @ t1
B: false -> true  @ t1
B: true  -> false @ t2
C: false -> true  @ t2
```

At `t1`, `B` can immediately begin the next replacement because Presence evaluation is inclusive at the event timestamp. Direct seek, forward playback, and rewind all resolve the same state from the same stable object identities.

## Authoring rules

The first chained-lifecycle contract is intentionally sequential per participant.

For every lifecycle source:

- all already-authored Presence events for that object must occur at or before the new animation start;
- the object must be present at the new animation start;
- therefore no pre-existing Presence event can make the source disappear during the new interval.

For every lifecycle target:

- all already-authored Presence events for that object must occur at or before the new animation start;
- if the target already has Presence history, it must be absent at the new animation start;
- if it has no Presence history, first-use target semantics generate `false -> true` at handoff and establish initial absence.

These rules permit sequential composition while rejecting retroactive lifecycle authoring and overlapping ownership of the same presence timeline.

## Supported chains

Examples that are now valid include:

```python
scene.play(ReplacementTransform(a, b), duration=1.0, start_time=0.0)
scene.play(ReplacementTransform(b, c), duration=1.0, start_time=1.0)
```

and:

```python
scene.play(TransformFromCopy(a, b), duration=1.0, start_time=0.0)
scene.play(ReplacementTransform(b, c), duration=1.0, start_time=1.0)
```

A source that remains present may also seed multiple later `TransformFromCopy` operations. The generated transient copies remain independent stable objects with their own `false -> true -> false` Presence chains.

An object hidden by an earlier lifecycle operation may later be reused as a target. Reactivation does not reset it to its original authoring snapshot: the target snapshot is evaluated from that object's latest timeline-resolved Transform/Position/Rotation/Opacity state at the new handoff. This matches runtime seek semantics and preserves stable identity rather than silently resurrecting stale base state.

## Rejected composition

Noon rejects rather than guesses when:

- a lifecycle source is absent at the requested start;
- a previously-used lifecycle target is already present;
- an object has a Presence event after the requested new start, meaning the new operation is being inserted retroactively into an already-authored lifecycle timeline;
- the normal Transform snapshot/precedence restrictions would make source or target visual state ambiguous;
- generated Presence events would violate the compiler's continuity invariant.

`Scene.play(...)` remains transactional, so any rejection restores all objects, tracks, identity allocation, and Transform scheduling state from before the call.

## Runtime contract

No runtime feature is special-cased for chains. The existing compiled Presence groups already support multiple continuous events per object. Chained runtime tests cover exact handoffs, midpoint geometry, direct seek, forward playback, and rewind for `A -> B -> C`.

This is important architecturally: Python authoring is relaxing an unnecessary frontend restriction, not introducing a second lifecycle execution model.

## Next work

This contract remains intentionally sequential for each participant. Future work can consider genuinely overlapping lifecycle graphs only if ownership and precedence are explicit. The next higher-value Transform feature is matching-shape correspondence, which can build on stable identity, evaluated snapshots, transactional lowering, and the now-composable Presence state machine.

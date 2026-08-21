# Transform lifecycle semantics

## Status

Noon models object lifetime independently from opacity. `Presence` is a first-class, renderer-independent boolean timeline property represented as a zero-duration event. This lets lifecycle animations preserve stable `ObjectId` values while arbitrary seek, forward playback, live reconciliation, and rendering all agree on whether an object exists in the visible scene at a given time.

The lifecycle animations built on this primitive now include `ReplacementTransform`, `TransformFromCopy`, `TransformMatchingShapes`, `FadeIn`, and `FadeOut`. Transform geometry/interpolation is documented in [generic-transform.md](generic-transform.md), authoring-time state evaluation is documented in [evaluated-authoring-snapshots.md](evaluated-authoring-snapshots.md), repeated lifecycle composition is documented in [chained-lifecycle.md](chained-lifecycle.md), and Fade's independent visibility channel is documented in [fade-appearance.md](fade-appearance.md).

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

## Fade lifecycle

Fade does not overload `Style.opacity`. Noon has a separate normalized `Appearance` scalar channel whose renderer contract is:

```text
effective opacity = semantic Style.opacity * Appearance
```

This keeps authored opacity stable while Fade controls lifecycle visibility. For example, an object authored at opacity `0.4` and halfway through a fade has appearance `0.5` and packs to effective opacity `0.2`; completing a later `FadeIn` restores the rendered opacity to `0.4`, not `1.0`.

`FadeOut(object)` animates Appearance from the value evaluated at the fade start to `0.0`, then emits `Presence(true -> false)` at the exact endpoint. `FadeIn(object)` emits `Presence(false -> true)` at the exact start and animates Appearance toward `1.0`. A first-use FadeIn establishes the object's pre-animation lifecycle state with that first `false -> true` Presence event and starts Appearance from zero, so direct seek before the animation sees the object as absent.

High-level fades for one object are authored chronologically and may not overlap. A later fade starts from the Appearance value produced by the existing timeline rather than assuming a hard-coded endpoint. Fade authoring participates in the same `Scene.play(...)` transaction boundary as Transform lifecycle operations, so a failed multi-animation call restores object IDs, track IDs, and scheduler state.

Appearance is independent from Transform, Position, Rotation, semantic Opacity, Reveal, and Morph. It changes only packed instance opacity: it does not change GPU instance layout, path tessellation, or path-mesh cache identity. See [fade-appearance.md](fade-appearance.md) for the complete contract.

## Current safety boundaries

Lifecycle composition remains explicit where exact semantics are not defined:

- source and target must be different objects owned by the same `Scene`;
- lifecycle operations for one participant are authored chronologically;
- a lifecycle source must be present at animation start;
- a reused lifecycle target must be absent at animation start;
- high-level Fade operations for one object must not overlap;
- `FadeOut` requires a present object and a reused `FadeIn` requires an absent object at animation start;
- a lifecycle snapshot cannot be taken inside an active generic Transform;
- Reveal/Morph snapshot state that is not represented by `ObjectSnapshot` is rejected;
- target and TransformFromCopy-source `Position`, `Rotation`, and `Opacity` tracks are evaluated with the same latest-started-track precedence and easing rules as runtime playback;
- `ReplacementTransform` source narrow-property tracks that begin before handoff are rejected because they would override the replacement Transform.

These are authoring restrictions, not renderer fallbacks. Noon should not silently produce a discontinuous handoff.

## Validation

Coverage spans the semantic pipeline:

- core: zero-duration boolean presence validation, distinct scalar Appearance, and deterministic IDs;
- IR: human-readable presence/bool and appearance/scalar JSON round trips through the normal serde document path;
- compiler: presence dynamic classification, chain continuity, patch validation, and independent Appearance dynamic classification;
- runtime: direct seek, forward playback, rewind, live-patch presence parity, multi-handoff `A -> B -> C` chains, normalized Appearance evaluation, and semantic-opacity independence;
- renderer: absent objects keep semantic slots but create no GPU instances; toggling presence rebuilds the prepared slot layout; Appearance multiplies packed opacity without changing geometry/cache identity;
- Python: lifecycle lowering, exact-boundary chains, matching-shape replacement, FadeIn/FadeOut lowering, fade chaining and overlap rejection, valid source/target reuse, absent-source and present-target rejection, chronological composition, evaluated Position/Rotation/Opacity snapshots, deterministic transient-copy identity, and transactional rollback.

## Next work

The core lifecycle family now covers direct replacement, copy-based replacement, deterministic matching-shape replacement, and Fade visibility semantics. Further lifecycle expansion should focus on richer composition only when ownership and precedence can remain explicit—for example simultaneous group transitions or staggered appearance—rather than adding ad-hoc renderer behavior.

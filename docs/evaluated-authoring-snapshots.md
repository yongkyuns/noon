# Evaluated authoring snapshots

## Motivation

A generic `Transform` stores explicit `from` and `to` `ObjectSnapshot` values. Those snapshots must match the semantic object state at the time the Transform begins or a lifecycle handoff occurs.

Using only the object's base definition or the last scheduled generic-Transform target is insufficient once ordinary narrow-property tracks exist. For example, if Position is halfway from `(0, 0)` to `(4, 0)` at `t=2`, a Transform beginning at `t=2` must start from `(2, 0)`, not the original `(0, 0)`.

## Authoring-time evaluator

Python authoring now evaluates the snapshot-representable portion of an object's timeline at a requested time.

Evaluation mirrors the runtime grouping rules:

1. start from the object's base semantic snapshot;
2. select the latest-started generic Transform at or before the requested time;
3. if that Transform is complete, use its exact target snapshot; if it is still active, reject the snapshot request;
4. apply the latest-started Position track at or before the time;
5. apply the latest-started Rotation track;
6. apply the latest-started Opacity track.

Narrow tracks use the same clamped normalized progress and easing equations as runtime playback, including `ease_in_out_cubic`. When two tracks for the same property have started, the later start wins; track ID breaks equal-time ties, matching compiled runtime ordering.

The evaluator deliberately does not attempt to replay frames. It computes the state directly at the requested time, preserving deterministic direct-seek semantics.

## Why active generic Transform is still bounded

An in-progress generic Transform cannot always be represented by a single semantic `ObjectSnapshot`.

Analytic primitive geometry could be sampled directly, but a vector-path Transform may use a compiler-prepared fixed-topology pair whose visible intermediate geometry is renderer preparation state rather than semantic endpoint geometry. Pretending that an arbitrary active path Transform has one exact `ObjectSnapshot` would therefore make Python authoring disagree with runtime/rendering semantics.

Until Transform progress itself becomes a first-class snapshot representation, Noon rejects authoring snapshots taken inside an active generic Transform. Exact completed endpoints are supported.

## Lifecycle use

`ReplacementTransform` evaluates the target at the exact handoff time. `TransformFromCopy` evaluates the source at copy start and the target at handoff. This means Position, Rotation, and Opacity animation can compose naturally with lifecycle authoring instead of being rejected merely because those tracks already exist.

Lifecycle snapshots still reject channels that `ObjectSnapshot` does not encode, currently Presence, Reveal, and standalone Morph state. This is an explicit correctness boundary: the source/copy Transform endpoint must not claim to reproduce visual state it cannot carry.

## Generic Transform use

Plain generic `Transform` also uses the evaluator for its `from` snapshot. This fixes continuity when a Transform begins after or during a narrow Position/Rotation/Opacity track. `Transform(source, VectorPath(...))` therefore preserves the actually evaluated transform/style state at its start while replacing only geometry.

Narrow-property runtime precedence remains unchanged: Position, Rotation, and Opacity groups are applied after generic Transform, so tracks that remain active after Transform start continue to override those channels during playback.

## Validation

Python regression coverage verifies:

- Position, Rotation, and Opacity are sampled into a Transform `from` snapshot;
- cubic easing matches expected midpoint state;
- the latest-started narrow track wins when narrow tracks overlap;
- completed generic Transform target state is available to later lifecycle handoffs;
- a copy operation can occur before an already-authored future Transform on its source;
- active generic Transform snapshots are rejected transactionally;
- lifecycle state outside `ObjectSnapshot`, such as Reveal, is rejected transactionally.

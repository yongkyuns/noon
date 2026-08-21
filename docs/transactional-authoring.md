# Transactional authoring semantics

## Motivation

High-level authoring operations can lower to several semantic objects and tracks. `TransformFromCopy`, for example, creates a stable transient object and then adds a Transform plus three Presence events. A validation failure late in that lowering must not leave an incomplete scene behind.

The same rule applies to `Scene.play(a, b, ...)`: animations passed in one play call represent one authoring operation. If a later animation fails, earlier animations from that call must not remain scheduled.

## Contract

`Scene.play(...)` is atomic with respect to authoring state.

Before scheduling the batch, Noon checkpoints:

- semantic objects and tracks;
- stable object and track authoring identities;
- scheduled generic-Transform target snapshots and end times;
- lifecycle participation state.

If any animation raises during validation or lowering, Noon restores that checkpoint and re-raises the original error. Existing scene state from before the play call is preserved exactly.

This includes failures caused by duplicate keys, invalid easing/timing, unsupported animation values, lifecycle conflicts, and failures that occur after a transient lifecycle object has already been authored.

## Identity guarantee

Rollback also restores ID allocation state because object and track IDs derive from the current list lengths. A failed authoring attempt therefore cannot create ID gaps or change deterministic authoring identities on a subsequent valid rerun.

## Validation

Regression tests cover two important cases:

1. a `TransformFromCopy` that creates its transient copy and then hits a duplicate Transform key; the object, tracks, identity maps, and ID allocation all roll back;
2. a multi-animation `Scene.play` where the first Transform succeeds and the second fails on a duplicate key; the first track and its scheduled target snapshot are rolled back, so a later Transform still starts from the original object snapshot.

## Scope

This transaction boundary intentionally sits at the public high-level `Scene.play` API. Lower-level single-track authoring helpers already validate before mutating their track lists. Future compound authoring APIs should either lower through `Scene.play` or adopt the same checkpoint/rollback rule.
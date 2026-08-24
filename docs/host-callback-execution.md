# Host callback execution

## Purpose

Noon supports arbitrary host-language behavior without putting Python or another host interpreter on the normal frame path. Host callbacks are an explicit third execution class beside static/timeline work and native reactive dependencies.

The runtime protocol is frame/transaction oriented:

```text
runtime frame
    |
    +-- evaluate timeline/reactive state
    |
    v
coherent callback frame
    |
    +-- time / signed delta time
    +-- deduplicated watched-object table
    +-- callback invocations referencing that table
    |
    v
host callback phase
    |
    +-- arbitrary Python/JS/native code
    |
    v
one MutationTransaction
    |
    v
atomic runtime commit
    |
    +-- property work: incremental in-place path
    +-- timeline/structure: staged commit when safe
    +-- reactive re-lowering required rather than stale bindings
    |
    v
render
```

The callback implementation itself is not serialized into Noon. `HostCallbackId` is a stable language-neutral handle used to associate a semantic callback slot with a host-owned callable.

## Callback slots

`HostCallbackRegistry` declares which callback slots participate and which semantic objects each slot needs to observe.

A callback may watch zero or more objects. Empty object lists are useful for time/input-only handlers. Duplicate object subscriptions inside one slot are removed while preserving order.

The runtime validates every watched object when `HostDrivenScene` is built and lowers semantic object IDs once. Unknown objects fail before playback.

## Coherent frame snapshot

`HostDrivenScene::callback_frame()` captures one phase-wide object table. If several callbacks watch the same object, its dynamic state appears only once. `HostCallbackInvocation` entries reference this table by compact indices.

The frame-critical snapshot contains:

- object ID;
- transform;
- style;
- presence;
- appearance;
- reveal;
- morph.

Geometry is deliberately excluded. Large path geometry must not be cloned every frame just because an updater reads position/style. Stable geometry metadata or bounds can be exposed separately when the host API requires them.

`delta_time` is signed. Normal forward playback produces positive updater `dt`; seeks/reverse control remain deterministic instead of fabricating elapsed time.

## Mutation commit

Host code returns one existing `MutationTransaction` for the callback phase.

### Property-only transactions

`SetTransform` and `SetStyle` transactions are preflighted as a batch, then applied through the existing incremental runtime path. They do not clone the complete scene. A later invalid object causes the whole transaction to fail before any mutation is visible.

Changed objects flow through normal `FrameChanges`, so renderer preparation remains localized.

### Timeline/structural transactions

For non-reactive runtime instances, higher-impact transactions currently stage a cloned runtime and swap only after all patches succeed. This provides correct atomic behavior while specialized incremental paths are developed.

For runtime instances carrying native reactive state, timeline/structural host mutations currently return `ReactiveReloweringRequired`. Such changes can invalidate object indices, driver ownership, or reactive bindings; keeping old dense targets would be incorrect. A later semantic re-lowering transaction will close this gap.

## Performance contract

Host callbacks must not turn unrelated content into host-dynamic work.

The runtime lowers callback watched-object sets once, snapshots only their union, and applies property mutation batches only to affected dense objects. CI includes a large static-scene regression demonstrating that one callback property mutation reports only the single changed object.

The language bridge should cross once per callback phase rather than once per getter/setter. Python wrappers should read from the coherent callback snapshot locally, record mutations locally, and submit one transaction at phase end.

## Next slices

1. Serialize/transport host callback slot declarations with the semantic scene.
2. Expose a browser callback-phase request/commit API around `ReactiveCanvasPlayer`/`ReactiveScenePlayer`.
3. Add Python `Mobject.add_updater`, `remove_updater`, and `clear_updaters` using snapshot-backed proxy reads and batched mutation recording.
4. Add native pointer/keyboard/viewport signals so common interaction can bypass host callbacks.
5. Implement semantic reactive re-lowering for structural/timeline callback transactions.
6. Add `always_redraw` and native-lowering recognition for common updater patterns where semantics are known.

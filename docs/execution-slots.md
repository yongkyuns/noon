# Stable execution slots and local mutation

Noon now distinguishes three identities during the architecture migration:

- semantic identity (`SemanticNodeId`) belongs to the retained authoring/family graph;
- execution identity (`ExecutionSlotId`) is a generational runtime handle that survives unrelated insertion/removal;
- compiled/frame object slots are a stable compatibility view for the existing renderer while consumers migrate to execution identity.

`ExecutionSlotTable` uses tombstones plus a free list. Removing an object invalidates only its slot generation; unrelated slot IDs are unchanged. `SlottedSceneInstance` wraps the current `SceneInstance` and emits an `ExecutionDelta` that names the affected stable slots and timeline channels plus coarse structure/render/resource/hierarchy effects. This gives renderer/reactive/browser migrations a stable local impact contract before they stop consuming compiled/frame slot positions.

Structural and timeline `MutationTransaction`s no longer clone `SceneDefinition`. They build lightweight object/track identity metadata, validate the complete mutation sequence, then commit the already-preflighted patches. Heavy geometry and scene payloads therefore stay out of transaction staging.

Compiled timeline edits avoid cloning the existing `CompiledTrack` payload vector. `CompiledScene` keeps a stable track-ID locator index, validates only affected presence chains, inserts/removes tracks in sorted order, and recomputes dynamic-property flags only for objects whose channels changed. `CompiledPatchStats` makes those locality properties executable and separately reports residual dense-vector slot movement.

Runtime timeline edits are also channel-local. `CompiledChannelKey` identifies an `(object, property)` channel without exposing dense track positions. `SceneInstance` stores channel-addressed groups, and the event-driven scheduler keeps stable group slots plus a mutable event index so add/replace/remove-track operations relower only their old/new channels. The affected object is rebuilt from its compiled base and its channels are replayed in semantic property order before native reactive bindings are reapplied. Ordinary timeline edits therefore avoid both a full runtime-group rebuild and a full-scene seek.

Structural object edits are local as well. `CompiledScene` now treats object-vector positions as stable append-only slots: removing an object tombstones its slot instead of shifting every later object and rewriting surviving track targets. Removal drains only that object's contiguous track range, removes only its track locators and runtime scheduler channels, marks one frame slot absent, and leaves every unrelated compiled/frame slot unchanged. Creation appends one compiled/frame slot and initializes only that slot. Neither operation performs a full runtime-group rebuild or full-scene seek.

New structural objects deliberately append instead of reusing retired middle compiled/frame slots. Reusing a middle position would silently change source/painter order for the compatibility renderer. Retired-slot reclamation therefore belongs behind an explicit generation- and order-aware compaction boundary rather than the live mutation fast path.

The regression suite exercises 100,000 execution slots, stale-generation rejection, atomic generation-exhaustion failure, stable compiled object removal, single-channel timeline deltas, a 10,000-track local compiler edit, cross-object track replacement, a 100,000-channel scheduler relower, a 10,000-channel live runtime timeline edit, and a 100,000-object structural live-edit case. The structural case requires zero unrelated object-index rewrites, zero surviving track-target rewrites, zero full group rebuilds, and zero full seeks while matching a fresh compile semantically.

## Remaining #58 migration

This work intentionally does not pretend every dense compatibility structure is gone. Track insertion/removal can still shift later entries in `CompiledScene`'s dense track `Vec`, and retired object slots retain append-only capacity until a generation/order-aware compaction boundary is introduced. Renderer and reactive consumers also still need to consume stable `ExecutionDelta`/slot identities directly instead of relying on compiled/frame positions.

Follow-up #58 work should move track storage off the remaining dense-shift path, define safe retired-slot reclamation/compaction, and propagate `ExecutionDelta` directly into renderer/reactive/browser consumers. The event-driven scheduler from #52 remains the timeline algorithm; local relowering updates only its affected channel/event index rather than replacing it.
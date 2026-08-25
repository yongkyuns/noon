# Stable execution slots and local mutation

Noon now distinguishes three identities during the architecture migration:

- semantic identity (`SemanticNodeId`) belongs to the retained authoring/family graph;
- execution identity (`ExecutionSlotId`) is a generational runtime handle that survives unrelated insertion/removal;
- dense object indices remain a temporary compatibility view for the existing compiled frame and renderer.

`ExecutionSlotTable` uses tombstones plus a free list. Removing an object invalidates only its slot generation; unrelated slot IDs are unchanged. `SlottedSceneInstance` wraps the current dense `SceneInstance` and emits an `ExecutionDelta` that names the affected stable slots and timeline channels plus coarse structure/render/resource/hierarchy effects. This gives renderer/reactive/browser migrations a stable local impact contract before they stop consuming dense indices.

Structural and timeline `MutationTransaction`s no longer clone `SceneDefinition`. They build lightweight object/track identity metadata, validate the complete mutation sequence, then commit the already-preflighted patches. Heavy geometry and scene payloads therefore stay out of transaction staging.

Compiled timeline edits now also avoid cloning the existing `CompiledTrack` payload vector. `CompiledScene` keeps a stable track-ID locator index, validates only affected presence chains, inserts/removes tracks in sorted order, and recomputes dynamic-property flags only for objects whose channels changed. `CompiledPatchStats` makes those locality properties executable and separately reports residual dense-vector slot movement.

The regression suite exercises 100,000 execution slots, stale-generation rejection, atomic generation-exhaustion failure, dense compatibility removal, single-channel timeline deltas, a 10,000-track local compiler edit, cross-object track replacement, and local presence-chain rejection. These are architecture invariants rather than performance claims about the still-dense compatibility renderer.

## Remaining #58 migration

This work intentionally does not pretend the dense compatibility view is gone. Track insertion/removal can still shift later entries in the compiler's dense `Vec`, and `SceneInstance` still rebuilds all runtime track groups plus the timeline event scheduler after a timeline edit. Follow-up #58 work should make compiled/runtime storage slot- or channel-addressed, relower only affected timeline groups, and let renderer/reactive consumers ingest `ExecutionDelta` directly. The event-driven scheduler from #52 remains the timeline algorithm; local relowering should update its affected groups/index rather than replace it.

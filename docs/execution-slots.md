# Stable execution slots and local mutation

Noon now distinguishes three identities during the architecture migration:

- semantic identity (`SemanticNodeId`) belongs to the retained authoring/family graph;
- execution identity (`ExecutionSlotId`) is a generational runtime handle that survives unrelated insertion/removal;
- dense object indices remain a temporary compatibility view for the existing compiled frame and renderer.

`ExecutionSlotTable` uses tombstones plus a free list. Removing an object invalidates only its slot generation; unrelated slot IDs are unchanged. `SlottedSceneInstance` wraps the current dense `SceneInstance` and emits an `ExecutionDelta` that names the affected stable slots and timeline channels plus coarse structure/render/resource/hierarchy effects. This gives renderer/reactive/browser migrations a stable local impact contract before they stop consuming dense indices.

Structural and timeline `MutationTransaction`s no longer clone `SceneDefinition`. They build lightweight object/track identity metadata, validate the complete mutation sequence, then commit the already-preflighted patches. Heavy geometry and scene payloads therefore stay out of transaction staging.

The regression suite exercises 100,000 execution slots, stale-generation rejection, atomic generation-exhaustion failure, dense compatibility removal, and single-channel timeline deltas. These are architecture invariants rather than performance claims about the still-dense compatibility renderer.

## Remaining #58 migration

This slice intentionally does not pretend the dense compatibility view is gone. Follow-up #58 work should make compiled/runtime storage itself slot-addressed, relower only affected timeline groups, and let renderer/reactive consumers ingest `ExecutionDelta` directly. The event-driven scheduler from #52 remains the timeline algorithm; local relowering should update its affected groups/index rather than replace it.

## Slot-addressed compiled storage

`CompiledScene` object indices are now stable slot addresses. Removal tombstones a slot and removes only tracks targeting that slot; later object indices are never decremented or rebuilt. Creation reuses free slots before extending slot capacity. `FrameObjectState.live` carries the tombstone boundary into runtime/render preparation, and browser object-count metrics report live objects rather than slot capacity.

The next #58 slice localizes timeline channel relowering and scheduler event updates; the temporary full group/scheduler rebuild in `SceneInstance::apply_patch` is intentionally left visible until that change is validated independently.

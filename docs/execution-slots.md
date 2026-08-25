# Stable execution slots and local mutation

Noon now distinguishes three identities during the architecture migration:

- semantic identity (`SemanticNodeId`) belongs to the retained authoring/family graph;
- execution identity (`ExecutionSlotId`) is a generational runtime handle that survives unrelated insertion/removal;
- dense object indices remain a temporary compatibility view for the existing compiled frame and renderer.

`ExecutionSlotTable` uses tombstones plus a free list. Removing an object invalidates only its slot generation; unrelated slot IDs are unchanged. `SlottedSceneInstance` wraps the current dense `SceneInstance` and emits an `ExecutionDelta` that names the affected stable slots and timeline channels plus coarse structure/render/resource/hierarchy effects. This gives renderer/reactive/browser migrations a stable local impact contract before they stop consuming dense indices.

Structural and timeline `MutationTransaction`s no longer clone `SceneDefinition`. They build lightweight object/track identity metadata, validate the complete mutation sequence, then commit the already-preflighted patches. Heavy geometry and scene payloads therefore stay out of transaction staging.

Compiled timeline edits avoid cloning the existing `CompiledTrack` payload vector. `CompiledScene` keeps a stable track-ID locator index, validates only affected presence chains, inserts/removes tracks in sorted order, and recomputes dynamic-property flags only for objects whose channels changed. `CompiledPatchStats` makes those locality properties executable and separately reports residual dense-vector slot movement.

Runtime timeline edits are also channel-local. `CompiledChannelKey` identifies an `(object, property)` channel without exposing dense track positions. `SceneInstance` stores channel-addressed groups, and the event-driven scheduler keeps stable group slots plus a mutable event index so add/replace/remove-track operations relower only their old/new channels. The affected object is rebuilt from its compiled base and its channels are replayed in semantic property order before native reactive bindings are reapplied. Ordinary timeline edits therefore avoid both a full runtime-group rebuild and a full-scene seek.

The regression suite exercises 100,000 execution slots, stale-generation rejection, atomic generation-exhaustion failure, dense compatibility removal, single-channel timeline deltas, a 10,000-track local compiler edit, cross-object track replacement, a 100,000-channel scheduler relower, and a 10,000-channel live runtime edit that must report one channel relowered, one object recomputed, zero full group rebuilds, and zero full seeks while matching a fresh compile.

## Remaining #58 migration

This work intentionally does not pretend the dense compatibility view is gone. Track insertion/removal can still shift later entries in `CompiledScene`'s dense track `Vec`, and structural object creation/removal still falls back to rebuilding dense compiled/runtime object-indexed state. Renderer and reactive consumers also still need to consume stable `ExecutionDelta`/slot identities directly instead of relying on dense object positions.

Follow-up #58 work should move compiled object/track storage to stable slot- or channel-addressed structures, make structural edits local, and propagate `ExecutionDelta` directly into renderer/reactive/browser consumers. The event-driven scheduler from #52 remains the timeline algorithm; the local relowering work here updates its affected channel/event index rather than replacing it.

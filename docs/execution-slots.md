# Stable execution slots and local mutation

Noon now distinguishes three identities during the architecture migration:

- semantic identity (`SemanticNodeId`) belongs to the retained authoring/family graph;
- execution identity (`ExecutionSlotId`) is a generational runtime handle that survives unrelated insertion/removal;
- compiled/frame object slots are a stable compatibility view for the existing renderer while consumers migrate to execution identity.

`ExecutionSlotTable` uses tombstones plus a free list. Removing an object invalidates only its slot generation; unrelated slot IDs are unchanged. `SlottedSceneInstance` wraps the current `SceneInstance` and emits an `ExecutionDelta` that names the affected stable slots and timeline channels plus coarse structure/render/resource/hierarchy effects. This gives renderer/reactive/browser migrations a stable local impact contract before they stop consuming compiled/frame slot positions.

Structural and timeline `MutationTransaction`s no longer clone `SceneDefinition`. They build lightweight object/track identity metadata, validate the complete mutation sequence, then commit the already-preflighted patches. Heavy geometry and scene payloads therefore stay out of transaction staging.

Compiled timeline edits are channel-addressed. `CompiledScene` stores each `(object slot, property)` channel in its own sorted track vector behind `CompiledChannelKey`, with a stable track-ID locator index. Add/replace/remove therefore move entries only inside affected channels; unrelated track payloads do not relocate. Presence validation and dynamic-property recomputation also inspect only affected object channels. `CompiledPatchStats::unrelated_track_slots_shifted` is the deterministic zero-work contract for this boundary, while `dense_track_slots_shifted` now reports only movement inside an affected channel.

Runtime timeline edits are also channel-local. `SceneInstance` stores channel-addressed groups, and the event-driven scheduler keeps stable group slots plus a mutable event index so add/replace/remove-track operations relower only their old/new channels. The affected object is rebuilt from its compiled base and its channels are replayed in semantic property order before native reactive bindings are reapplied. Ordinary timeline edits therefore avoid both a full runtime-group rebuild and a full-scene seek. Initial scheduler lowering now consumes compiled channels directly instead of requiring a flattened cloned track vector.

Structural object edits are local as well. `CompiledScene` treats object-vector positions as stable append-only slots: removing an object tombstones its slot instead of shifting every later object and rewriting surviving track targets. Removal deletes only that object's channel entries and track locators, relowers only its runtime scheduler channels, marks one frame slot absent, and leaves every unrelated compiled/frame slot unchanged. Creation appends one compiled/frame slot and initializes only that slot. Neither operation performs a full runtime-group rebuild or full-scene seek.

New structural objects deliberately append instead of reusing retired middle compiled/frame slots. Reusing a middle position would silently change source/painter order for the compatibility renderer. Retired-slot reclamation therefore belongs behind an explicit generation- and order-aware compaction boundary rather than the live mutation fast path.

The regression suite exercises 100,000 execution slots, stale-generation rejection, atomic generation-exhaustion failure, stable compiled object removal, single-channel timeline deltas, a 10,000-track local compiler edit, cross-object track replacement, a 100,000-channel scheduler relower, a 10,000-channel live runtime timeline edit, and a 100,000-object structural live-edit case. A separate 10,000-track storage regression records the address of an unrelated compiled track payload across a local channel edit and requires that address to remain unchanged. Structural edits require zero unrelated object-index rewrites, zero surviving track-target rewrites, zero full group rebuilds, and zero full seeks while matching a fresh compile semantically.

## Remaining #58 migration

The normal compiler/runtime mutation path no longer depends on dense object renumbering, global track-vector movement, full runtime-group rebuilds, or full-scene seeks. Two compatibility boundaries remain before #58 is fully exhausted: retired object slots retain append-only capacity until a generation/order-aware compaction policy is defined, and renderer/browser consumers still need to consume stable `ExecutionDelta`/slot identities directly instead of conservatively rebuilding when structural frame shape changes.

Follow-up #58 work should define safe retired-slot reclamation/compaction and propagate `ExecutionDelta` directly into renderer/browser consumers. The event-driven scheduler from #52 remains the timeline algorithm; channel-addressed storage feeds its existing local relowering path rather than replacing it.

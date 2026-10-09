# Existing-attachment value publication (M1)

This slice extends the existing `ExecutionPatch` and prepared authored-value lane.
`SemanticMutation::UpdateEffect` exposes the final sparse staged definition to the
compiler; `SetGlow` carries the same generational attachment identity into the
existing runtime row. No independent scene, clock, scheduling policy, renderer,
or host-owned parameter state is introduced.

Only color, intensity, and same-unit radius edits on an already admitted attachment
use the value lane. Attachment enrollment/removal use the distinct structural
projection below; source-mode and radius-unit changes on an existing attachment
remain outside the value contract. Detached effect-bearing owners entering through
a structural publication carry their final staged effect column.

The semantic layer retains its established rejection of duplicate writes to the
same parameter in one transaction. Distinct parameter writes combine. The lower
execution transaction uses the existing final-value lane coalescing; every input
is validated before a superseded value can be removed.

Active ordinary channels retain their driven values while other base parameters
can change. Removing a channel reveals the new base. Ordinary completion
reconciliation keeps endpoint/overlap checks and requires the same attachment
generation. Prepared reapplication uses the canonical glow-channel evaluator.
Any copy-on-write for the effective value happens during preparation, before the
semantic point of no return. Glow-only commits dirty rendering, not spatial
geometry, layout, or picking.

## Evidence and scope

`noon-runtime/tests/glow_live.rs` tests value preparation/abort, rollback,
independent lane coalescing, stale context, exact no-op, active/reconciled channels,
and one affected row among 4,096 unrelated rows. The semantic bridge tests exercise
prepared projection and unchanged duplicate-write rejection, not an enabled Scene.

The canonical native raster runner includes `live-value-publication`, a distinct
25-frame test of live intensity/color/radius changes, offscreen and silhouette
sources, neutral output, restored base values, and retained intensity-only reuse.
It uses the existing full-image Gaussian/composition reference and unchanged
two-byte output tolerance. It is not physical performance evidence.

Public effect-target activation/completion orchestration and full Scene lifecycle
qualification remain unfinished. Automatic host preparation and retained worker
transport are implemented and separately qualified; those checks do not qualify
public Rust/Python `Scene.play` with glow.

## Structural execution projection (not public Scene admission)

`ExecutionPatch::SetGlowAttachment` is the structural companion to the existing
`SetGlow` value lane. Its expected attachment must match the installed semantic
generation; a replacement uses a different semantic identity. Removal clears the
optional glow column and retires only its three ordinary parameter channels,
including their compiled track locators and runtime scheduler entries. It neither
removes/recreates the source object nor changes motion, painter order, geometry,
semantic bounds or picking. The absent-to-absent case is idle; stale removal is
an error, even when the desired column is already absent.

The existing sparse transaction preflight validates topology against earlier
staged edits, including tracks moved to other objects. Topology is a coalescing
barrier, so a value write cannot cross into a new generation. The existing finite
replay revision saves the one affected row plus retired glow channels/tracks and
restores them through the ordinary scheduler on rewind. No second attachment
allocator, semantic scene, replay engine or renderer path is introduced.

`crates/noon-runtime/tests/glow_attachment.rs` covers this typed compiler/runtime
boundary: removal during animation, new-generation attachment, stale rejection,
atomic failure, coalescing, staged and moved tracks, bounded 4,097-row locality,
and repeated forward/rewind restoration. These are **not** public Rust/Python
Scene, browser-pixel or physical-GPU results. The prepared semantic bridge below
now supplies this structural operation. Public Scene bootstrap and direct live
attachment creation remain guarded pending their separate qualification.


## Prepared semantic attachment publication

The existing `prepare_semantic_publication` path reads final staged attachment
snapshots and emits at most one `SetGlowAttachment` for each affected resident
owner. The expected generation comes from the committed semantic scene; the next
generation is reserved by that same scene's prepared transaction allocator. No
new identity allocator or effect mutation protocol is introduced. Unchanged
identity/definition is idle; same-generation parameter changes still use `SetGlow`.

Newly reachable owners receive the column in their ordinary `CreateObject` patch,
including objects and attachments both created in the same transaction. Detached
owners remain inert until enrollment. Removing an attachment does not create an
object-exit patch; removing the owner uses the existing object retirement path.
Only touched owners and their own attachment lists are read. Unrelated scene
objects, families, resources and animation channels are not traversed.

The compiler validates the final staged source/profile, not an intermediate
mutation order or stale committed style. An unsupported stack, stroked/unfilled
source, or unsupported spatial/geometry state fails before publication. Removing
the attachment and making the source unfilled in the same transaction is valid.
The semantic transaction's existing rules are unchanged: removals form a suffix,
so replacement declares the new attachment before retiring the old one, and a
write targeting a node retired by the same transaction still rejects atomically.

`noon-runtime/tests/glow_semantic_publication.rs` exercises the existing prepared
semantic/compiler/runtime boundaries: preparation abort, pending IDs, replacement,
detached re-entry, alias membership, cancellation, source retirement, final-state
profile validation, stale runtime rejection, bounded dirty rows and replay of the
retired drivers. These are boundary tests, not an alternative product session.
The raw public publication validator still rejects new attachment declarations;
initial Scene execution still retains its effect-admission guard. This bridge
alone does not claim end-to-end Rust/Python Scene playback or new GPU pixels.

# Existing-attachment value publication (M1)

This slice extends the existing `ExecutionPatch` and prepared authored-value lane.
`SemanticMutation::UpdateEffect` exposes the final sparse staged definition to the
compiler; `SetGlow` carries the same generational attachment identity into the
existing runtime row. No independent scene, clock, scheduling policy, renderer,
or host-owned parameter state is introduced.

Only color, intensity, and same-unit radius edits on an already admitted attachment
are accepted. Attachment enrollment/removal, source-mode changes, and radius-unit
changes remain outside this value contract. Detached effect-bearing owners cannot
enter via a structural publication that silently strips their effect column.

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

Public effect-target activation/completion orchestration, live topology, automatic
host preparation, and worker transport remain guarded/unfinished. Passing this
slice does not enable or qualify public Rust/Python `Scene.play` with glow.

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
Scene, browser-pixel or physical-GPU results. Semantic publication of live
attachment topology and public Scene admission remain guarded pending their
separate integration and qualification.

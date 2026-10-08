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

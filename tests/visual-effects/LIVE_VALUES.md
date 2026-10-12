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
prepared projection and unchanged duplicate-write rejection. Public Scene integration
is tested separately below.

The canonical native raster runner includes `live-value-publication`, a distinct
25-frame test of live intensity/color/radius changes, offscreen and silhouette
sources, neutral output, restored base values, and retained intensity-only reuse.
It uses the existing full-image Gaussian/composition reference and unchanged
two-byte output tolerance. It is not physical performance evidence.

Automatic host preparation and retained worker transport are separately qualified.
Their lower-level fixtures do not replace public Scene playback checks or
actual Python/browser execution of the paired example.

## Structural execution projection

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
supplies this structural operation to ordinary public Scene/LiveSession calls.


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

## Public Scene playback and finite admission

Initial Scene lowering and ordinary LiveSession publication now admit one glow on
an eligible filled, unstroked analytic circle or rectangle. Unsupported reachable
profiles/stacks reject through the same compiler validation; unsupported detached
targets remain inert until enrollment. Removing blanket store-wide gates does not
weaken the source-profile validator or the existing pending-segment/callback barrier.

`LiveSession::target_editor` stages both the object and an independent attachment
in the same semantic transaction, with allocator-resolved target identity. It
captures a resident source's effective glow by exact generation rather than losing
appearance or copying an obsolete authored base. A detached source copies its
own authored attachments. No frontend mirror or effect-specific target allocator
is added. Ordinary target activation, interpolation, completion reconciliation,
live removal and finite replay use their existing shared owners.

`noon/src/effect_authoring/playback_tests.rs` exercises the real public APIs for
bootstrap, motion plus glow, independent live targets, completion, live creation/
removal, stale handles, unsupported-profile rollback, ordinary publication
barriers, reversed/returning easing, and repeated replay after source glow removal.
`noon-web` adapter tests verify the same retained player identity and its actual
encoded glow/removal publications; these native Rust tests are not a Python run.

`noon/tests/glow_scene_raster.rs` uses public Scenes for both the effect and a
separate no-effect source, then the production retained renderer. Its independent
full 2D Gaussian uses only the ordinary white source's coverage and authored
palette endpoints. Five sampled animation frames retain the frozen maximum
2-byte full-image allowance and a visible-halo negative control. Clean-frame,
neutral, removal, and new-generation reattachment comparisons are exact. This
central-source test complements rather than replaces the existing offscreen,
silhouette, painter-order and real-worker fixtures.

The equivalent `glow_scene_playback.rs` and `glow_scene_playback.py` examples
exercise movement with persistent appearance, completion, fade-to-neutral,
explicit removal, stale-handle rejection, and ordinary continuation. The Python
example is registered in the existing shared-authoring browser gate. Compilation
or the native adapter tests alone do not qualify that Python/browser execution.

A live `TransformTo` from a missing canonical glow to a target carrying that
attachment stages an allocator-owned zero-intensity copy at activation. It is
published together with its ordinary intensity track, never when the builder is
created. The same combined compiler/runtime preflight validates staged attachment
identity, reconcilable-channel ownership and publication before committing either
semantic or runtime state. The source object's geometry/motion identity remains
unchanged. Completing the track preserves the new attachment; returning rates
reconcile back to neutral; removal retires it, and finite runtime replay restores
its original generation without resurrecting authored handles. The public Rust
regressions cover neutral/midpoint/completion, stale-handle rejection, unsupported
stacks with unchanged revisions and repeated replay. The Python worker example
now exercises the same absent-to-glow target animation; real browser qualification
remains a separate CI result, not inferred from the Rust test.

Still unsupported: adding an absent glow via a **predeclared, immutable**
`play_animation` declaration (the supported enrollment is the live atomic
`declare_and_activate_transform_to` path), attachment schema/source-mode or
radius-unit changes during target interpolation, multiple/composed-group/view
effects, and callback-staged effect edits. Temporary `GlowPulse` lifecycle is M2. Active family
animation/inset combinations retain the renderer's explicit finite-profile rejection.
State-replacement APIs retain their separate effect-bearing-store guard. Complete
cross-language/browser/current-head and physical-GPU qualification remain required;
exact current results belong in PR #1921, not inferred from these test descriptions.

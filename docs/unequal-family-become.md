# Unequal family topology: alignment versus persistent `become()`

## Status and authority

This is a focused B1.4 compatibility/implementation note for #74 and the Phase B umbrella #954. It applies the ownership, identity, authored/effective-state, and atomic-publication rules in [`architecture.md`](architecture.md); it does **not** define a second architecture, scene model, roadmap, or compatibility oracle. If this note conflicts with `architecture.md` or an owning issue, those sources win.

Manim Community v0.21.x remains the compatibility oracle for supported common 2D Python behavior. Noon intentionally does not copy Manim's Python object graph or use its internal family-padding objects as semantic identity.

## Decision

Manim-style family alignment may duplicate, fade, or otherwise synthesize family members so unequal structures can participate in visual interpolation. In Noon those members are **derived animation/presentation correspondence**, not authored objects.

Synthetic alignment members therefore:

- are not Semantic Scene nodes;
- receive no scene-global semantic `NodeId`;
- are not independently addressable through language wrappers;
- do not change authored family membership merely because an animation needs equal interpolation cardinality;
- disappear with the derived animation/effective state that required them.

This keeps a visual matching device from becoming a second identity system.

## Operation split

| Operation | Where unequal-family correspondence lives | Persistent semantic topology |
| --- | --- | --- |
| `Transform` / family target-state interpolation | Derived execution/runtime animation correspondence | Unchanged during interpolation |
| `ReplacementTransform` | Derived correspondence during interpolation | Real source/target lifecycle change is committed atomically at completion |
| `always_redraw`-style regeneration | Effective presentation/resource state | No per-frame semantic ID or membership churn |
| Explicit persistent `Group.become()` / family `become` | Semantic Scene reconciliation | Receiver topology is persistently reconciled in one semantic transaction |

The implementation details used by Manim to achieve a visual result do not force Noon to publish synthetic children into authored state.

## Persistent identity contract

For explicit persistent unequal-topology `become()`:

1. The receiver root semantic ID survives.
2. Target descendant IDs are never transferred, stolen, or aliased into the receiver.
3. Compatible receiver descendants are reused deterministically in authoritative target/member order when one existing identity can represent that target node.
4. A target alias maps to exactly one receiver-owned identity across the reconciled family DAG.
5. If one receiver alias would need to represent two distinct target nodes, the first compatible mapping may retain the receiver identity and later distinct target nodes receive fresh receiver-owned identities.
6. Genuinely new receiver descendants use transaction-local pending references and receive permanent semantic IDs only if the complete transaction commits.
7. Removed family edges do not imply node deletion. Detached descendants remain valid when other handles or aliases still reference them.
8. Membership, ordering, object state/content, and new-node publication are one atomic semantic transaction. Any preflight or commit failure publishes none of them.
9. Frontends must observe post-`become()` children from Semantic Scene membership/order. A Python wrapper must not retain a second authoritative submobject list or invent IDs for synthesized alignment members.

These rules intentionally provide stronger and more explicit identity semantics than Manim's visual padding mechanism while preserving compatible visible behavior.

## Transform pairing remains strict

`SemanticStore::ordered_family_leaf_pairs()` continues to mean **structurally equivalent semantic-family pairing for animation/Transform preparation**. Persistent unequal-topology `become()` must use a separate reconciliation path rather than weakening that pairing function.

This separation is important: allowing a persistent mutation to reconcile authored topology does not make synthetic Transform padding authored truth.

## Atomic reconciliation shape

Conceptually, for a receiver with one child becoming a two-child target:

```text
before                     target template
receiver R                 target T
└─ A (#receiver-a)          ├─ X (#target-x)
                           └─ Y (#target-y)

one prepared semantic transaction

R keeps its root ID
├─ A' (#receiver-a)         reused receiver identity, target X state
└─ B' (#fresh)              fresh receiver-owned identity, target Y state

#fresh != #target-y
```

The target remains unchanged. If the receiver later contracts, obsolete receiver membership edges are removed atomically; the detached objects are not implicitly destroyed.

## Current bounded implementation seam

The persistent path reuses the ordinary semantic transaction's pending-node creation plus membership/order mutations. It does not reserve IDs during preflight and does not use rollback state in Python.

State fitting still uses the shared `ManimBecomeOptions` preparation. A rare unequal-topology case that both requires a **new pending receiver leaf** and requires path materialization for a non-representable rotated non-uniform stretch currently fails closed before publication; extending atomic path-resource admission to pending node refs is the bounded follow-up for that case. Equal-topology rotated replacement continues to use the existing retained-path replacement path.

## Qualification expectations

B1.4 qualification should keep separate evidence for:

- strict animation pairing versus persistent topology reconciliation;
- expansion and contraction in one scene revision;
- receiver-root and reusable-child identity preservation;
- fresh receiver IDs for added topology and non-transfer of target IDs;
- target alias preservation and source-alias splitting when the target requires it;
- failure atomicity, including foreign/stale handles and unsupported pending-path materialization;
- post-reconciliation member lookup through semantic membership/order;
- later animation work proving unequal Transform alignment remains derived/effective rather than authored topology.

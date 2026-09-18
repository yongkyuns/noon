# Phase C parallel work tracks

## Status

This document is an execution/scheduling overlay for Phase C of `docs/architecture.md` and #955.

It does **not** define a second architecture, roadmap, goal set, acceptance criterion, interaction model, or optimization policy. The architecture, Phase C cases, gates, and completion checklist remain exactly those stated in `docs/architecture.md` and the owning issues. If this document conflicts with them, the architecture and owning case win.

The purpose of this document is only to make explicit which existing interaction/locality/live-authoring cases can progress independently and where they must synchronize.

## Principle

Phase C has several producer lanes around the same Phase A mutation/runtime architecture. They should not be serialized behind one umbrella implementation.

```text
host callbacks -----> mutation contract ----+
                                           |
native input -------------------------------+--> interactive session
                                           |
spatial/local renderer ---------------------+
                                           |
content replacement / hot reload -----------+

browser startup measurement runs independently.
measured specialization starts only from evidence.
```

C1–C4 should converge through the existing shared semantic identity, mutation, execution, and locality contracts; parallel work is not permission to create callback-specific patch models, editor scene state, duplicate spatial indexes, or content-specific update protocols.

## Shared handoff contracts

### CH1 — mutation and driver contract

Owned by C1/#70 on top of the Phase A mutation vocabulary.

The handoff covers:
- coherent callback snapshots;
- one validated/batched semantic transaction;
- generation-safe mutation targeting;
- explicit driver/conflict arbitration;
- replay/seek classification;
- bounded impact/locality reporting.

C4 and C5 consume the same mutation semantics; neither gets a separate editor/content patch model.

### CH2 — common typed Rust input contract

Owned by C2/#69.

The handoff covers:
- one backend-neutral Rust ingress vocabulary shared by browser, native, embedded/platform and test/replay collectors;
- sampled latest-value state versus ordered discrete occurrences;
- occurrence-local pointer/source identity, position and button/modifier context where applicable;
- ingress sequence plus compatible scene/view publication context;
- deterministic coalescing rules that cannot rewrite already ordered occurrence context;
- runtime/reactive projection plus ordered delivery to the interaction session;
- platform capture/cancel/lost-capture normalization;
- paused-scene wake behavior.

C5 consumes this Rust input contract without requiring C2 to own hit testing, semantic selection/capture, trigger synthesis, tools, actions or session state. Platform shells may own DOM/OS pointer-capture mechanics but not the captured scene target.

### CH3 — spatial/locality contract

Owned by C3/#569/#362/#835 and the existing runtime/renderer locality machinery.

The handoff covers:
- execution-owned spatial candidates;
- painter-correct candidate order;
- generation/coherence rules;
- bounded refit/dirty preparation/upload work;
- deterministic locality counters.

C2 hit-test integration, C4 replacement, and C5 selection/manipulation can consume this contract independently.

### CH4 — localized replacement and reconciliation contract

Owned by C4/#368/#64.

The handoff covers:
- `ReplaceContent` through the shared semantic transaction;
- source/content generation safety;
- local relowering/runtime/resource install;
- rollback on failed preparation;
- stable source/semantic identity reconciliation.

C1 `always_redraw`/host replacement and C5 hot-reload session migration consume this contract.

### CH5 — measurement contract

Owned by the existing locality instrumentation plus C6/#642 measurements.

C7 specialization may begin only when a measured cost is isolated with representative counters/traces. Evidence is the handoff; architectural analogy is not.

## Parallel tracks

### Track CM — mutation and host callbacks

**Owner:** C1/#70.

**Owns:** coherent host callback phase, bounded property/content/structural transactions, driver arbitration, replay classification, and `always_redraw`-class host behavior.

**Can proceed independently from:** native input UI/session work and browser startup tuning. Content replacement integration consumes CH4 when needed rather than blocking the rest of C1.

**Primary output:** CH1.

### Track CI — native input and events

**Owner:** C2/#69.

**Owns:** the common typed Rust pointer/keyboard/viewport/control/wheel/gesture ingress, sampled native-reactive projection, and ordered interaction delivery.

**Can proceed independently from:** editor/session state, host callbacks, and hot reload. It may validate input delivery with ordinary reactive fixtures before C5 exists.

**Primary output:** CH2.

### Track CL — spatial queries and retained locality

**Owners:** C3/#569/#362/#835.

**Owns:** one execution-owned spatial index, dirty-object/member rendering, resident family realization, and locality instrumentation.

**Internal parallelism:** spatial candidate work, dirty text/mixed rendering, and family-animation residency are separate implementation subtracks as long as they converge on the same runtime/renderer change-set and measurement model.

**Primary output:** CH3.

### Track CR — content replacement and hot reload

**Owners:** C4/#368/#64.

**Owns:** generic generation-safe resource/content replacement and stable source-identity reconciliation.

**Can proceed independently from:** direct manipulation UI and most native input work. Generic `ReplaceContent` can be proven with semantic/runtime fixtures before editor hot reload is integrated.

**Primary output:** CH4.

### Track CS — interactive session and direct manipulation

**Owner:** C5/#846.

This is primarily an **integration/convergence track**. It consumes:
- CH2 for normalized input;
- CH3 for hit-test candidates/locality;
- CH1 for mutation/driver ownership;
- CH4 for hot-reload reference migration.

Session identity, overlay projection, and other state explicitly outside authored scene content can be developed before every integration is available. C5 also owns language-neutral Trigger synthesis and semantic InteractionBinding -> Action dispatch: Rust, Python and future language wrappers declare the same bindings, while ordinary native actions execute on the shared Rust path without host callbacks. Hit/select, click/highlight, drag, and hot-reload integration land as their specific handoffs become usable.

### Track CB — browser startup/topology measurement

**Owner:** C6/#642.

**Owns:** measuring and improving the post-Phase-A startup topology from actual traces.

**Can proceed independently from:** C1–C5 feature completion, subject to the existing requirement to measure the consolidated architecture rather than optimizing obsolete migration topology.

This lane must not redesign semantic/runtime ownership merely to improve startup aesthetics.

### Track CO — measured execution/render specialization

**Owners:** C7/#67/#847.

This is deliberately **not** an unconditional parallel feature lane. It waits for CH5 evidence showing a material cost and then runs as an isolated experiment against the stable reference behavior.

A valid outcome is adopt, reject, or defer. It must not create a duplicate semantic/runtime/renderer architecture.

## Dependency view

```text
Track CM: mutation/host -------- CH1 -----+
                                          |
Track CI: input --------------- CH2 -----+
                                          |
Track CL: locality/spatial ---- CH3 -----+--> Track CS: session/manipulation
                                          |
Track CR: replacement/reload -- CH4 -----+

Track CB: startup measurement runs beside the above.
Track CL/CB measurements -- CH5 --> Track CO only when evidence justifies it.
```

This is a synchronization graph, not a new phase order. C1–C4 remain their existing cases and may be pursued simultaneously when their Phase A prerequisites exist.

## Practical isolation rules

1. **One mutation vocabulary.** Host callbacks, editor manipulation, hot reload, and native edits all converge on `MutationTransaction` semantics.
2. **One spatial authority.** Input/session/render work consumes the execution-owned spatial index; no frontend or renderer duplicate index is added for convenience.
3. **Separate input from session policy.** C2 owns the common Rust ingress and delivery contract; C5 owns hit testing, trigger synthesis, semantic selection/capture, interaction actions, tools/undo/overlays. Platform shells own only platform capture mechanics.
4. **Separate replacement from producer.** Text edits, host callbacks, hot reload, images, and paths reuse generic content replacement rather than defining producer-specific patch systems.
5. **Use fixtures to unblock producer tracks.** C1–C4 can prove their contracts without waiting for the complete editor workflow.
6. **Instrument locality at each handoff.** A feature that is functionally correct but silently scans/rebuilds the whole scene has not satisfied the existing Phase C contract.
7. **Keep optimization evidence-driven.** C7 follows measured bottlenecks only; C6 optimizes measured post-consolidation startup only.

## Suggested integration cadence

```text
CM/CI/CL/CR land small independent contracts
        |
        +--> CS integrates one interaction path at a time
        +--> locality counters prove affected work stays bounded

CB records startup/runtime measurements continuously
        |
        +--> CO experiments only on demonstrated material costs
```

The final Phase C exit remains the existing #955 completion checklist. Parallel execution changes how work is scheduled, not the interaction semantics, locality requirements, or optimization policy.
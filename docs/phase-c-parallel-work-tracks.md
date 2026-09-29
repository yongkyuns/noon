# Phase C parallel work tracks

## Status

This document is an execution/scheduling overlay for Phase C of `docs/architecture.md` and #955.

It does **not** define a second architecture, roadmap, goal set, acceptance criterion, interaction model, or optimization policy. The architecture, Phase C cases, gates, and completion checklist remain exactly those stated in `docs/architecture.md` and the owning issues. If this document conflicts with them, the architecture and owning case win.

The purpose of this document is only to make explicit which existing interaction/locality/live-authoring cases can progress independently and where they must synchronize. The [common Rust input and interaction seam](architecture.md#common-rust-input-and-interaction-seam) defines the shared contract; the [ordered interaction/callback rules](architecture.md#9-host-callbacks-and-interaction) define its execution ordering.

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
- one coherent staged batch of effective driver writes and/or validated authored semantic transactions;
- generation-safe mutation targeting;
- explicit driver/conflict arbitration;
- replay/seek classification;
- bounded impact/locality reporting.

C4 and C5 consume the same effective-write and authored-mutation contracts; neither gets a separate editor/content patch model. Ordinary effective interaction updates do not require authored semantic transactions or relowering.

### CH2 — common typed Rust input contract

Owned by C2/#69.

The handoff covers:
- one backend-neutral Rust ingress vocabulary shared by browser, native, embedded/platform and test/replay collectors;
- sampled latest-value state versus ordered discrete occurrences;
- occurrence-local pointer/source identity, position and button/modifier context where applicable;
- ingress sequence plus compatible scene/view publication context;
- deterministic coalescing rules that cannot rewrite already ordered occurrence context;
- native-reactive projection of sampled state and subscribed ordered events, plus coherent delivery to the interaction session;
- platform capture/cancel/lost-capture normalization;
- paused-scene wake behavior.

C5 consumes this Rust input contract without requiring C2 to own hit testing, semantic selection/capture, trigger synthesis, tools, actions or session state. Platform shells may own DOM/OS pointer-capture mechanics but not the captured scene target.

These are projections of one admitted input stream, not mutually exclusive input classes or independent schedulers. Discrete events remain usable by native reactive bindings, and the session also consumes pointer samples needed for hover/drag. Delivery preserves the existing session/updater order and required callback barriers from CH1.

### CH3 — spatial/locality contract

Owned by C3/#569/#362/#835 and the existing runtime/renderer locality machinery.

The handoff covers:
- execution-owned spatial candidates;
- painter-correct candidate order;
- generation/coherence rules;
- bounded refit/dirty preparation/upload work;
- deterministic locality counters.

C4 replacement and C5 precise picking/selection/manipulation can consume this contract independently. C2 supplies normalized input; scene-target interpretation stays with C5.

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

**Owns:** the common typed Rust pointer/keyboard/viewport/control/wheel/gesture ingress, sampled/event native-reactive projection, and coherent interaction delivery.

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

Session identity, overlay projection, and other state explicitly outside authored scene content can be developed before every integration is available. C5 also owns language-neutral trigger synthesis and semantic interaction-binding/action dispatch: Rust, Python and future language wrappers declare the same bindings, while ordinary native actions execute on the shared Rust path without host callbacks. Hit/select, click/highlight, drag, and hot-reload integration land as their specific handoffs become usable.

The names `InputIngress`, `Trigger`, `InteractionBinding`, `Action`, and `InteractiveSession` identify shared Rust responsibilities, not mandatory new public types, crates, or a second scheduler. Action execution reuses existing session, signal, driver, animation, callback and authored-mutation operations. Qualification and the concrete click/highlight integration sequence belong in #69 and #846.

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

1. **One authored mutation vocabulary.** Persistent host/editor/hot-reload/native changes converge on the shared semantic transaction contract. Effective-only interaction writes use the existing driver/publication path without authored mutation or relowering.
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

## Phase C evidence ledger

Snapshot: 2026-09-29. This ledger points to executable evidence and named gaps; it does not change the roadmap or close an owning issue. Unit tests, focused smokes and renderer-specific fixtures prove only the contract they exercise.

| #955 area | Existing evidence | Remaining acceptance gap |
| --- | --- | --- |
| **C1 — callbacks and staged publication** | `crates/noon/src/host_callbacks.rs` (`callbacks_share_ordered_overlay_and_accumulate_from_prior_effective_frame`); `crates/noon/src/execution_session/callback.rs` (ordered phase and transaction-local provisional-node tests); `web/python/test_noon_callback_errors_wasm.py` (`test_typed_callback_membership_stages_ordered_existing_handles_until_one_commit`); `crates/noon-web/src/semantic_execution_player.rs` (membership collector commit/retry tests); `web/semantic-engine-endpoint.test.mjs` (complete/discard lifecycle); `scripts/shared-authoring-smoke.mjs` (ordered callback continuation/pixels); `scripts/host-callback-perf.mjs` (native/host workload counters). | #70 remains open. Python callbacks now stage ordered add/remove/clear operations for **existing typed handles**, but Python provisional object construction is not qualified. Ordered mixed native/host execution remains follow-up, as do all slow-callback/read-miss/backpressure and stale-resource cases. Do not treat Rust transaction-local provisional tests or existing-handle membership as Python provisional construction. |
| **C2 — native input/events** | Native normalization/lifecycle cases in `crates/noon-native/src/pointer_input/tests.rs`, including `normalized_press_move_release_and_cancel_match_at_one_and_two_device_scale`, `repeated_edges_and_nonpointer_events_share_sequence_without_coalescing`, `pending_callback_rejects_bursts_without_acknowledging_or_overwriting_position`, and `paused_move_press_and_release_use_one_coherent_session_path`; DOM collector DPR 1/2 cases in `scripts/browser-pointer-input-smoke.mjs`; direct Rust/WASM interaction across WebGPU/WebGL and DPR 1/2 in `scripts/pointer-selection-direct-qualification.mjs`; worker/browser pixel path in `scripts/pointer-selection-qualification.mjs`. Bounded browser lanes are covered by `web/browser-pointer-input.test.mjs` (64 in-flight samples) and `web/semantic-engine-endpoint.test.mjs` (128 queued semantic controls); time-domain/replay classification is covered by `semantic_execution_player.rs::browser_input_classifies_unrecorded_replay_without_blocking_first_execution` and `semantic-engine-endpoint.test.mjs` (renderer timestamps do not become playback time). The WebKit DPR2 touch drag path is `scripts/playground-gallery-selection-smoke.mjs`. | #69 remains open. These tests cover normalization, DPI, bounded admission, paused wake and classification as separate contracts; they do not provide the complete paired native/browser trace through one Rust-authored scene, native reactive event subscriptions and C5 interaction/session policy, including all lifecycle outcomes. DPR qualification alone does not close C2. |
| **C3 — retained locality/spatial queries** | `crates/noon-runtime/tests/static_frame_locality.rs` proves 100k static objects do zero timeline work on an unchanged frame; `geometry_patch_locality.rs` and `property_patch_locality.rs` prove one-object changes in a 100k scene; renderer residency/dirty upload cases are in `crates/noon-render-wgpu/tests/retained_static_frame_locality.rs` and `src/gpu/mod.rs`. `web/direct-execution-smoke-probe.js` runs a real direct-WASM 100k sparse-locality scene during `scripts/browser-smoke.mjs` and asserts the normal-playback single-target upload remains at most 256 bytes. The 88-byte scope is independently exact: packed circle/rectangle/line instances are 88 bytes in `crates/noon-render-wgpu/src/lib.rs`; `src/gpu/mod.rs` verifies a dirty circle upload equals one `CircleInstance` (88 bytes), and `tests/line_instance_vertex_abi.rs` pins the line stride to 88 bytes. | #569/#362/#835 remain open. The 100k tests establish selected runtime locality and the direct-WASM smoke bounds one normal-playback update; the 88-byte assertion is for a one-circle renderer fixture, not a universal per-scene or all-shapes update budget. The full 100k acceptance still needs same-index interaction/viewport candidate, stale-context and platform/backend coverage across the specified workload shapes and long churn. |
| **C4 — content replacement/hot reload** | Shared `ReplaceContent` publication and rollback coverage is in `crates/noon-compile/src/semantic_lowering/publication.rs` and `crates/noon-runtime/tests/live_patch.rs`. Source restart/Run lifecycle contracts are exercised by `web/playground-source-restart.test.mjs`, `web/playground-restart-integration.test.mjs`, and `web/playground-run-request-router.test.mjs`. | #368/#64 remain open. Passing Run/restart lifecycle tests are not stable-identity hot-reload qualification: normal Run has not been qualified as local `ReplaceContent` reconciliation with compatible live/session-state migration. For web resources, wgpu retains submitted handles through their GPU use; the remaining gap is proving bounded ownership/dependency closure and accepted transport retirement while preserving in-flight resources through wgpu, not requiring a custom completion fence before CPU semantic retirement. |
| **C5 — session/direct manipulation** | Native and browser input contracts above feed the public touch drag/cancel/Run-reset qualification in `scripts/playground-gallery-selection-smoke.mjs`; its artifact harness is `web/playground-gallery-selection-artifacts.test.mjs`. Direct interaction pixels and DPR coverage are also in `scripts/pointer-selection-direct-qualification.mjs`. | #846 remains open. These prove selected click/drag/reset paths, not the complete pure-Rust/Python/future-wrapper equivalence, undo grouping, overlapping animated hit targets, stationary hover transitions, or every driver-arbitration case. |
| **C6 — startup/topology** | `scripts/playground-cold-start.mjs` and `web/playground-cold-start-metrics.js` record page-load-to-authoring-worker creation, authoring readiness, first successful renderer `render()` timestamp (worker `performance.timeOrigin` converted to epoch), page-observation lag, warm source-edit-to-completed-run, post-run resource timing, worker roles and 250 ms sampled Chromium process-tree RSS. Artifacts from the delegated run were `/tmp/noon-c6-startup-desktop-final.json` and `/tmp/noon-c6-startup-mobile-final.json`; they used frontend `d0851975` with a renderer-smoke package from `a55a5493`, Chromium 151, and geometry/text cases. | #642 remains open. Those values are topology/telemetry qualification only, not a production startup baseline. The package used the renderer-smoke build. `load` is not a paint timestamp; successful renderer return is not physical scanout; warm edit-to-first-present is intentionally unreported because metrics lack an exact run-bound frame timestamp. Semantic Scene, runtime/device-ready phase times, cold no-preload comparison, before/after traces, WASM instance/instantiated-byte counts, and true peak memory remain unmeasured. Mobile-class is Chromium emulation, not a physical device. RSS can double-count shared pages, misses external GPU allocations, and is sampled rather than a true peak. |
| **C7 — measured specialization** | The locality and startup measurement paths above provide workload/counter entry points. | No specialization should be claimed from these proofs alone. A proposed optimization still needs an isolated representative trace, before/after evidence and equivalence checks against the stable path. |

The current evidence therefore supports selected input, locality, renderer ABI and startup telemetry contracts; it does not establish Phase C exit. In addition to the row-specific gaps, #955 still requires ordered native/host callback behavior, provisional Python structural mutations, C4 normal-Run reconciliation and browser GPU-safe resource retirement to be qualified before those areas can be claimed complete.

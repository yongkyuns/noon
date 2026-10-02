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
content replacement ------------------------+

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

### CH4 — localized replacement contract

Owned by C4/#368. Source hot reload under #64 is a separate follow-on that may consume this contract.

The handoff covers:
- `ReplaceContent` through the shared semantic transaction;
- source/content generation safety;
- local relowering/runtime/resource install;
- rollback on failed preparation;
- effective content lease/version safety and resource lifetime.

C1 `always_redraw`/host replacement consumes this contract. A future #64 implementation must define source identity and session migration separately.

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

### Track CR — content replacement

**Owner:** C4/#368.

**Owns:** generic generation-safe authored/effective resource and content replacement.

**Can proceed independently from:** direct manipulation UI, most native input work and the separate source hot-reload feature. Generic `ReplaceContent` can be proven with semantic/runtime fixtures.

**Primary output:** CH4.

### Track CS — interactive session and direct manipulation

**Owner:** C5/#846.

This is primarily an **integration/convergence track**. It consumes:
- CH2 for normalized input;
- CH3 for hit-test candidates/locality;
- CH1 for mutation/driver ownership;
- CH4 for content results used by interaction actions.

Session identity, overlay projection, and other state explicitly outside authored scene content can be developed before every integration is available. C5 also owns language-neutral trigger synthesis and semantic interaction-binding/action dispatch: Rust, Python and future language wrappers declare the same bindings, while ordinary native actions execute on the shared Rust path without host callbacks. Hit/select, click/highlight and drag land as their specific handoffs become usable. Source hot-reload migration is a separate #64 follow-on.

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
Track CR: replacement --------- CH4 -----+

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

Snapshot: 2026-10-01. The [Phase C issue](https://github.com/yongkyuns/noon/issues/955) and its child cases own acceptance. This ledger identifies the strongest executable proof and the remaining limits; detailed run artifacts and platform caveats stay with the owning issues. It does not define another roadmap or change the contracts in `docs/architecture.md`.

### C1 — callbacks and staged publication

The shared callback/session path proves ordered native–host evaluation, coherent overlays, provisional construction, effective writes without relowering, stale-result rejection, and bounded input backpressure. See `crates/noon/src/execution_session/callback/tests.rs`, `web/python/examples/ordinary_mixed_updater_order.py`, and `scripts/shared-authoring-smoke.mjs`. Python `always_redraw` gives stable targets effective geometry/transform/style snapshots from fresh callback-local Circle, Rectangle/Square, Line, or Path producers. A 600-object regression checks 16 replacements without authored revision or store growth and with one dirty row; mixed callback batches reject an invalid content row without publishing preceding effective writes. Multi-target callback output is published atomically. Sparse callback reads suspend the same invocation and resume against its pinned token; `ordinary_callback_sparse_reads.py` and worker tests check exactly-once execution and stale-read rejection. Native hosts that cannot suspend fail explicitly. Unsupported content and nonvisual declarations fail explicitly. The bounded two-target gallery lesson and physical capture are in [#1839](https://github.com/yongkyuns/noon/pull/1839). Exact-head-green [#1841](https://github.com/yongkyuns/noon/pull/1841) supports `move_to` on callback-provisional shapes, while [#1844](https://github.com/yongkyuns/noon/pull/1844) qualifies a one-row callback payload in a 600-object scene; neither measures the latency of 599 distinct read misses. [#70](https://github.com/yongkyuns/noon/issues/70) still needs broader producer content and quantitative callback working-set/latency budgets. Python circle and polygon producers use the C4 effective-content lane. Exact-head-green [#1846](https://github.com/yongkyuns/noon/pull/1846) lets `always_redraw` select among prebuilt Text resources through that same lease; it does not shape fresh Text in a callback.

### C2 — native input and events

The shared Rust input contracts cover normalized pointer edges, ordered discrete events, sampled state, paused-time behavior, overflow policy, and replay guards; browser and native host tests exercise those contracts (`crates/noon-native/src/pointer_input/tests.rs`, `crates/noon/src/execution_session/input/tests.rs`). Exact-head-green [#1843](https://github.com/yongkyuns/noon/pull/1843) adds paired native/browser viewport-rebind and stale-input replay-classification tests; accepted sequence counts remain host-specific. [#69](https://github.com/yongkyuns/noon/issues/69) remains open for broader physical-host and replay qualification rather than a new input model.

### C3 — retained locality and spatial queries

100k mostly-static Rust runtime tests show clean-frame O(0) timeline evaluation, candidate-local picking, and one-row spatial refits. The native retained-renderer API qualification moves one visible object through prepare, upload, encode, and no-op-device submit while preserving one 88-byte instance update. The physical browser smoke runs a direct Rust/WASM 100k-scene trace on real WebGPU and WebGL2: 64 local playback ticks with pointer and viewport changes, followed by 64 actual browser animation frames changing one target. Every local frame uploads at most 256 bytes; the trace records RAF intervals, advance/render-submit CPU times and draw calls without asserting a device-independent FPS budget. In one headless CI run, RAF p95 was 16.7/16.8 ms on WebGPU/WebGL2 but p99 stalled at 61/238 ms; advance CPU p95 was 0.2/0.1 ms and render-submit CPU p95 was 0.7/3.12 ms. `scripts/browser-smoke.mjs` runs this proof through `web/direct-execution-smoke-probe.js`; CI invokes the browser smoke in both backend jobs. A separate mixed scene with over 10k glyphs proves a one-object text transform remains a 96-byte upload with stable glyph residency and painter order. Exact-head-green [#1838](https://github.com/yongkyuns/noon/pull/1838) reuses retained scratch slots for mixed Text/Circle family Reveal; its native regression embeds the family in 10,000 static Circles and asserts family-sized uploads with no fresh geometry/cache work or painter-order rebuild. Its source-exact local capture shows both members at mid-progress and endpoint with pixel-exact WebGPU/WebGL2 parity. The PR awaits the required human review. These measurements do not isolate physical scanout or certify a 60 FPS device budget. See also `crates/noon-runtime/tests/static_frame_locality.rs`, `crates/noon/tests/retained_session_renderer_locality.rs`, [#362](https://github.com/yongkyuns/noon/issues/362)/[#835](https://github.com/yongkyuns/noon/issues/835), and [#569](https://github.com/yongkyuns/noon/issues/569).

### C4 — authored and effective content replacement

One semantic `ReplaceContent` path handles persistent edits; one runtime effective-content lease handles transient image, path, analytic geometry, and prepared text results with stale/failure atomicity. Explicit maintenance barriers reclaim obsolete compiled text/font/vector dependencies while retaining the active closure. The 100k resident-mesh churn test exercises the bounded retirement threshold, while image tests reject stale content deltas and a physical GPU test checks that in-flight resource versions remain alive through submission completion. See `crates/noon-runtime/src/effective_content.rs`, `crates/noon-compile/src/compaction.rs`, and `crates/noon/src/execution_session/maintenance_tests.rs`. Python circle, bounded polygon and prebuilt-text producers use the same effective-content lane. [#368](https://github.com/yongkyuns/noon/issues/368) is closed for its generic replacement contract; broader `always_redraw` producer coverage remains with C1 and renderer-specific locality remains with C3.

### C5 — interactive session and direct manipulation

Shared selection, hover, click `Indicate`, drag ownership, and frame-qualified pointer input run through the session/runtime rather than a JavaScript effect. The paired [#1800](https://github.com/yongkyuns/noon/pull/1800) executable trace proves pixel-equal static filled-shape selection and clear behavior for direct Rust/WASM and Python-worker hosts on WebGPU/WebGL2. The same harness checks selection during authored motion, highlight follow-through, clearing, and a click where the moving circle overlaps the front rectangle; both hosts select that visible front target without an authored edit. A paired DOM drag trace proves pixel-equal effective movement and one authored reconciliation on release. A source-declared click `Indicate` trace on both hosts verifies active Rust scale/color, unchanged authored time/revision, pixel-exact restoration, and idle settlement on WebGPU/WebGL2. [#1828](https://github.com/yongkyuns/noon/pull/1828) fixes the externally sampled worker wake, adds authored-motion parity, and bounds interaction clock catch-up; its exact-head CI passed and human review remains. See `scripts/platform-interaction-parity.mjs` and `crates/noon/src/execution_session/selection/tests.rs`. Test-only [#1840](https://github.com/yongkyuns/noon/pull/1840) verifies that dragging one translation channel leaves the same object’s authored opacity animation advancing. [#846](https://github.com/yongkyuns/noon/issues/846) remains open for editor-session lifecycle and broader action arbitration; the current editor has no semantic undo stack to consume the existing one-drag undo receipt.

### C6 — startup and topology

`scripts/playground-cold-start.mjs` measures source-ready, preload, worker execution, reconciliation, exact-session first renderer return, and a warm edit. A controlled preload comparison lowered first-edit latency by roughly 3.9–4.2 seconds on the measured Chromium package; instrumentation observed one authoring and one render WASM instance. See [#642](https://github.com/yongkyuns/noon/issues/642) for exact package hashes, raw artifacts, and caveats. The 4× setting throttles the page target, with a separate fixed JavaScript loop in the authoring worker; it does not calibrate Python interpreter CPU. A later three-run Chromium/WebGL trace separates renderer preparation from actual Python source execution: source execution began after renderer preparation ended, so page-observed `client.run` overlap was worker startup/transport rather than useful concurrent Python work. A topology split is therefore not justified by that apparent overlap. Process-tree RSS is not true peak memory, and renderer return does not prove physical scanout. Exact-head-green [#1845](https://github.com/yongkyuns/noon/pull/1845) separates the page target’s CPU throttle setting from worker-loop calibration; its paired 1×/4× desktop WebGL measurement did not demonstrate a worker-topology win. Python-specific worker calibration, true peak memory, and an un-overlapped startup reference remain open.

### C7 — measured specialization

The 600-shape/24-label gallery scene completed in the browser, but one headless Intel Mac/WebGL2 run sampled well below 60 FPS during several animated sections. Grouping the non-overlapping initial primitives reduced early draw calls from 627 to about 47 without a corresponding FPS gain in that single run; the change was discarded. Native Metal command encoding and release-mode active-channel probes provide bounded subsystem costs, not gallery frame-time attribution. A separate opt-in, bounded render-substage profiler in [#1829](https://github.com/yongkyuns/noon/pull/1829) passed its exact-head CI but awaits the required human review. Its paired 240-frame Field-in-Motion WebGL2 run measured renderer prepare/upload/encode/submit p95 at 2.2/0.1/1.8/4.5 ms. The profiled and unprofiled passes ran in sequence, so their observed FPS difference is not a causal profiler-overhead estimate; neither pass measures physical display scanout. A separate one-run headful Chrome 154/macOS 15.5 trace on the default AMD RDNA-1 WebGPU backend measured 627-object morph presentation intervals at 33.0 ms median (about 30 FPS), while renderer-call p95 was 1.7 ms. On an exact-head opt-in trace, engine tick-to-wake p95 was 9.7 ms, main-page RAF median 16.7 ms and render-worker RAF median 33.3 ms. Moving the intermediate wake before delta send in an isolated A/B did not produce a sustained 60 Hz cadence; the exploratory patch was discarded. These measurements localize delay to worker message/RAF scheduling but do not distinguish browser OffscreenCanvas cadence from queued worker work or measure compositor scanout. Evidence is recorded on [#847](https://github.com/yongkyuns/noon/issues/847) and [#67](https://github.com/yongkyuns/noon/issues/67). Render bundles or GPU interpolation should be adopted only after an isolated representative bottleneck and stable-path equivalence are demonstrated; deferral is a valid measured decision.

Phase C exit is not yet established. The unmerged Phase C PRs and remaining callback budget, editor lifecycle, startup-measurement, and hardware frame-pacing questions above remain explicit. Source hot reload stays under [#64](https://github.com/yongkyuns/noon/issues/64); normal Python Run is an explicit restart.

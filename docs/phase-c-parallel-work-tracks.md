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

Snapshot: 2026-09-30. This ledger points to executable evidence and named gaps; it does not change the roadmap or close an owning issue. Unit tests, focused smokes and renderer-specific fixtures prove only the contract they exercise.

| #955 area | Existing evidence | Remaining acceptance gap |
| --- | --- | --- |
| **C1 — callbacks and staged publication** | `crates/noon/src/host_callbacks.rs` and `crates/noon/src/execution_session/callback.rs` cover versioned callback reads, ordered transaction overlays, effective writes and provisional identities. Python fixtures `web/python/examples/ordinary_mixed_updater_order.py` and `ordinary_callback_provisional_path.py`, with their smoke paths in `scripts/shared-authoring-smoke.mjs`, qualify mixed native/host declaration order, same-phase reads and callback-local path construction; #1745 merged their shared publication path. The Python/WASM delayed-completion fixture holds a required phase without changing the frame, then rejects its late result after a newer phase begins. The Rust host callback read-miss fixture stages an effective transform before reading a nonexistent semantic object, then verifies the phase terminates without publishing that write or replaying the host callable. `web/python-worker-source.test.mjs` now drives the production continuation helpers through a failed sparse read and successor callback, proving that the late obsolete result cannot settle or replace the successor read or trigger callback replay. | #70 remains open. Arbitrary `always_redraw` resource production is not wired to an effective content/resource lane. Required-callback latency under load, broader read-miss suspension and stale async resource outcomes still need qualification; external host side effects remain outside Noon rollback. |
| **C2 — native input/events** | Native normalization/lifecycle cases in `crates/noon-native/src/pointer_input/tests.rs`, including `normalized_press_move_release_and_cancel_match_at_one_and_two_device_scale`, `repeated_edges_and_nonpointer_events_share_sequence_without_coalescing`, `pending_callback_rejects_bursts_without_acknowledging_or_overwriting_position`, and `paused_move_press_and_release_use_one_coherent_session_path`; DOM collector DPR 1/2 cases in `scripts/browser-pointer-input-smoke.mjs`; direct Rust/WASM interaction across WebGPU/WebGL and DPR 1/2 in `scripts/pointer-selection-direct-qualification.mjs`; worker/browser pixel path in `scripts/pointer-selection-qualification.mjs`. Bounded browser lanes are covered by `web/browser-pointer-input.test.mjs` (64 in-flight samples) and `web/semantic-engine-endpoint.test.mjs` (128 queued semantic controls); time-domain/replay classification is covered by `semantic_execution_player.rs::browser_input_classifies_unrecorded_replay_without_blocking_first_execution` and `semantic-engine-endpoint.test.mjs` (renderer timestamps do not become playback time). The WebKit DPR2 touch drag path is `scripts/playground-gallery-selection-smoke.mjs`. `pointer_input_trace::Fixture` now drives a paired native test and a direct Rust/WASM DOM case in `scripts/pointer-selection-direct-qualification.mjs`: both exercise subscribed down/up signals, paused-session selection, cancel without a synthetic up edge, and background clear through one Rust-authored scene. The direct qualification passed its ten-case WebGPU/WebGL matrix, with the paired Rust trace at WebGPU DPR1 and WebGL2 DPR2. The same Rust-authored fixture now also declares a Space key state and ordered press/release signals: the native test verifies final state, both edge counts, paused time and the presence binding; the direct browser/WASM path exercises DOM keydown/keyup and verifies the same paused presence transition. | #69 remains open. The paired pointer/key trace covers these bounded sequences, not all native/browser lifecycle outcomes or C5 drag arbitration. Native tests check key press/release signal counts; the browser qualification verifies key-state behavior but does not inspect those event counters, so cross-host key-edge counter parity remains open. It does not establish parity for touch/WebKit or a complete platform-equivalent interaction workload; other backend/DPR and WebKit cases remain separate qualification paths. |
| **C3 — retained locality/spatial queries** | `crates/noon-runtime/tests/static_frame_locality.rs` proves 100k static objects do zero timeline work on an unchanged frame; `geometry_patch_locality.rs` and `property_patch_locality.rs` prove one-object changes in a 100k scene; renderer residency/dirty upload cases are in `crates/noon-render-wgpu/tests/retained_static_frame_locality.rs` and `src/gpu/mod.rs`. The existing `hundred_thousand_objects_share_local_viewport_and_pointer_updates_through_churn` test in `crates/noon/src/execution_session/picking/tests.rs` checks one scene's pointer and viewport candidates through 64 local authored moves: stale pointer and viewport contexts are rejected, at most 16 candidates are tested, no full scan/rebuild occurs, and one leaf is refit each turn. The companion `hundred_thousand_object_animation_and_pointer_view_stay_local_across_ticks` repeats 32 animated frames with one-object frame deltas, no spatial full scan/rebuild, at most 16 viewport/pick candidates and one precise hit. `web/direct-execution-smoke-probe.js` runs a real direct-WASM 100k sparse-locality scene during `scripts/browser-smoke.mjs`; 64 normal-playback ticks each must advance the animated target and upload 1–256 bytes while object count, backend and draw calls remain valid. The same fixture now combines 64 admitted direct-WASM pointer moves and eight viewport revisions with that animation. WebGPU and WebGL2 both pass with 64/64 ticks uploading exactly 88 bytes, one draw call, and the final pointer presentation settled. View changes retire pointer sources, so the smoke allocates a newer source ID for each revision. This browser smoke proves GPU upload locality and presentation settlement under mixed direct input, not frame rate or pointer CPU candidate counts; the Rust tests above provide the latter. The native-host adapter trace in `crates/noon-native/src/execution_source/viewport_tests.rs::native_host_adapter_keeps_100k_viewport_and_pointer_candidates_local_through_churn` admits 64 pointer moves through `StaticExecutionSource` in a 100k scene, checks that renderer-facing viewport candidates and stats equal the session spatial query, and bounds the same session precise picker to 16 candidates and one precise test while each mutation refits one leaf without a full rebuild. This is CPU-side adapter and execution evidence. The 88-byte scope is independently exact: packed circle/rectangle/line instances are 88 bytes in `crates/noon-render-wgpu/src/lib.rs`; `src/gpu/mod.rs` verifies a dirty circle upload equals one `CircleInstance` (88 bytes), and `tests/line_instance_vertex_abi.rs` pins the line stride to 88 bytes. | #569/#362/#835 remain open. These tests establish selected 100k runtime locality and repeated direct-WASM updates on WebGPU and WebGL2; the native adapter trace is CPU-side only and does not demonstrate a native GPU draw or upload path. The 88-byte assertion is for a one-circle renderer fixture, not a universal per-scene or all-shapes update budget. Full acceptance still needs platform coverage across the specified workload shapes and sustained mixed workloads on the platform hosts.|
| **C4 — content replacement** | Shared semantic `ReplaceContent` publication and rollback coverage is in `crates/noon-compile/src/semantic_lowering/publication.rs` and `crates/noon-runtime/tests/live_patch.rs`. #1747 adds a runtime/session effective content lease with stale-result and conflict checks; its 600-lease test stages only the unrelated animated row, and a session test checks local spatial picking on replace/release. #1749 keeps a held lease valid across unrelated frame ticks. #1750 adds an explicit maintenance barrier for superseded compiled resources. The lease-aware barrier retains an active effective-content version as a resource root while pruning unrelated obsolete entries. The barrier now traverses ordinary track geometry dependencies, so a property animation no longer pins unrelated text history. Existing text/image handles and inline geometry are supported. The runtime/session image path now prepares a new immutable image from a producer-owned image arena, publishes it with the lease, resolves it through renderer lookup, and retires it after the last referring lease; stale results do not enter runtime lookup, and an active runtime-owned image does not block pruning unrelated compiled resources. A producer-owned external vector path can now be prepared against a versioned geometry arena, committed through the same lease, resolved by renderer publication, and retired after its final lease; same-ID claims from another producer are rejected at prepare and commit, while a sole owning lease can swap to a new version of its ID and dirty only that row. Shared readers cannot be silently retargeted. The compaction barrier excludes this runtime-owned resource while reclaiming unrelated compiled history. The shared execution spatial index now resolves the external path through the runtime resource lookup, refits one leaf on replacement, and removes it on release; viewport candidates see the path, and candidate-local fill picking resolves the active leased version for move/line/close polygons up to 4,097 commands with the renderer's non-zero rule; curves and morphs remain unsupported. A noop-device retained renderer prepares an actual path draw batch after the source arena is dropped and returns to the authored analytic primitive on lease release. Producer-owned retained text with font/vector dependencies now prepares through the existing compiled resource projection and commits under an effective-content lease; new dependencies advance execution and frame revisions together. Runtime tests verify failed/stale results publish no resources, reject conflicting font faces and bare geometry IDs, keep the current closure after source arenas drop, and reclaim exactly 21 entries from seven superseded closures at the explicit maintenance barrier. | #368 remains open. A Python text/content producer, bounded retirement across derived plans and general long churn (including periods when active drivers defer the explicit barrier), and an arbitrary Python `always_redraw` producer still need integration and platform qualification. A bounded Python `Mobject.set_effective_circle` updater now produces one terminal-region analytic circle replacement through the shared callback/session publication; async and synchronous browser fixtures qualify that path, while Rust tests cover same-frame property/content publication and lease conflict. A 600-object browser-player fixture repeats 33 callback content publications, verifies one-object callback read sets and dirty frame rows, reuses one lease, and avoids a spatial full rebuild; it is locality evidence, not a browser FPS measurement. Runtime-owned image and external vector-path primitives still have no Python producer. #64 owns source hot reload separately; normal Python Run is an explicit restart, not a Phase C C4 exit requirement. |
| **C5 — session/direct manipulation** | Native and browser input contracts feed public touch drag/cancel/Run-reset qualification in `scripts/playground-gallery-selection-smoke.mjs`; its artifact harness is `web/playground-gallery-selection-artifacts.test.mjs`. `crates/noon/src/execution_session/selection/tests.rs` covers native click-to-Indicate, stationary hover under presented geometry, animated release repicking, cancellation, out-and-back motion, callback barriers and 10k-object candidate locality. `translation_drag_tests.rs` covers one-commit drag, undo, cancellation, target-driver conflicts and drag alongside unrelated animation in multiple release/cancel orders. Its 32-cycle case repeats animation, drag release/undo and cancel while checking a quiescent baseline. `picking/tests.rs` checks topmost repicking when a second animated target crosses a previously animated target. Direct interaction pixels and DPR coverage are in `scripts/pointer-selection-direct-qualification.mjs`; its DPR1 Rust/Python fixture comparison proves selected-frame pixel parity. `execution_session::interactions::tests::overlapping_source_scale_supersedes_click_effect_while_unrelated_track_continues` verifies a source segment takes the visible click-effect state, retires its overlapping driver, continues an unrelated track, and reaches the authored endpoint. | #846 remains open. The existing paths do not yet prove one full pure-Rust/Python platform-equivalent interaction trace or sustained mixed-workload arbitration across platforms; the overlap case is a bounded Rust session proof, not full platform qualification. Source hot-reload reference migration belongs to #64. |
| **C6 — startup/topology** | `scripts/playground-cold-start.mjs` compares the existing `live-authoring-bootstrap.js` automatic preload with a controlled off arm that fulfills only that module as empty, waits the same two-animation-frame gate, then submits the identical public source edit. `web/main.js` marks selected-source/public-gallery-API readiness; the existing bootstrap marks preload start. Scene presentation is attributed to the exact retained transport session’s first successful `renderer.render()` (blank prepared-canvas frames are excluded). Production package `c06f27f49848542a9f6d4ddfc21334e942f1ad2a` (66,804,567-byte WASM), Chromium 151 on macOS 15.5 / Intel i7-9750H; mobile-class is 390×844 DPR2 with 4× CPU throttle. Three runs per profile and mode, geometry and retained text; summary `/tmp/noon-c6-preload-comparison-summary.json`, run artifacts `/tmp/noon-c6-content-{desktop,mobile-class}-{on,off}[-r2/-r3].json`; measurement code is committed as `3cadc6db9`. After preload completion, first edit→scene render medians were desktop 1.550 s geometry / 0.551 s text and mobile-class 1.554 s / 0.571 s. Without preload, the first edit→scene render medians were desktop 5.511 s / 4.459 s and mobile-class 5.686 s / 4.785 s. The measured first-edit improvement is 3.91–4.21 s (about 72–88%). Selected source/API readiness preceded the two-frame gate; automatic preload began 24–49 ms later on desktop and 16–112 ms later on mobile-class. Both arms had one authoring and one render worker after the first run; the warm follow-up retained their worker topology. A separate preload-plus-two-edit race used the exact `21b2e6bb7` master package (82,207,740-byte WASM), Chromium 151 and the `parity-create-circle` scene: across three desktop and three mobile-class runs, newest edit to exact-session renderer return was 3.915–5.739 s (desktop median 4.195 s) and 3.725–4.090 s (mobile-class median 3.970 s). Every run rendered the newest two-object scene and sampled no obsolete sessions after the newest edit; artifacts are `/tmp/noon-c6-master-race-{desktop,mobile-class}-r{1,2,3}.json`. A separate three-run-per-profile warm-edit probe on the 82,207,740-byte package `21b2e6bb7` (Chromium 151) records page-observed call intervals keyed to accepted run generation, semantic context and presented session. Geometry source-run medians were 1.022/1.026 s desktop/mobile-class, including its authored one-second continuation; continuation reconciliation was 7.4/14.5 ms and replay 8.8/11.7 ms. Retained-text source-run medians were 9.4/9.3 ms and final reconciliation 35.8/43.8 ms. Continuation reconciliation can occur inside source execution, so these intervals are not additive; raw runs are `/tmp/noon-c6-phase-timing-{desktop,mobile-class}-{1,2,3}.json`. `web/authoring-out-of-order-races.test.mjs` deterministically holds the preload response through two rapid edits, checks zero obsolete publication, and verifies that only the newest edit is dispatched and published. An opt-in instance-accounting run on combined validation frontend `f8cf133e20cd1ef7953c6c417aab5be96b27a6fd` used Chrome 154.0.8037.58, WebGL and `parity-create-circle`; package size was 82,161,401 bytes (SHA-256 `ce40dcbd2c6c1f796fad181a1f96ef1dd5a5fc95b14059ff0f0cd62104e302a6`, runtime identity source revision null). It observed one successful Noon wasm-bindgen instance in `authoring-0` and one in `render-0`; both exact module inputs were 82,161,401 bytes, total 164,322,802 bytes. Both contexts were instrumented and available; artifact `/tmp/noon-c6-wasm-validation.json`. An opt-in single-purpose Python compute probe (`NOON_COLD_START_PYTHON_COMPUTE=on`) runs a fixed 300,000-iteration integer loop and a minimal one-circle scene without awaited source continuation. Six real-browser runs on the 82,161,401-byte release Rust/WASM package without wasm-opt (runtime build `c096e7c07c4c4da474dc172eb3bec43bef7674a2bd094e04e673e28429f29ca9`) used installed Chrome 154 on macOS 15.5/Intel i7-9750H: three desktop and three mobile-class (390×844 DPR2, page CPU throttle 4×). Desktop worker source intervals were 142.2–162.9 ms (median 154.6 ms) with final reconciliation 10.6–11.3 ms (median 11.1 ms); mobile-class intervals were 172.8–181.7 ms (median 174.2 ms) with final reconciliation 16.7–20.0 ms (median 18.1 ms). Raw artifacts are `/tmp/noon-c6-python-compute-{desktop,mobile-class}-r{1,2,3}.json`. The source interval includes imports, compilation and interpreter/setup overhead; page CPU throttling may not apply uniformly to worker execution. The opt-in hook identifies the wasm-bindgen export signature and exact source lengths, but does not measure linear-memory capacity or resident/peak memory. Its worker interception/import/wrappers are included in startup timing, so these timings are not comparable to uninstrumented runs. | #642 remains open. The completed-preload comparison isolates a lower first-edit latency; the browser race arm supplies bounded latency and sampled supersession evidence for edits arriving during preload. The source-ready mark identifies the selected source and public API, while two rAFs give the browser a paint opportunity; neither is pixel proof of source text reaching scanout. Renderer-return timestamps are not physical scanout. Navigation-to-authored-frame medians also varied: on/off were 4.906/5.905 s geometry and 4.075/4.810 s text on desktop, 5.806/6.467 s and 4.553/5.657 s mobile-class; the off path waits for a first source edit and its normal debounce, so these are not an isolated startup-overlap attribution. Still missing are pure Python compute time excluding imports/compilation, cold first-run reconciliation timing, true peak memory, and an un-overlapped reference to isolate overlap benefit. The deterministic test exercises production restart/router/generation guards with a held worker response; it is not a physical browser scanout observation. The race numbers are Chromium measurements, with a 4× throttled mobile-class emulation rather than an iPhone; sampled process-tree RSS may double-count shared pages and excludes external GPU allocations. |
| **C7 — measured specialization** | The locality and startup measurement paths above provide workload/counter entry points. | No specialization should be claimed from these proofs alone. A proposed optimization still needs an isolated representative trace, before/after evidence and equivalence checks against the stable path. |

The current evidence supports selected callback ordering, provisional Python construction, input, locality, renderer ABI, effective content and startup contracts; it does not establish Phase C exit. The remaining gaps are the row-specific producer/resource, cross-platform trace, long-churn and startup qualification above. Source hot reload is the separate #64 follow-on; normal Run remains an explicit restart.

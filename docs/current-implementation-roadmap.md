# Noon current implementation roadmap

Status date: 2026-09-02  
Baseline: `master` at `c7827fb7fa6cebfe2474de35edb9cd8880ab8fe5` (`#871`, retained `Text` `Write` / `Unwrite` through canonical families)

This document is the current execution roadmap for Noon. It complements the original milestone-oriented `docs/implementation-plan.md`, which describes how the architecture was intended to be built. This document instead answers: **given the repository as it exists now, what remains, in what order should it be implemented, and what proves each tranche complete?**

The architectural target remains the scalability contract from #68:

```text
static/paused clean frame    ~ O(0) meaningful CPU/render work
timeline CPU work            ~ O(events crossed + CPU-active channels)
reactive CPU work            ~ O(dirty dependency closure)
property edit                ~ O(affected object/slots)
structural edit              ~ O(local dependencies)
render preparation           ~ O(dirty resident state + visible projection)
render upload                ~ O(changed slots/resources)
draw submission              ~ O(visible batches/instances)
hit test                     ~ O(index query + candidates)
host bridge                  ~ O(host-relevant state)
resource regeneration        ~ O(actual content/resource changes)
```

For common 2D Manim compatibility, ManimCE v0.21.0 remains the semantic/API/visual/timing oracle. Current 3b1b/manim can be used as an interaction/live-authoring reference, but it is not a second source-compatibility target.

---

## 1. Current state

Noon is substantially further along than several open tracker descriptions and the checked-in compatibility manifest imply. Planning must distinguish:

1. **missing implementation**;
2. **implemented but not parity-qualified**;
3. **implemented but still behind a migration/compatibility seam**;
4. **stale issue/manifest bookkeeping**.

### Architecture already substantially landed

The architecture reset has delivered the core contracts needed for broad feature work:

- stable semantic identity/family/source identity;
- sparse event-driven timeline scheduling;
- painter/z-correct ordered rendering;
- typed native reactive compute IR/VM;
- host-callback realtime/deadline/seek semantics;
- pinned ManimCE differential validation;
- stable execution-slot/local-mutation work;
- immutable resource ownership and retained rendering infrastructure;
- canonical `SceneSpec` as the authored execution model;
- unified geometry/Text scene-global object identity;
- retained native Text/Typst rendering;
- canonical retained Text family animation, including Create/Uncreate and Write/Unwrite;
- substantial retained plotting/coordinate-system implementation;
- viewport/spatial-index infrastructure;
- worker-separated browser runtime.

### Important bookkeeping mismatch

The compatibility policy has been reconciled against current `master`: retained `Text`, `Typst`, and `MathTypst` facades with executable browser evidence are recorded as partial until exact-output gates pass, while plotting classes such as `Axes`, `NumberLine`, `NumberPlane`, `ComplexPlane`, `PolarPlane`, `ParametricFunction`, `FunctionGraph`, and `ImplicitFunction` are recorded as missing because no plotting facade is exported on this ref. Historical plotting branches and PRs are not current-master evidence.

Therefore the compatibility manifest must not be treated as the implementation backlog until it is reconciled with executable evidence.

### Major active boundaries

| Area | Current interpretation | Main trackers |
| --- | --- | --- |
| Canonical scene model | Architecture largely landed; migration cleanup remains | #367 |
| Retained Text family animation | Create/Uncreate/Write/Unwrite architecture landed; parity/performance breadth remains | #705, #741, #835, #362 |
| Plotting | Broad implementation exists; finish/rebase/qualify rather than redesign | #85, #856 |
| Core Mobject/VMobject | Still a genuine high-leverage feature foundation | #74, #78 |
| Text/math public API | Largest remaining common-2D dependency | #83, #364, #365 |
| Graph/DiGraph | Shared topology implementation has started | #87, #697, #866 |
| Native interaction | Schema exists; runtime ingress/queueing/session integration remains | #69, #846 |
| Performance | Remaining work is specific/measurable rather than another reset | #362, #569, #642, #847 |
| 3D | Semantic design exists; production retained 3D renderer still genuinely open | #90, #698, #699 |

---

## 2. Priority order

Use the following priority order unless a correctness regression blocks master.

| Priority | Workstream | Why now |
| --- | --- | --- |
| P0 | Correctness and authoritative bookkeeping | Prevent invalid feature planning and backend regressions |
| P1 | Finish canonical/family architecture seams | Avoid carrying compatibility scaffolding into new features |
| P1 | Complete core Mobject/VMobject semantics | Multiplies the implementation speed of most remaining 2D features |
| P1 | Complete text/math public surface | Unblocks labels, matrices, graph labels, Tex transforms, many examples |
| P2 | Finish common-2D breadth | Build on the now-stable shared semantics |
| P2 | Scalability/startup locality | Remove measured remaining O(total) or duplicated startup work |
| P3 | Native interaction/live authoring | Turn Noon into an interactive retained system rather than only browser Manim |
| P4 | 3D/vector-space | Important, but must not distort common-2D completion |

---

# P0 — correctness and repository truth

## 3. Fix #513 WebGL host-updater divergence

This is the highest-priority runtime correctness bug because it questions backend equivalence for dynamic/host-updated scenes.

### Implementation plan

Instrument one failing object at four boundaries for the known `RotationUpdater` frame:

```text
committed semantic/runtime state
    -> RetainedFramePreparer output
    -> GPU upload/instance state
    -> submitted/presented WebGL frame
```

At each boundary capture, in debug/test instrumentation only:

- object/slot identity;
- execution session/sequence/generation;
- world transform or resolved endpoints;
- dirty classification;
- GPU instance range/generation;
- draw submission membership.

Compare the same data against WebGPU at the failing logical time.

### Do not

- relax raster tolerance;
- force full-scene snapshots;
- force full GPU uploads;
- add arbitrary screenshot delays as the product fix.

### Acceptance

- deterministic regression reproduces the forward host-updater path;
- first divergent layer is identified by state, not timing speculation;
- WebGL and WebGPU both pass the unchanged canonical raster oracle at the failing sample;
- direct seek and forward playback agree where the behavior is deterministic;
- no O(total) fallback is introduced.

---

## 4. Reconcile #73/#93 compatibility bookkeeping

The manifest should become an output of executable evidence rather than an increasingly stale manual approximation.

### Implementation plan

1. Audit each public class currently marked `blocked`/`partial` against current `master` exports and acceptance tests.
2. Distinguish at least these states conceptually:
   - `missing`;
   - `implemented-unqualified`;
   - `partial`;
   - `parity-qualified`;
   - `deferred`;
   - `intentional-divergence`.
3. Update plotting and retained-text classifications first because they contain the largest known drift.
4. Close or annotate historical stacked PRs whose feature blobs have already landed through superseding branches.
5. Add a policy test that catches obvious contradictions such as a public exported facade with required browser acceptance still marked `blocked`.

### Acceptance

- #73 and the JSON manifest describe current `master` rather than historical architecture state;
- every supported/qualified entry names executable evidence;
- tracker issues contain explicit remaining checklists rather than pre-migration prose only.

---

## 5. Make exact-output validators pre-merge

Continue the direction of #865/#851/#838: specialized correctness oracles must run on relevant PRs before merge rather than discovering regressions only on `master`.

### Acceptance

- Manim raster differential runs pre-merge on owned source paths;
- retained Text/Typst specialized gates run pre-merge on their owned paths;
- trigger-policy tests prevent PR/push path-filter drift;
- existing tolerances and reference fixtures remain unchanged unless a separately justified ratchet is reviewed.

---

# P1 — remove remaining architecture seams

## 6. Finish #367 canonical mixed-scene migration

The target is one authored semantic scene and one canonical `SceneSpec`. The transitional retained sidecar should no longer be part of normal architecture.

### Current direction

The repository has already unified geometry/Text object IDs and canonical retained execution. The remaining seam is the internal compatibility flow where Python retained output is still adapted into the canonical Rust-owned scene representation.

The first producer-ownership slice keeps one intentional transport seam: Python
bind/update/finalize calls currently use compact JSON payloads to invoke the typed
Rust/WASM canonical context. This is transitional authoring transport, not a
frame-loop mutation API. The follow-up should pass the existing typed semantic
handles (or equivalent Rust-owned request values) directly and retain JSON only
for debug/export compatibility.

### Implementation tranches

#### 6.1 Remove retained sidecar from normal authoring results

- Python wrappers emit semantic handles/requests only;
- Rust `Scene` owns canonical object/resource/family-animation production;
- `AuthoringExecutionClient` consumes only canonical output for normal execution.

#### 6.2 Remove split reconciliation

- geometry and Text/resource state cannot be reconciled independently;
- mode switching/re-run uses one scene identity graph;
- retained resource replacement is expressed as localized canonical mutations.

#### 6.3 Delete retired compatibility identity scaffolding

Remove after no production caller depends on it:

- special retained ID namespace logic;
- retained-sidecar-only validators;
- duplicate painter-order reconciliation;
- compatibility fields whose only purpose was split geometry/Text startup.

#### 6.4 Ratchet the boundary

Add structural tests forbidding reintroduction of the old paths.

### Acceptance

- mixed geometry/Text scenes author and execute from canonical `SceneSpec` alone;
- all object/resource/family-animation identities are scene-global;
- no production execution path depends on `retained_document()` reconciliation;
- #367 can close.

---

## 7. Finish resident retained-family rendering (#835/#362)

The functional family-animation architecture is now useful enough that the next goal is locality.

### Target

```text
family animation frame work ~= O(active/dirty members)
```

not repeated family/glyph topology reconstruction.

### Implementation plan

1. Cache immutable per-member outline handles with the retained text resource.
2. Keep semantic family membership/member ordering resident and stable.
3. Maintain per-member animation state in compact arrays keyed by stable family member identity.
4. Track dirty member intervals/ranges.
5. Prepare reveal/DrawBorderThenFill state only for dirty members.
6. Upload only affected instance/vector ranges when resource topology is unchanged.
7. Return immediately to the glyph-atlas fast path when no path-level family animation is active.
8. Preserve one retained Text semantic object; do not promote glyphs to ordinary scene objects merely for animation.

### Acceptance

- static retained Text frame does effectively no text preparation/upload work;
- one-member/few-member changes do not rebuild every member;
- Write/Create use shared family state and lazy outlines;
- text-heavy benchmark exposes dirty-object/member metrics;
- WebGPU/WebGL exact-output gates remain unchanged.

---

# P1 — complete high-leverage common semantics

## 8. Complete #74 core Mobject/VMobject behavior

This is the most valuable general-purpose 2D feature work because almost every specialized class depends on it.

### Tranche O1 — family operations

Implement shared semantic behavior for:

- family traversal;
- indexing/slicing;
- `add`, `remove`, insert/reorder;
- stable submobject identity;
- deterministic family/painter order.

No Python-only family engine.

### Tranche O2 — bounds and geometry queries

Complete:

- center/edge/corner/critical-point queries;
- start/end where meaningful;
- width/height/depth-compatible getters;
- accurate semantic bounds through current transforms.

### Tranche O3 — generic transform vocabulary

Complete:

- `set_x/y/z`;
- `set_coord`;
- `match_*`;
- `stretch`;
- `stretch_to_fit_width/height`;
- `flip`;
- `rotate_about_origin`;
- center-pivot Manim scale routing through the shared `manim_scale` semantic operation.

### Tranche O4 — layout semantics

Complete option parity for:

- `next_to`;
- `align_to`;
- `to_edge`;
- `to_corner`;
- `arrange`;
- `arrange_in_grid`.

### Tranche O5 — state/copy/target semantics

Complete:

- `generate_target`;
- `save_state` / restore-compatible state;
- `become`;
- `replace`;
- copy semantics with fresh semantic identity and preserved resource sharing.

### Tranche O6 — VMobject query facade

Expose shared path queries needed by downstream animation/geometry work rather than duplicating path math in Python.

### Acceptance

- at least 15 focused Manim differential fixtures across family/layout/state/style/query behavior;
- `.animate` works automatically for newly supported mutators through shared target-state semantics;
- family operations remain runtime semantic data, not a Python list flattened at `Scene.add`.

---

# P1 — text and math

## 9. Finish #83 public text/math/numeric parity

The retained text architecture is no longer the main blocker. Public authoring behavior and exact parity are.

All backends should converge on the same retained `TextResource`/member identity model.

## 9.1 Native `Text` / `MarkupText`

Complete:

- constructor option parity for common fonts, size, weight/style;
- line spacing and alignment;
- substring/range styling;
- source-part identity;
- gradients/color ranges where supported by the shaping model;
- `Paragraph` if it composes cleanly from the same resource model.

Do not make path-per-glyph the normal text representation.

## 9.2 `MathTypst`

Implement:

- `{{ ... }}` grouping;
- `select(str | int)` semantics;
- stable part identity derived from source/resource metadata;
- coloring/indexing through shared part identities.

## 9.3 Real LaTeX backend (#365)

Implement `Tex`, `MathTex`, and compatible `SingleStringMathTex` behavior through a real LaTeX pipeline that produces the same retained resource representation.

Do not silently translate TeX into Typst.

Required architecture:

```text
Tex/MathTex source
    -> deterministic LaTeX compiler adapter/cache
    -> normalized glyph/vector decorations
    -> shared TextResource + source/part identity
    -> existing retained renderer/family animation
```

### Requirements

- deterministic compiler/resource caching;
- diagnostics for unsupported/missing packages/fonts;
- browser-compatible resource policy;
- no SVG-only separate scene architecture;
- no per-frame Python renderer.

## 9.4 Numeric text

Implement:

- `DecimalNumber`;
- `Integer`;
- `Variable`;
- common formatting/update semantics.

Value changes should use localized retained resource replacement, not delete/recreate full semantic scenes.

## 9.5 Integration

Then qualify:

- `Write` / `Unwrite` on native Text/Typst/Tex;
- `TransformMatchingTex`;
- coloring/indexing by tex/part;
- changing-number animations.

### Acceptance

Every promoted public text/math slice must pass source-equivalent Manim semantic + raster + timing/seek validation according to #176/#185.

---

# P2 — finish coordinate plotting instead of redesigning it

## 10. Close out #85

The current plotting architecture is appropriate:

- Rust owns coordinate semantics, sampling, transforms, contour topology, geometry construction and validation;
- Python evaluates arbitrary authored functions only at Rust-requested authoring-time samples;
- retained output is ordinary Line/Circle/VectorPath/Rectangle geometry;
- no per-frame Python callback for static plotting.

Preserve that model.

### Immediate work

1. Replay/refresh current active plotting PRs onto current `master` rather than merging old stacked histories.
2. Land `ImplicitFunction` / `Axes.plot_implicit_curve` after requalification.
3. Route the public Manim `scale` adapter through shared center-preserving `manim_scale` where needed.
4. Finish arrow/tip-dependent plotting after #77.
5. Add numeric/MathTex labels after #83.
6. Add nonlinear/log scaling only through a shared coordinate-system scaling abstraction; do not put nonlinear transforms in Python loops.
7. Qualify representative literal upstream plotting examples.
8. Correct #73 manifest status as slices become qualified.

### Acceptance

#85 should eventually become mostly a parity/qualification parent rather than a permanent architecture issue.

---

# P2 — Graph / DiGraph

## 11. Continue #866/#697 architecture

The existing direction is correct: shared Rust owns topology and stable identities, with per-vertex incident-edge indexing.

### Tranche G2.1 — land topology core

- monotonic stable vertex/edge IDs;
- endpoint validation;
- undirected duplicate semantics;
- directed orientation semantics;
- O(degree) incident lookup;
- transactional vertex removal.

### Tranche G2.2 — semantic-family binding

Represent a Graph as shared semantic family structure with stable child object identities and deterministic painter order.

### Tranche G2.3 — explicit-position Graph

Start with explicit vertex positions only.

- vertices can be ordinary existing mobjects;
- undirected edges lower to ordinary `Line`;
- directed edges lower to #77 `Arrow`;
- no Graph-specific renderer primitive.

### Tranche G2.4 — native edge following

Vertex position change:

```text
changed vertex
    -> incident_edges(vertex)
    -> update only those edge geometries
```

This should be native dependency work, not Python updater callbacks.

### Tranche G2.5 — local structural mutations

Implement add/remove vertices/edges as localized semantic/execution deltas.

### Tranche G2.6 — thin Python/JS facade

Expose Manim-facing `Graph`/`DiGraph` wrappers over the same shared topology.

### Later

- automatic layouts;
- optional NetworkX as authoring-time layout provider only;
- labels after #83;
- broader graph mutation/layout animation parity.

### Acceptance

Moving one vertex in a large sparse graph must update O(degree) graph dependencies, not scan O(V+E).

---

# P2 — remaining common-2D feature breadth

## 12. Recommended dependency order

After #74 and the core text path are stable, use this sequence:

### 12.1 #77 — lines/arrows/tips/dashes

Finish Arrow/tip families and shared line-derived geometry.

### 12.2 #78 — VMobject path/boolean surface

Complete:

- path construction/query APIs;
- partial paths;
- tangent/point-at-proportion facilities;
- boolean geometry;
- shared exact path semantics needed by animation/plotting.

### 12.3 #76 — remaining common shapes

Build specialized shape classes on shared geometry rather than adding renderer primitives.

### 12.4 #82 — transform/matching family

Finish family-aware target alignment and matching-parts semantics, especially Tex-aware matching after #83.

### 12.5 #80 — deterministic animation breadth

Complete remaining creation/growing/movement/rotation/composition features that can lower entirely to deterministic retained state.

### 12.6 #81 + #70 — dynamic/updater animations

Implement changing/updater/`always_redraw` behavior through bounded shared mutation transactions and explicit host/native driver arbitration.

### 12.7 #79 — SVG/image resources

Treat SVG/images as immutable retained resources with content-addressed caching and localized replacement. Do not create frontend-only asset semantics.

### 12.8 #84 — braces/matrices/tables/composites

Build from #74 family/layout semantics and #83 text/math.

### 12.9 #89 — MovingCameraScene / ZoomedScene

Use one semantic camera and persistent canvas/render owner. Do not add separate scene engines or replacement canvases.

### 12.10 #88 — fields/probability

Implement static vector fields/charts first; dynamic StreamLines only after native dynamic mutation/runtime semantics are ready.

---

# P2 — measured performance and startup

## 13. Finish #642 from current topology, not its historical topology

The original problem statement includes startup behavior that has already changed. Narrow the remaining work to what measurement still proves expensive.

### Required measurements

For cold and warm runs record:

- page ready;
- Python worker ready;
- source authored;
- execution WASM ready;
- renderer ready;
- exact first presented frame;
- worker count;
- WASM instance count;
- package bytes/compile/instantiate time;
- peak memory where practical.

Run at least:

- first geometry example;
- first retained Text example;
- warm rerun;
- constrained/mobile-class browser configuration.

### Remaining implementation candidates

- remove duplicate callback reconciliation where still present;
- add explicit renderer first-presented-frame instrumentation;
- split authoring/engine/renderer WASM surfaces **only if profiling shows material compile/memory benefit**;
- preserve lazy page startup: no Pyodide/GPU allocation before explicit Run.

### Acceptance

Do not optimize based on worker-count aesthetics alone; demonstrate click-to-first-frame and/or memory improvement on repeatable workloads.

---

## 14. Complete retained culling/locality (#569)

Keep the execution engine as the spatial-index owner.

Required path:

```text
semantic/runtime bounds
    -> engine spatial query
    -> ordered visible candidate identity
    -> renderer mirror resolution
    -> resident GPU caches
    -> visible-only submission
```

Do not create a second renderer-side scene spatial index.

### Acceptance

- mostly-offscreen large-scene browser gate;
- candidate work scales with spatial query result, not live-set size;
- resize/camera changes preserve generation/order correctness;
- static resident GPU resources are not reuploaded merely because visibility changes.

---

## 15. Keep #847 measurement-driven

Render bundles/draw-plan caching are not mandatory architecture.

Complete the benchmark matrix first:

- semantic-camera movement with stable topology;
- transform-only workloads;
- unique-path/mega-mesh workloads;
- text-heavy workloads;
- mixed painter order;
- deliberate visibility invalidation;
- small-scene bookkeeping.

Adopt production render-bundle reuse only if encode/submit cost is a material measured bottleneck and invalidation can remain local.

---

# P3 — native interaction and live authoring

## 16. Finish #69 runtime ingress

The core contract should distinguish:

- sampled state: latest value wins per exact source identity;
- discrete events: ordered occurrences with monotonic ingress sequence;
- authored timeline time: independent from input arrival.

### Implementation plan

1. Normalize DOM/native input at the browser boundary.
2. Assign monotonic host-ingress sequence numbers to discrete events.
3. Maintain bounded latest-value slots for sampled state.
4. Maintain bounded ordered queue for discrete events.
5. Transfer batches to the execution worker.
6. Apply them through the native reactive graph without Python.
7. Wake paused scenes for input processing without advancing timeline time.
8. Expose instrumentation:
   - sampled coalesced count;
   - discrete queue depth;
   - dropped/rejected events if bounded capacity is exceeded;
   - input-to-apply latency.

### Browser acceptance

- burst mouse/pointer movement coalesces;
- key/pointer press/release order never collapses;
- input can update a paused scene;
- no scene scan is needed to route one bound source;
- malformed/non-finite payloads fail before reactive evaluation.

---

## 17. Add #846 interactive session state above #69

Keep editor/session concerns out of semantic scene identity.

Session state includes:

- selection;
- hover;
- pointer capture;
- drag reference values;
- direct-manipulation state;
- undo grouping;
- inspector state;
- editor overlays.

### Property-driver arbitration

Direct manipulation must explicitly resolve conflicts between:

- timeline ownership;
- reactive ownership;
- host updater ownership;
- editor/session manipulation.

Do not let "last callback wins" become an implicit policy.

### Acceptance

- hover/selection do not create semantic mobjects;
- paused direct manipulation works without timeline advancement;
- one drag updates only affected semantic slots/dependencies;
- cancel/commit/undo have transactional boundaries.

---

## 18. Use #70 as the shared mutation vocabulary

The same structural/geometry transaction representation should serve:

- Manim `always_redraw` / updater mutations;
- host callbacks;
- native editor tools;
- graph local topology updates;
- hot reload reconciliation.

The engine, not Python/JS, validates and commits mutations transactionally.

---

## 19. Finish #64 hot reload after stable session/runtime state exists

Reconcile new authored source against stable source/semantic identities.

Preserve compatible:

- execution slot identity;
- resource identity/cache entries;
- reactive state;
- native input bindings;
- editor/session selection where source identity still resolves.

Reset only incompatible subtrees.

---

# P4 — 3D/vector-space

## 20. Preserve #90 staged design

3D should extend the current architecture, not create a parallel scene engine.

### Stage 0 — refresh #698 semantic substrate

Use current canonical types to establish:

- `WorldTransform`;
- normalized quaternion rotation;
- `Camera3DState` matching observable ManimCE v0.21 behavior;
- immutable validated `MeshResource`;
- explicit 2D -> 3D transform lifting;
- renderer-independent camera projection tests.

Refresh old Stage-0 work onto current canonical `SceneSpec`/resource APIs before merging.

### Stage 1 — #699 retained mesh renderer

Add to the existing GPU device/surface:

- depth attachment;
- opaque mesh pipeline;
- mesh resource residency/cache;
- camera uniform/state;
- one minimal `ThreeDScene` + `ThreeDAxes` + mesh/surface fixture.

Do not add a separate canvas or 3D runtime.

### Later stages

- generated solids;
- Surface sampling;
- lighting/material semantics;
- fixed-in-frame mixed 2D/3D composition;
- explicit transparency policy;
- ambient camera rotation and camera helpers;
- vector-space scenes.

### Acceptance

Every promoted 3D slice uses the same semantic/timing/seek/raster qualification discipline as common 2D.

---

# 21. Concrete next-work sequence

Given the 2026-09-02 master state, the recommended sequence is:

1. Reconcile #73 compatibility manifest and stale trackers.
2. Fix #513 WebGL host-updater correctness.
3. Finish #367 and remove the retained-sidecar production seam.
4. Finish resident/incremental family rendering (#835/#362).
5. Drive #74 core Mobject/VMobject semantics to completion.
6. Finish native Text/MarkupText, MathTypst part semantics, then real Tex/MathTex (#83/#365).
7. Refresh and land remaining plotting completion work, including #860, against current master.
8. Land #866 topology and implement explicit-position Graph end-to-end.
9. Land #869's runtime ordering contract and implement browser/worker native-input ingress.
10. Fill remaining common-2D gaps in #76-#82, #84, #88, #89 in dependency order.
11. Finish measured #642 startup and #569 locality work.
12. Build #846 direct-manipulation/session state and #64 hot reload on the native input/mutation substrate.
13. Refresh #698 and start #699 production 3D rendering.

Parallelism is encouraged where ownership boundaries do not overlap. In particular:

```text
correctness/CI        #513 #73 #865
core semantics        #74 #78
text/math             #83 #364 #365
renderer locality     #362 #569 #847
graph                 #697 #87
native interaction    #69
startup               #642
3D semantic refresh   #698
```

should be able to progress substantially in parallel if each PR stays within its owned boundary.

---

# 22. Review checklist for every implementation PR

Before merging a feature or architecture slice, answer all of the following.

### Ownership

- Is semantic behavior owned in shared Rust rather than duplicated in Python/JS?
- Is renderer-specific state derived from semantic/resource state rather than becoming a second authority?
- Are editor/session concerns kept out of authored semantic identity?

### Complexity

- What is the expected complexity in terms of active, dirty, visible, or affected objects?
- Does a local change accidentally scan/rebuild the whole scene/family/resource set?
- Does static work disappear from the frame path?

### Resources

- Is heavy geometry/text/image/mesh content immutable and shared?
- Are resource replacements localized and generation-safe?
- Is GPU residency preserved across transform/style/visibility-only changes?

### Determinism

- Does direct seek equal forward playback where the behavior is classified deterministic?
- Are ordering/lifecycle boundaries explicit?
- Are malformed/non-finite inputs rejected at the owning boundary?

### Compatibility

- Is ManimCE v0.21 behavior used as the oracle for supported public APIs?
- Is unsupported behavior explicit rather than silently approximated?
- Does the PR add/update executable source-equivalent evidence where practical?
- Is #73 updated when a public compatibility classification changes?

### CI

- Does the exact PR head run the relevant specialized gate?
- Are raster/timing tolerances unchanged unless the PR is specifically a reviewed tolerance ratchet?
- Are PR and master workflow path filters aligned for specialized validators?

---

# 23. Common-2D completion milestone

Noon can call the common-2D tranche coherent when all of the following are true:

- #73 accurately reflects current executable support;
- core Mobject/VMobject family/layout/state/query APIs are broadly complete;
- native Text, Typst, Tex/MathTex and numeric text have representative exact-output coverage;
- representative geometry, transform, updater, axes/plotting, Graph, table/matrix, SVG/image and moving-camera examples run in CI;
- canonical `SceneSpec` is the only normal authored execution model;
- retained static scenes remain effectively asleep when clean;
- local property/structural/input changes remain local through execution and rendering;
- common native interaction works without waking Python;
- WebGPU/WebGL agree for the supported backend surface;
- representative source-equivalent ManimCE v0.21 examples require only the intended import/browser adaptation.

At that point, feature work can shift decisively toward 3D, advanced live-authoring UX, and broader Manim namespace coverage without carrying unresolved common-2D architecture debt.

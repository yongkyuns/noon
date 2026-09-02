# Noon current implementation plan

Status date: 2026-09-02  
Planning baseline: `master` at `def6e86f461286617f127f7fd286077e2ab3c035`  
Companion roadmap: [`current-implementation-roadmap.md`](current-implementation-roadmap.md)

This document converts the current roadmap into **individually executable implementation items**. The roadmap answers *what matters and in what priority order*. This document answers *what a concrete PR should implement, which subsystem owns it, what it depends on, how it should be tested, and what makes it complete*.

The item IDs below are planning IDs, not replacements for GitHub issue numbers. Existing issues remain the source of public problem statements and discussion.

---

## 0. Execution rules

### 0.1 PR sizing

Prefer one implementation axis per PR. A PR should normally do one of:

- define/extend one shared semantic contract;
- wire one existing shared contract into a frontend/runtime/renderer consumer;
- add one feature family;
- add one qualification/performance gate;
- remove one obsolete migration seam.

Do not combine unrelated authoring, execution, renderer, cache, and API work merely because they touch the same public feature.

### 0.2 Definition of done

A public feature is **not complete because its constructor exists**. A feature is complete only when the applicable layers are covered:

1. shared Rust semantic representation;
2. canonical `SceneSpec`/resource/family identity;
3. deterministic execution/direct-seek behavior;
4. retained renderer integration;
5. thin Python/JS authoring adapters where public support is claimed;
6. ManimCE v0.21.0 semantic/raster/timing parity where compatibility is claimed;
7. browser backend qualification on WebGPU/WebGL where supported;
8. compatibility manifest/tracker evidence updated.

### 0.3 Architecture invariants

Every item must preserve the #68 scaling model:

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

Do not introduce:

- per-frame Python for deterministic built-ins;
- frontend-owned geometry/layout/render semantics;
- full-scene recompilation for local property/resource edits;
- renderer-specific semantic identity;
- path-per-glyph steady-state text;
- a second scene/runtime architecture for Graph, Text, interaction, or 3D.

### 0.4 Compatibility oracle

For common 2D compatibility, ManimCE v0.21.0 is the semantic/API/visual/timing oracle. Current 3b1b/manim may inform interaction or renderer capability, but it is not a second compatibility target.

---

# Phase A — P0 correctness and repository truth

## ITEM-001 — Reconcile compatibility manifest and parent trackers

**Issues:** #73, #93, #185  
**Priority:** P0  
**Depends on:** none

### Outcome

Make repository bookkeeping describe current `master`, not historical migration state.

### Implementation

1. Audit `compat/manim-v0.21.0.json` against current public exports and executable fixtures.
2. Classify each tracked symbol using explicit evidence-oriented states:
   - missing;
   - implemented-unqualified;
   - partial;
   - parity-qualified;
   - deferred;
   - intentional-divergence.
3. Reconcile plotting first because the manifest is known to lag `Axes`, number/coordinate planes, function plotting, polar plotting, and implicit-curve work.
4. Reconcile retained Text/Typst family-animation support next.
5. Update #73/#93 parent descriptions or checklists so already-landed architecture is not described as missing implementation.
6. Mark historical stacked PRs/issues as superseded where their feature delta landed through another branch.
7. Require every `parity-qualified` entry to point to a concrete canonical fixture/gate.

### Tests / gates

- manifest schema validation;
- public-export coverage test;
- validation that every qualified entry has an evidence identifier/path;
- no `blocked` entry for a public symbol whose required canonical acceptance fixture is already green.

### Exit criteria

- manifest and trackers agree with current `master`;
- remaining work can be derived from evidence rather than stale prose;
- no feature is promoted solely from unit implementation without parity evidence.

---

## ITEM-002 — Add manifest/tracker consistency CI

**Issues:** #73, #93, #185  
**Priority:** P0  
**Depends on:** ITEM-001

### Outcome

Prevent compatibility bookkeeping from drifting again.

### Implementation

1. Add a machine-readable mapping from manifest entries to qualification fixtures/gates where practical.
2. Validate that `parity-qualified` symbols have:
   - public export coverage;
   - a canonical fixture;
   - the expected semantic/raster/timeline qualification metadata.
3. Validate that tutorial examples marked `ready` remain literal upstream ManimCE v0.21 source except the `manim -> noon` import substitution.
4. Validate that `parity-qualified` tutorial examples have all required backend/timing/lifecycle evidence.
5. Fail CI on contradictory states rather than silently allowing manual drift.

### Exit criteria

A future implementation PR cannot claim support in one tracker while leaving another authoritative source contradictory.

---

## ITEM-003 — Build a deterministic #513 WebGL reproducer

**Issues:** #513, #512, #185  
**Priority:** P0  
**Depends on:** none

### Outcome

Turn the transient WebGL `RotationUpdater` raster failure into a deterministic layered regression.

### Implementation

1. Extract the smallest scene that still reproduces the forward host-updater failure.
2. Lock test inputs:
   - logical times/frame numbers around the failing sample;
   - viewport and DPR;
   - backend = WebGL;
   - the exact updater sequence and host-patch order.
3. Capture debug-only checkpoints for the moving object at:
   - committed host/runtime patch state;
   - execution mirror state;
   - incremental `FramePreparer` output;
   - GPU instance/upload state;
   - draw submission membership;
   - presented/read-back frame where available.
4. Record object/slot/session/generation identity and resolved transform/endpoints at each boundary.
5. Run the same checkpoints under WebGPU to establish the first differing layer rather than guessing from the final screenshot.
6. Cover at least frame-before, failing frame, frame-after, and a later reconverged frame so stale/late state is detectable.

### Tests / gates

The new regression should initially fail on WebGL without changing the existing raster tolerance and should demonstrate whether semantic/prepared/upload/presentation state first diverges.

### Exit criteria

The first divergent boundary is reproducible and observable in a focused test. No fix is required in this item.

---

## ITEM-004 — Fix the first divergent #513 layer

**Issues:** #513  
**Priority:** P0 blocker  
**Depends on:** ITEM-003

### Outcome

Restore WebGL/WebGPU equivalence without abandoning incremental execution/upload.

### Implementation

Apply the smallest fix at the first proven divergent layer from ITEM-003. Depending on evidence, likely classes of fix are:

- stale generation/sequence acceptance in the host mirror;
- dirty classification losing a committed transform;
- changed-slot range not propagated into prepared state;
- WebGL-specific instance-buffer update/range reuse bug;
- submission/presentation synchronization exposing a prior buffer generation.

Do **not** fix by:

- full snapshots every frame;
- full-scene upload;
- extra arbitrary browser delays;
- tolerance relaxation.

### Tests / gates

- ITEM-003 regression passes;
- existing `rotation-updater` raster ratchet passes unchanged on WebGPU and WebGL at the known failing sample;
- direct seek and forward playback agree;
- changed-slot counters remain local.

### Exit criteria

#513 can close with the root cause stated precisely and with a permanent regression test.

---

## ITEM-005 — Make exact-output validators pre-merge

**Issues:** #865, #851, #838, #185  
**Priority:** P0  
**Depends on:** none; can run in parallel with ITEM-003/004

### Outcome

Move specialized parity failures from post-merge discovery to PR qualification.

### Implementation

1. Identify source ownership for:
   - canonical Manim raster differential;
   - retained Text/Typst qualification;
   - host-updater qualification;
   - backend-specific WebGPU/WebGL fixtures.
2. Add PR path triggers for the owned code and fixture paths.
3. Add trigger-policy tests so PR and push filters cannot drift silently.
4. Reuse existing reference artifacts/tolerances; do not regenerate references as part of ordinary implementation PRs.
5. Keep expensive gates scoped but impossible to bypass for relevant changes.

### Exit criteria

A PR touching an owned semantic/render path must pass the relevant exact-output gate before merge.

---

# Phase B — P1 canonical execution and retained locality

## ITEM-006 — Finish canonical `SceneSpec` authoring and retire normal sidecar use

**Issues:** #367, #867, #61  
**Priority:** P1  
**Depends on:** current canonical mixed-ID work already landed

### Outcome

One authored semantic scene, one scene-global identity space, one normal execution input.

### Implementation

1. Make supported Python/Rust authoring construct canonical mixed scene objects directly through shared semantic handles/requests.
2. Ensure geometry and text source specs occupy the same scene-global object list and painter order.
3. Route family animation requests through the same canonical object/family identities.
4. Change normal browser authoring output to canonical `SceneSpec` without requiring `retained_document()` reconciliation.
5. Keep legacy geometry-document decode only as an isolated import/compatibility adapter.
6. Remove normal-path special retained ID allocation/ranges.
7. Remove duplicated geometry-vs-text painter-order reconciliation.
8. Add ratchet tests that reject reintroduction of obsolete sidecar-only fields/IDs in canonical authoring output.

### Tests / gates

- geometry → Text → geometry mixed-order fixture;
- text-only scene without empty legacy geometry document;
- re-run/mode-switch preserves one coherent identity graph;
- Rust/Python equivalent authored scenes canonicalize identically where inputs are equivalent;
- existing Typst/Text family-animation fixtures remain green.

### Exit criteria

No production execution path depends on `retained_document()` as the normal authoring contract; #367 can close or narrow to legacy-import cleanup only.

---

## ITEM-007 — Complete local structural `ExecutionDelta` consumption

**Issues:** #58, #61, #68  
**Priority:** P1  
**Depends on:** stable slots/tombstones already landed

### Outcome

Local structural edits remain local through execution consumers.

### Implementation

1. Inventory structural mutations still causing dense track shifts or whole-scene rebuilds.
2. Make create/remove/reorder/resource-binding changes emit explicit `ExecutionDelta` operations against stable slots/generations.
3. Update downstream mirror/render consumers to apply those deltas directly.
4. Preserve tombstones until reclamation is explicitly safe.
5. Add a compaction/reclamation policy that changes layout generation atomically and invalidates stale identities deterministically.
6. Never let compaction become ordinary-frame work.

### Tests / gates

- create/remove one object in a large scene touches O(local) execution records;
- stale slot/generation references are rejected;
- direct rebuild and incremental edit produce identical frame state;
- compaction preserves stable public semantic IDs even if internal slot positions change.

### Exit criteria

No remaining common structural operation requires dense historical track shifting or unrelated scene recompilation.

---

## ITEM-008 — Add resident family-member render state

**Issues:** #835, #362  
**Priority:** P1  
**Depends on:** canonical family identity already landed

### Outcome

Family-animation frame work scales with active/dirty members rather than total glyph/member count.

### Implementation

1. Add retained per-family/member render bindings keyed by stable family member identity.
2. Cache immutable member outline/resource handles once resolved.
3. Keep compact mutable member progress/reveal/fill state separately from immutable resources.
4. Track changed members as indices/ranges, not by rebuilding a temporary family representation.
5. Materialize path-level glyph state only for members currently requiring path animation.
6. Upload only affected vector/instance ranges when topology is unchanged.
7. Return members to ordinary glyph-atlas rendering when path-level animation is inactive.
8. Keep family timing/order calculation in Rust.

### Tests / gates

- 10k-member retained text family with one/few changing members;
- no outline/resource rematerialization for unchanged members;
- no unrelated painter-order rebuild;
- Create/Uncreate/Write/Unwrite/DrawBorderThenFill preserve exact timing and direct-seek equivalence;
- counters report member updates, outline materializations, uploaded bytes/ranges.

### Exit criteria

After warmup, family progress frames are approximately O(active/dirty members), not O(total family members).

---

## ITEM-009 — Make retained text/mixed preparation dirty-object incremental

**Issues:** #362, #361, #114  
**Priority:** P1  
**Depends on:** ITEM-008 can be developed in parallel but both must converge

### Outcome

Static text-heavy scenes do effectively zero preparation/upload work after warmup.

### Implementation

1. Introduce/complete retained per-object prepared records in the shared renderer/text-renderer path:
   - semantic object/slot identity;
   - content handle;
   - painter position;
   - prepared glyph/instance ranges;
   - vector/outline references;
   - last transform/style/presence/reveal state.
2. Consume object-level `FrameChanges` directly.
3. For transform/style/opacity changes, mutate only affected instance records.
4. For presence changes, activate/deactivate only affected records.
5. For reveal/morph/lane transitions, invalidate only that object's materialized path state.
6. Preserve resident atlas/raster resources across ordinary frames.
7. Preserve retained painter order; rebuild global order only for structural edits.
8. Upload dirty ranges only.
9. Integrate ITEM-008 family-member state into this same resident model rather than a parallel scratch path.

### Tests / gates

Deterministic benchmark cases:

- 10k+ static visible glyphs, no changes;
- one translating text object;
- one style/opacity-changing object;
- one reveal/outline object;
- one mixed Text+geometry family animation.

After warmup require:

- zero/effectively-zero unchanged preparation;
- object-local reprepare/update;
- zero global order rebuild on non-structural changes;
- zero warm atlas misses when raster size is unchanged;
- WebGPU/WebGL parity unchanged.

### Exit criteria

Renderer preparation and upload preserve execution locality end to end.

---

# Phase C — P1 core Mobject / VMobject semantics

All six items below belong under #74. They should be separate PRs so downstream feature work can consume each tranche as soon as it lands.

## ITEM-010 — Core family traversal, indexing, and mutation

**Issues:** #74  
**Depends on:** ITEM-006 preferred for clean canonical ownership

### Implementation

1. Define shared family traversal order and recursive/non-recursive query semantics.
2. Implement stable indexing/slicing behavior over semantic family members.
3. Implement `add`, `remove`, insertion, reorder, and equivalent family mutations in shared semantics.
4. Preserve child semantic identity when merely reordered/reparented where Manim-observable semantics permit.
5. Define painter-order consequences explicitly.
6. Keep Python wrappers as thin handle operations, not a second family engine.

### Tests

- nested family traversal;
- duplicate-add/remove edge cases;
- deterministic ordering after insert/reorder;
- slicing/indexing against ManimCE fixtures;
- local structural-delta behavior in large families.

### Exit criteria

Downstream composites can rely on one shared, stable family model.

---

## ITEM-011 — Bounds and critical-point query vocabulary

**Issues:** #74  
**Depends on:** ITEM-010 where family bounds aggregate children

### Implementation

1. Complete center, left/right/top/bottom, corner, edge-center, and critical-point queries.
2. Define family bounds as transformed semantic bounds over present relevant members.
3. Provide width/height/depth-compatible observable getters.
4. Expose start/end/center queries for applicable path-like objects through shared geometry APIs.
5. Cache or incrementally invalidate bounds so repeated queries do not force geometry regeneration.
6. Keep exact path/resource math in Rust; Python/JS return values only.

### Tests

- translation/rotation/nonuniform scale/reflection;
- nested families;
- empty/degenerate objects;
- transformed curves and stroke-sensitive cases where Manim semantics require them;
- comparison with canonical Manim values.

### Exit criteria

Layout and transform helpers no longer need frontend-local bounding-box logic.

---

## ITEM-012 — Generic transform/matching vocabulary

**Issues:** #74  
**Depends on:** ITEM-011

### Implementation

Implement shared observable semantics for:

- `set_x`, `set_y`, `set_z`, `set_coord`;
- `match_x/y/z`, `match_width/height`, related `match_*` helpers;
- `stretch`, `stretch_to_fit_width/height`;
- `flip`;
- `rotate_about_origin`;
- center-preserving Manim `scale` routing through the shared semantic operation.

Prefer transform/state mutation over destructive point-array rewriting. Only produce new immutable geometry where Manim-observable behavior genuinely requires geometry transformation.

### Tests

- pivot semantics;
- negative factors/reflections;
- nested family transforms;
- chaining order;
- `.animate` target-state capture for each new mutator;
- direct seek = forward playback.

### Exit criteria

Common transform helpers share one native semantic path and automatically participate in generic animation.

---

## ITEM-013 — Complete layout helpers

**Issues:** #74  
**Depends on:** ITEM-011, ITEM-012

### Implementation

Complete Manim-compatible option behavior for:

- `next_to`;
- `align_to`;
- `to_edge`;
- `to_corner`;
- `arrange`;
- `arrange_in_grid`.

Implementation must use shared semantic bounds and transforms. Do not duplicate vector/bounds math in Python.

### Tests

Cover buff, aligned edge, direction normalization, center placement, row/column counts, flow order, unequal child sizes, nested groups, and chained layout operations against ManimCE.

### Exit criteria

Representative upstream layout examples execute without Noon-specific positioning edits.

---

## ITEM-014 — Target/state/copy/become/replace semantics

**Issues:** #74, #82  
**Depends on:** ITEM-010 through ITEM-012

### Implementation

1. `generate_target`: snapshot compatible semantic state with clear target identity rules.
2. `save_state`/restore-compatible state: capture observable transform/style/content/family state required by Manim semantics.
3. `copy`: allocate fresh semantic identity while sharing immutable resources until mutation requires replacement.
4. `become`: replace observable content/state while preserving the receiving object's identity.
5. `replace`: match geometry/layout semantics without aliasing identities incorrectly.
6. Ensure resource/family changes are expressed as localized canonical mutations.

### Tests

- copy independence with resource sharing;
- target animation;
- save/restore round-trip;
- become across compatible and differing resource types;
- replace with/without stretch where applicable;
- family identity and painter order.

### Exit criteria

Generic transform-family implementations can build on these primitives rather than special-case object cloning in Python.

---

## ITEM-015 — VMobject/path query facade

**Issues:** #74, #78  
**Depends on:** shared `VectorPath` infrastructure

### Implementation

Expose the shared path-level observable queries needed by downstream geometry/animations, such as point/proportion, subpath, curve-count/length-related operations, start/end/tangent queries, and topology information required for matching/morphing.

Requirements:

- renderer-independent;
- deterministic;
- no exposure of Manim's mutable point-array internals as Noon's architecture;
- no Python reconstruction of Bézier/path math.

### Tests

Analytic line/arc/Bezier cases, degenerate subpaths, closed paths, transformed paths, and direct comparison with Manim-visible query results.

### Exit criteria

#78/#82 can consume a stable shared path-query contract.

---

# Phase D — P1 text / math / numeric surface

## ITEM-016 — Finish native `Text` / `MarkupText`

**Issues:** #83, #364  
**Depends on:** ITEM-006; benefits from ITEM-009

### Outcome

Complete the common public native text surface without introducing a special renderer.

### Implementation

1. Normalize constructor options into the shared native text compiler/layout backend.
2. Cover common font family, size, weight, style, line spacing, alignment, and wrapping semantics.
3. Preserve stable source/glyph/cluster/part metadata through shaping.
4. Implement substring/range styling and color/gradient ranges against source/cluster identities.
5. Keep normal output as shaped glyph runs/atlas-backed data.
6. Generate/cached outlines lazily only for path-level operations.
7. If `Paragraph` cleanly composes from the same resource model, add it here or immediately after as a thin semantic composition.

### Tests

Include ASCII, Unicode, combining marks, ligatures, RTL/bidirectional samples where supported, multiline alignment, font fallback, substring coloring, nonuniform transforms, and family Write/Create.

### Exit criteria

`Text`/`MarkupText` are public, retained, backend-neutral, and qualified for a representative Manim subset without path-per-glyph steady-state rendering.

---

## ITEM-017 — Complete `MathTypst` grouping/select/index/color semantics

**Issues:** #83  
**Depends on:** current Typst backend

### Implementation

1. Represent `{{ ... }}` groups as stable source-part identities during compile/lowering.
2. Implement `select(str | int)` over those identities.
3. Make indexing/slicing/coloring resolve through shared part metadata, not geometry position guesses.
4. Preserve identities through family animation and compatible resource reuse.
5. Define diagnostics for ambiguous/not-found selections consistently.

### Tests

Nested/repeated groups, repeated substrings, index selection, color-by-part, transforms, Write/Unwrite, direct seek, and exact raster fixtures.

### Exit criteria

MathTypst can participate in part-based authoring and matching without frontend heuristics.

---

## ITEM-018 — Implement real LaTeX `Tex` / `MathTex`

**Issues:** #83, #365, #369  
**Depends on:** ITEM-006; compiler-registry/cache contract may land first

### Architecture

```text
Tex / MathTex source
      -> deterministic LaTeX compiler adapter
      -> normalized glyph/vector decorations + source parts
      -> shared TextResource
      -> existing retained text/vector renderer
```

### Implementation

1. Define backend configuration key: source, environment/template, compiler/options, font/resource dependencies.
2. Invoke a real LaTeX pipeline at authoring/compile time; do not silently translate TeX to Typst.
3. Normalize compiler output into the existing backend-neutral `TextResource` representation.
4. Preserve source/part identity needed for indexing/coloring/matching.
5. Content-address compiler artifacts and dependencies for reuse.
6. Surface clear diagnostics for missing packages/fonts/compiler failures.
7. Define browser resource-transfer policy so normal playback does not require a live Python/LaTeX compiler.
8. Reuse ordinary glyph/vector/family render paths.

### Tests

- representative text/math expressions;
- fractions, radicals, scripts, environments used by Manim fixtures;
- repeated identical compilation reuses cache;
- deterministic normalized resource output;
- Write/Unwrite and transforms;
- browser playback after authoring worker/compiler is idle.

### Exit criteria

`Tex`/`MathTex` have real LaTeX semantics but no separate scene or renderer architecture.

---

## ITEM-019 — Add `DecimalNumber`, `Integer`, and `Variable`

**Issues:** #83, #368  
**Depends on:** ITEM-016; ITEM-018 only if exact MathTex label behavior requires it

### Implementation

1. Match common formatting options: decimal places, sign, grouping, units where applicable, digit replacement rules, and edge cases.
2. Represent displayed content through the shared text resource model.
3. On numeric value change, compile/replace only the affected text resource/content binding.
4. Preserve semantic object identity and compatible transform/style state.
5. Reuse unchanged glyph/resource content where the compiler/resource cache permits it.
6. Wire value updates into deterministic timeline/reactive paths rather than per-frame Python where values are native signals.

### Tests

Formatting boundaries, negative/zero values, carry between digit lengths, animated value changes, style preservation, direct seek, and resource-locality counters.

### Exit criteria

Changing a number does not delete/rebuild an unrelated scene or reset object identity.

---

## ITEM-020 — Text/math animation and matching qualification

**Issues:** #83, #82, #705  
**Depends on:** ITEM-016 through ITEM-019 as applicable

### Implementation

1. Qualify Create/Uncreate/Write/Unwrite across native Text, Typst, MathTypst, Tex, and MathTex.
2. Implement/finish `TransformMatchingTex` using stable source/part/member identities plus explicit fallback behavior.
3. Verify color/index selection survives compatible transforms/matching.
4. Add source-equivalent Manim fixtures at begin/25/50/75/final/exact-end and direct seek.
5. Preserve Rust-owned family timing/member ordering.

### Exit criteria

Text/math promotion in the compatibility manifest is backed by exact semantic/raster/timing evidence, not constructor smoke tests.

---

# Phase E — P2 coordinate plotting completion

## ITEM-021 — Replay the retained implicit-curve delta from #860 onto current master

**Issues:** #85, #860  
**Depends on:** current plotting core; do not merge the historical stack wholesale

### Implementation

1. Extract only the still-missing implicit-curve feature delta from #860.
2. Rebase/reimplement against current coordinate/path APIs.
3. Keep adaptive deterministic isoline/contour generation in shared Rust geometry.
4. Let Python evaluate arbitrary scalar functions only when Rust requests authoring-time samples.
5. Emit ordinary retained `VectorPath` output; no runtime Python callback.
6. Define deterministic contour stitching/order.
7. Handle NaN/discontinuity/empty-domain cases explicitly.
8. Keep tolerance in semantic/scene space rather than browser/DPR-dependent pixels unless a documented API requires otherwise.

### Tests

Circle/ellipse-like zero sets, saddle/intersections, disconnected contours, no-contour case, discontinuity/NaN, determinism, transformed axes, WebGPU/WebGL raster parity.

### Exit criteria

`ImplicitFunction`/`Axes.plot_implicit_curve` run through the normal retained plotting architecture on current master.

---

## ITEM-022 — Finish plotting tips, labels, scales, and helper surface

**Issues:** #85, #77, #83  
**Depends on:** ITEM-012/013, #77 arrow/tip work, relevant text labels

### Implementation

1. Audit remaining `Axes`, `NumberLine`, `NumberPlane`, `ComplexPlane`, `PolarPlane`, `ParametricFunction`, `FunctionGraph` API gaps.
2. Complete tick/tip construction using shared geometry/line/arrow semantics.
3. Complete coordinate/axis labels using shared Text/MathText resources.
4. Introduce a shared coordinate-scale abstraction for linear/log/nonlinear mapping; no Python loop as the semantic implementation.
5. Complete helper methods (`c2p`, `p2c`, graph helpers, vertical/horizontal lines, area/riemann-style helpers where in scope) by composing retained primitives.
6. Ensure function sampling occurs at authoring/compile time for static functions and uses typed reactive/native paths for truly dynamic native data.

### Tests

Literal upstream plotting examples, transformed/scaled axes, labels, nonlinear/log samples, discontinuities, direct-seek animation cases, and backend parity.

### Exit criteria

#85 becomes primarily a qualification/coverage parent rather than an architecture implementation blocker.

---

## ITEM-023 — Plotting parity qualification and manifest promotion

**Issues:** #85, #73, #93, #185  
**Depends on:** ITEM-021, ITEM-022

### Implementation

1. Select representative literal ManimCE v0.21 tutorial/reference scenes.
2. Keep source byte-equivalent except import substitution for `ready` examples.
3. Run semantic, raster, timing, lifecycle, direct-seek, WebGPU, and WebGL gates as applicable.
4. Promote manifest entries only after evidence is green.
5. Remove obsolete `blocked` statuses and link canonical fixtures.

### Exit criteria

The manifest accurately reflects plotting support and users can find source-equivalent gallery evidence.

---

# Phase F — P2 Graph / DiGraph

## ITEM-024 — Rebase and land #866 shared topology core

**Issues:** #87, #866  
**Depends on:** none beyond current shared core

### Implementation

Preserve the #866 design while replaying it onto current master:

- monotonic stable vertex IDs;
- monotonic stable edge IDs;
- deterministic insertion order;
- endpoint validation;
- undirected duplicate normalization;
- directed orientation semantics;
- per-vertex incident-edge index with O(degree) access;
- transactional vertex/edge removal.

IDs must not be silently reused during the graph/scene lifetime.

### Tests

Duplicate edges, directed vs undirected, remove vertex with many incident edges, deterministic iteration, invalid endpoints, repeated local mutation.

### Exit criteria

Topology is a shared Rust primitive independent of Python/NetworkX/rendering.

---

## ITEM-025 — Bind Graph topology to a semantic family with explicit positions

**Issues:** #87  
**Depends on:** ITEM-010, ITEM-024

### Implementation

1. Represent Graph as a semantic family/composite tied to the stable topology IDs.
2. Start with explicit vertex positions only.
3. Instantiate vertices as ordinary existing Mobjects.
4. Instantiate edges as ordinary Line/Arrow-family geometry according to graph direction/config.
5. Define deterministic family/painter order for vertices, edges, labels.
6. Preserve stable child object IDs across position/style updates.
7. Do not add a graph-specific renderer lane.

### Tests

Explicit triangle/path/directed graph, style updates, reorder-independent topology identity, copy/transform where supported.

### Exit criteria

A graph with explicit positions renders entirely through existing retained primitives.

---

## ITEM-026 — Add incident-edge dependency updates and local graph mutations

**Issues:** #87, #69/#70 where structural runtime mutation overlaps  
**Depends on:** ITEM-007, ITEM-025

### Implementation

1. Register native dependencies from vertex position to only its incident edge geometry.
2. Moving one vertex marks only that vertex and incident edges dirty.
3. Add/remove vertex/edge emits local semantic/execution deltas.
4. Preserve unaffected object/resource identities.
5. Reject stale topology IDs deterministically after removal.
6. Expose locality counters for changed vertices/edges/slots.

### Tests

Large sparse graph: move degree-3 vertex and prove update cost/counters scale with degree, not total edges. Add/remove operations must match full rebuild output while touching local execution state.

### Exit criteria

Graph dynamics honor Noon's O(local dependencies) architecture.

---

## ITEM-027 — Add thin public Graph/DiGraph API, labels, and layouts

**Issues:** #87  
**Depends on:** ITEM-025/026; text for labels

### Implementation

1. Add thin Python adapters for constructor, vertex/edge configs, add/remove operations, position access, and layout selection.
2. Implement deterministic shared layouts required for common parity, or explicitly use an author-time optional compatibility provider where a layout algorithm is intentionally external.
3. Never make NetworkX a browser/runtime dependency.
4. Add vertex/edge labels through shared text resources.
5. Preserve topology IDs independent of layout recomputation.

### Tests

Common Manim Graph/DiGraph examples, deterministic seeds/layout output where specified, label/style behavior, local mutation, browser playback without authoring dependencies.

### Exit criteria

Representative Manim Graph/DiGraph scenes are source-equivalent and retained.

---

# Phase G — P2 remaining common-2D feature families

These items should consume the core semantics above instead of creating new local abstractions.

## ITEM-028 — Lines, arrows, dashed geometry (#77)

### Implementation

- complete Line/DashedLine/Arrow/DoubleArrow/vector-like endpoint semantics;
- shared tip geometry/orientation/scale behavior;
- deterministic dashed segmentation/resource generation;
- local endpoint updates without rebuilding unrelated scene state;
- Python/JS as thin adapters.

### Tests / exit

Canonical endpoint/tip/buff/dash fixtures, transforms, Create/Transform, plotting tips, WebGPU/WebGL parity. #77 can close when public surface + exact qualification is complete.

---

## ITEM-029 — Vector-path and boolean geometry (#78)

### Implementation

- finish path composition/query APIs using ITEM-015;
- deterministic boolean operations in shared geometry;
- immutable output resources with explicit cache keys;
- topology/morph compatibility plans computed outside the steady-state frame loop;
- no frontend Bézier/boolean implementation.

### Tests / exit

Boolean union/intersection/difference on convex/concave/disconnected cases, winding/fill rules, transforms, morph endpoints, determinism, malformed/degenerate paths.

---

## ITEM-030 — Basic geometry public breadth (#76)

### Implementation

Expose/finish the shared Rust geometry families through thin public adapters, including arc/sector/annulus/polygon/polygram/star/rounded-rectangle/surrounding helpers as tracked. Reuse ordinary path/primitive renderer lanes.

### Tests / exit

Constructor/default/query parity, exact Manim raster fixtures, public namespace coverage, no frontend geometry reconstruction. Promote manifest entries only after qualification.

---

## ITEM-031 — Transform family and matching (#82)

### Implementation

Build Transform/replacement/matching variants on ITEM-014 state/target semantics and shared morph/resource plans. Keep lifecycle/timing ownership native and define explicit fallback for incompatible topology/parts.

### Tests / exit

Begin/mid/end/exact-end/direct-seek fixtures, copy/replace identity, family matching, text matching integration, no per-frame Python interpolation.

---

## ITEM-032 — Deterministic animation breadth (#80)

### Implementation

Audit remaining deterministic built-ins and lower them into Rust-owned timeline/family/state operations. Built-ins whose result depends only on timeline/state must not execute Python callbacks per frame.

### Tests / exit

Lifecycle boundaries, rate functions, lag/family behavior, zero/short durations, chained animations, direct seek = forward playback.

---

## ITEM-033 — Dynamic / indication features (#81)

### Implementation

Split features into:

- deterministic time-only effects -> native timeline channels;
- dependency-driven dynamic behavior -> typed reactive graph;
- true host callbacks -> explicit host callback contract.

Do not treat all dynamic APIs as Python updaters. Reuse retained resources and dirty-closure invalidation.

### Tests / exit

Static-neighbor stress scene proving one dynamic effect does not reprepare all objects; exact lifecycle/seek tests for deterministic effects.

---

## ITEM-034 — SVG / image assets (#79)

### Implementation

- author/compile-time decode into immutable shared resources;
- content-addressed asset cache;
- preserve source dimensions/viewBox/image sampling semantics;
- no per-frame SVG/image decoding;
- local resource replacement for hot reload;
- ordinary painter/transform/style paths.

### Tests / exit

Repeated asset reuse, transform-only zero decode, opacity/z-order, malformed asset diagnostics, browser asset transfer, parity fixtures.

---

## ITEM-035 — Composite objects, matrices, and tables (#84)

### Implementation

Compose shared family, geometry, layout, and text primitives. Preserve stable member identities/indexing so row/column/entry selection and animation work naturally. Do not create matrix/table-specific renderer semantics.

### Tests / exit

Matrix/Table construction, entry indexing, row/column highlighting, brackets/labels, layout with differing entry sizes, transforms and Write/Create where applicable.

---

## ITEM-036 — Scene and camera public parity (#89)

### Implementation

Map public scene/camera operations into canonical scene camera state and native execution. Keep camera changes compatible with retained viewport/spatial visibility. Define fixed-in-frame behavior in a way that can later share the 3D camera model.

### Tests / exit

Camera frame transforms, scene add/remove/order/lifecycle, visibility invalidation, direct seek, browser backend parity, no manual frontend-only camera state.

---

## ITEM-037 — Fields / probability helpers (#88)

### Implementation

Compile static sampled fields/distributions into retained primitives/resources. Dynamic native fields should use typed reactive inputs and dirty dependencies, not Python per-frame sampling. Reuse plotting/geometry/text primitives.

### Tests / exit

Representative field/probability scenes, deterministic sampling, local updates, direct seek, large static field warm-frame locality.

---

# Phase H — P2 measured performance work

## ITEM-038 — Instrument the remaining cold-first-run path

**Issues:** #642, #868  
**Depends on:** current correct-first-engine topology

### Outcome

Replace the obsolete startup story with measured current bottlenecks.

### Implementation

Add stable phase markers for:

1. Run click/request;
2. author worker ready;
3. source evaluation start/end;
4. canonical `SceneSpec` ready;
5. selected runtime worker ready;
6. WASM fetch/instantiate/initialization;
7. first evaluated/prepared frame;
8. first submitted frame;
9. first presented frame.

Measure cold vs warm and retained vs legacy paths. Report worker-specific WASM/JS initialization cost and byte duplication.

### Tests / exit

Instrumentation must not materially alter startup ordering. #642 should be rewritten/narrowed to only measured dominant phases after this data exists.

---

## ITEM-039 — Remove measured duplicate WASM/worker initialization

**Issues:** #642  
**Depends on:** ITEM-038

### Implementation

Optimize only phases shown to dominate. Candidate work may include shared fetched module bytes, eliminating unnecessary worker initialization, splitting code only where a worker does not need it, or overlapping independent initialization with authoring.

Do not perform speculative bundle splitting without phase/byte evidence.

### Exit criteria

A before/after cold-start trace demonstrates the improvement and initial page load remains deferred.

---

## ITEM-040 — Finish first-frame upload/resource reuse locality

**Issues:** #569 and related first-frame/resource work  
**Depends on:** resident renderer architecture

### Implementation

1. Identify resources/instances uploaded redundantly at first presentation or engine re-run.
2. Reuse resident immutable geometry/text resources where identity/generation proves compatibility.
3. Separate viewport projection from all-live residency.
4. Keep camera/visibility-only changes from invalidating unrelated resources.
5. Add counters for resource uploads, bytes, cache hits, candidate count, submitted instances.

### Tests / exit

Large scene with small visible fraction; panning out/back in must not cause geometry/text reconstruction solely due to visibility. Preparation/submission scales with candidates while resources remain resident.

---

## ITEM-041 — Profile and ratchet stable camera/draw-command submission

**Issues:** #847, #569  
**Depends on:** ITEM-040 preferred

### Implementation

Instrument command/draw submission under static camera, camera-only movement, and one-object changes. Determine whether command encoding/submission itself is a meaningful O(total) cost after visibility projection. Optimize only if counters/profiles justify it, using retained batches/bindings/indirect-like structures where compatible with backend constraints.

### Exit criteria

#847 becomes a measured decision: either a targeted optimization lands with a structural regression gate, or data explicitly closes/deprioritizes it.

---

# Phase I — P3 native interaction and live authoring

## ITEM-042 — Land/refit the native input-envelope contract from #869

**Issues:** #69, #869  
**Depends on:** current native reactive source model

### Implementation

Preserve the proposed contract:

- sampled state keyed/coalesced by exact source identity;
- discrete events assigned monotonically increasing host-ingress sequence numbers;
- value-kind/finiteness validation at ingress;
- no timeline time embedded into the raw ingress envelope.

Rebase the minimal contract onto current master and add serialization/ordering tests.

### Exit criteria

A small backend-neutral input envelope is stable before DOM/worker policy is added.

---

## ITEM-043 — Wire DOM input through bounded queues into native reactive execution

**Issues:** #69  
**Depends on:** ITEM-042

### Implementation

1. DOM adapters translate pointer/keyboard/control sources into typed envelopes.
2. Maintain separate semantics for:
   - sampled state: latest-value coalescing;
   - discrete events: ordered delivery.
3. Add bounded queue/backpressure policy with explicit overflow counters/behavior.
4. Forward envelopes to the engine worker without frontend semantic evaluation.
5. Apply sampled/event values to native typed reactive sources.
6. Invalidate only the dependent reactive closure/affected render state.
7. Instrument ingress, coalesced, queued, dropped/overflowed, applied sequence counts.

### Browser tests

- burst pointer movement;
- interleaved discrete events and sampled updates;
- ordering under worker delay;
- finite-value rejection;
- scene seek/re-run boundary behavior;
- large static scene with one input-driven object proving no all-scene scan.

### Exit criteria

Browser input drives retained native state deterministically within the defined ingress contract.

---

## ITEM-044 — Add persistent authoring/runtime session identity

**Issues:** #846  
**Depends on:** ITEM-006, ITEM-043

### Implementation

1. Define session/revision identity independent of individual scene object IDs.
2. Tag authoring results, execution deltas, input streams, and renderer visibility state with authoritative session/revision generations.
3. Reject stale worker/input/render responses from prior runs deterministically.
4. Preserve compatible runtime/resource state across re-run when semantic identity and dependency versions prove reuse is safe.
5. Make reset/restart semantics explicit.

### Tests / exit

Rapid Run/Run, stale worker reply, stale input event, compatible edit reuse, incompatible reset, and deterministic resource lifetime tests.

---

## ITEM-045 — Add local structural live-edit operations

**Issues:** #70, #58  
**Depends on:** ITEM-007, ITEM-044

### Implementation

Expose supported live create/remove/reorder/rebind operations as canonical semantic patches lowered to local `ExecutionDelta`s. Update dependencies/spatial index/painter order/resource bindings incrementally. Do not recompile the full scene for one local structural edit.

### Tests / exit

Edit a large running scene by adding/removing/reordering one object; verify full-rebuild equivalence and local counters. Stale references must be rejected by generation/session identity.

---

## ITEM-046 — Add localized resource replacement / hot reload

**Issues:** #64, #368, #369  
**Depends on:** ITEM-044/045; compiler/resource caches

### Implementation

1. Version resource dependencies explicitly.
2. On source/font/image/geometry content change, rebuild only the affected compiled resource.
3. Emit localized canonical content-binding/resource invalidation.
4. Preserve semantic object identity and unrelated resident resources.
5. Invalidate atlas/vector/cache entries only when their dependency key changes.
6. Deduplicate identical immutable compiled resources across objects/runs.

### Tests / exit

Edit one text source/font/image in a large scene; only affected resources recompile/reupload. Reverting to prior content can hit the content-addressed cache. No full scene restart is required where the edit is structurally compatible.

---

# Phase J — P4 3D / vector-space

## ITEM-047 — Refresh and land canonical 3D semantic foundations

**Issues:** #90, #698  
**Priority:** P4  
**Depends on:** common 2D core should be stable enough not to fork semantics

### Outcome

Define 3D data in the same canonical scene/execution architecture before adding renderer breadth.

### Implementation

1. Refresh #698 against current object/resource/camera APIs.
2. Define explicit world transforms compatible with existing semantic IDs and family structure.
3. Define canonical 3D camera/projection state in shared Rust semantics.
4. Define immutable mesh resource handles with positions/normals/indices and required material metadata.
5. Define mesh bounds for shared spatial/visibility infrastructure.
6. Ensure transforms/camera are timeline/reactive properties using existing execution machinery.
7. Keep Python/JS as constructors/adapters only; they must not own projection or mesh generation.

### Tests

World/local transform composition, camera projection math, mesh resource validation, deterministic serialization/canonicalization, direct seek of object/camera tracks.

### Exit criteria

A renderer-independent canonical 3D scene can be evaluated without inventing a separate 3D runtime.

---

## ITEM-048 — Add depth-aware mesh lane to the existing renderer

**Issues:** #90, #699  
**Depends on:** ITEM-047

### Implementation

1. Extend the existing renderer/device/surface with a retained mesh pipeline and depth buffer.
2. Resolve mesh resources once and retain GPU buffers by resource identity.
3. Update object transforms/material instance state through dirty ranges.
4. Use canonical camera matrices from shared frame state.
5. Preserve existing 2D/text painter semantics; define explicit composition rules between 2D/fixed-in-frame and depth-tested 3D.
6. Add viewport/depth attachment resize handling without resource-wide rebuild.
7. Keep WebGPU/WebGL capability differences explicit; do not create different semantics per backend.

### Tests

- first `ThreeDScene` fixture with two overlapping depth-separated objects;
- camera rotation;
- transform-only zero mesh reupload;
- resize/DPR;
- direct seek;
- backend parity where supported.

### Exit criteria

The first real ThreeDScene renders through the same resident engine/renderer with a mesh/depth lane, not a second renderer architecture.

---

## ITEM-049 — Expand 3D surfaces, lighting, solids, and fixed-in-frame behavior

**Issues:** #90 follow-ups  
**Depends on:** ITEM-048

### Implementation order

1. parametric surfaces / mesh generation;
2. normals/material basics;
3. deterministic lighting model and light state;
4. common solids;
5. fixed-in-frame/fixed-orientation objects using the shared camera contract;
6. transparency/order policy;
7. vector-space helpers and 3D coordinate systems.

Each slice should be a separate PR and must reuse canonical semantic IDs, immutable mesh resources, native execution, and the same GPU device/surface.

### Exit criteria

#90 can be decomposed into parity/qualification children rather than carrying unresolved renderer architecture.

---

# 1. Recommended execution sequence

The dependency-critical path is:

```text
ITEM-001 -> ITEM-002
ITEM-003 -> ITEM-004
ITEM-006 -> ITEM-010 -> ITEM-011 -> ITEM-012 -> ITEM-013/014 -> ITEM-015
ITEM-008 + ITEM-009
ITEM-016 -> ITEM-017/018/019 -> ITEM-020
ITEM-021 -> ITEM-022 -> ITEM-023
ITEM-024 -> ITEM-025 -> ITEM-026 -> ITEM-027
ITEM-038 -> ITEM-039
ITEM-042 -> ITEM-043 -> ITEM-044 -> ITEM-045 -> ITEM-046
ITEM-047 -> ITEM-048 -> ITEM-049
```

Recommended practical order on `master`:

1. ITEM-001 repository truth;
2. ITEM-003/004 #513 correctness blocker;
3. ITEM-005 pre-merge qualification;
4. ITEM-006 canonical sidecar retirement;
5. ITEM-008/009 renderer/family locality;
6. ITEM-010..015 core Mobject/VMobject semantics;
7. ITEM-016..020 text/math/numeric surface;
8. ITEM-021..023 plotting completion;
9. ITEM-024..027 Graph/DiGraph;
10. ITEM-028..037 remaining common-2D families, with independent lanes parallelized where dependencies permit;
11. ITEM-038..041 measured performance work;
12. ITEM-042..046 native interaction/live editing;
13. ITEM-047..049 3D.

---

# 2. Parallel work lanes

Once P0 correctness is under control, the following can proceed mostly independently:

| Lane | Items | Main ownership |
| --- | --- | --- |
| Canonical authoring/execution | 006-007 | core/execution/authoring |
| Renderer locality | 008-009, 040-041 | renderer/text renderer |
| Core object semantics | 010-015 | shared semantic/geometry core |
| Text backends/API | 016-020 | text compiler/resource + thin frontend |
| Plotting | 021-023 | geometry/plotting + qualification |
| Graph | 024-027 | topology/semantic family |
| 2D breadth | 028-037 | shared feature families |
| Startup | 038-039 | browser worker/bootstrap |
| Interaction | 042-046 | input/reactive/session/execution |
| 3D | 047-049 | shared 3D semantics + renderer mesh lane |

Parallel PRs must avoid changing the same architectural axis. In particular:

- do not combine dirty-render work with canonical authoring migration;
- do not combine LaTeX backend implementation with atlas/cache eviction;
- do not combine Graph topology with layout/frontend breadth;
- do not combine input-envelope schema with DOM/session/hot-reload policy;
- do not begin a second 3D execution or renderer architecture while 2D core work continues.

---

# 3. Per-PR checklist

Every implementation PR should answer these questions in its description:

- **Semantic owner:** Which shared Rust type/module owns the behavior?
- **Identity:** Which semantic/resource/family IDs must remain stable?
- **Incrementality:** What exactly becomes dirty, and what explicitly does *not* become dirty?
- **Seek:** Does direct seek produce the same state as forward execution?
- **Frontend:** Is Python/JS merely adapting to the shared operation?
- **Resources:** Are immutable resources reused instead of regenerated?
- **Renderer:** Is new renderer specialization actually necessary, or can existing lanes render the result?
- **Performance:** What counter/structural test prevents O(total) regression?
- **Parity:** Which ManimCE v0.21 fixture demonstrates observable equivalence?
- **Backends:** Which WebGPU/WebGL/native paths require qualification?
- **Bookkeeping:** Which manifest/tracker entry is updated when the gate is green?

A PR that cannot answer these yet is usually still at design/prototype stage and should not promote compatibility status.

---

# 4. Completion target

The roadmap is complete when the repository has moved from “broad retained architecture with remaining compatibility seams” to:

- one canonical authored/executed semantic scene;
- stable identities and local structural/property/resource deltas;
- retained rendering proportional to dirty + visible work;
- full common-2D object/text/plot/graph/composite breadth backed by ManimCE v0.21 evidence;
- measured, bounded startup/cache behavior;
- native input/live editing using the same reactive/execution architecture;
- 3D added as a mesh/depth capability of the existing retained engine rather than as a parallel system.

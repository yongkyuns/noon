# Renderer locality and caching

## Purpose

Noon should borrow the **work-avoidance principles** used by high-performance retained/cached drawing systems without copying Canvas-specific implementation choices.

The architectural goal is simple:

> Keep expensive renderable representations resident, and make ordinary animation update only the smallest mutable presentation state that actually changed.

For Noon, that means preserving vector/glyph/resource fidelity while driving frame cost toward visible and dirty work rather than total scene complexity.

This document records the current architecture, the remaining gaps, and the design rules that should guide renderer work.

Related trackers: #68, #362, #569, #737, #741, #835, #847.

## Current foundation

Noon already implements most of the retained-resource side of this model.

### Geometry

`FramePreparer` retains packed analytic/path state across frames and already supports:

- packed primitive instance records;
- dirty instance ranges;
- bounded tessellated-path caching;
- incremental unique-path replacement;
- retained path vertex/index arenas with free-range reuse;
- painter-order batching and packed mega-mesh submission;
- explicit full rebuild/compaction barriers rather than routine repacking.

A transform/style/reveal change therefore does not inherently require regenerating unrelated path geometry.

### Text

The retained text renderer already separates steady-state glyph rendering from path-level behavior:

- ordinary glyphs use a GPU glyph atlas;
- glyph outlines are extracted lazily;
- outline and stroked-outline paths are cached;
- outline residency is bounded and instrumented;
- text resources/font resources remain immutable renderer-resolved inputs;
- glyph/vector items share the same semantic `ObjectId` painter stream.

This is preferable to generic per-text bitmap caching because it preserves resolution-independent geometry and reuses glyph-level resources.

### Runtime locality

Runtime execution already carries local change information and stable execution identity. `ExecutionSpatialIndex` provides incrementally maintained conservative bounds, painter-ordered viewport queries, and point-query candidates.

The retained-renderer target is therefore not to invent more caches or another spatial tree. It is to **consume the locality that already exists all the way through preparation, upload, and draw submission**.

## Target complexity

After warmup, the intended cost model is:

```text
static frame CPU work           ~ O(0)
runtime/timeline work           ~ O(active or crossed work)
render preparation              ~ O(changed + visible projection)
GPU upload                      ~ O(changed instance/resource ranges)
draw submission                 ~ O(visible batches/instances)
spatial query                   ~ O(index query + candidates)
resource regeneration           ~ O(actual content/resource changes)
```

The important distinction is between **resident state** and **submitted state**:

- packed geometry/text resources may stay resident even while off-screen;
- viewport culling should filter draw projection/submission without destroying resident cache state;
- becoming visible again should normally not require resource reconstruction.

## Design rules

### 1. Animation properties must not invalidate unrelated resources

The renderer should preserve semantic invalidation boundaries through to GPU work.

Preferred behavior:

```text
transform / opacity / simple paint
    -> instance records only

reveal progress
    -> reveal/member progress state only when representation is unchanged

text position
    -> glyph instance ranges only

content / font / shaping input
    -> text resource + dependent residency

path topology / geometry replacement
    -> affected mesh/resource ranges

structural insertion/removal
    -> local arena/order updates or explicit maintenance barrier
```

Do not collapse all of these into “object changed”.

### 2. Cache the expensive representation, animate cheap state

If an immutable glyph outline, tessellated path, text layout, geometry resource, or GPU mesh is already known, animation should reference that representation rather than recreate an equivalent transient object each frame.

The canonical pattern is:

```text
immutable resource
      |
      v
resident renderer resource / mesh / atlas entry
      |
      v
small mutable instance state
(transform, style, opacity, reveal, morph/member progress)
```

### 3. Viewport culling is a projection, not a second scene

The execution runtime owns the spatial broad phase. Renderer culling must consume ordered candidates from that authority rather than:

- scanning semantic bounds in JavaScript;
- constructing a renderer-specific BVH/R-tree;
- rebuilding a visible-only scene;
- evicting otherwise healthy packed resources merely because an object left the viewport.

Candidate filtering should preserve canonical painter order and keep the ordinary all-live resident state intact.

### 4. Family animation must remain retained

Family scheduling is now content-independent, but renderer realization must also preserve locality.

A partial Text family reveal should not repeatedly turn cached glyph outlines into new transient `VectorPath` geometry and then force a fresh family scratch preparation when only per-member reveal progress changed.

Target model:

```text
TextResource / glyph identity
        |
        v
cached outline / resident path representation
        |
        v
stable family-member render binding
        |
        v
member instance state
(reveal / border-fill phase / visibility)
```

The same rule should apply to future VMobject/SVG/graph/mesh family members: family timing changes mutable member state; immutable content stays resident.

### 5. Separate canonical resident state from frame-specific projection

Visibility, family-member filtering, and similar frame-local projections must not mutate the canonical retained preparation state in ways that leak into later uncullled/ordinary frames.

Use separate candidate/projection scratch where necessary while keeping the packed all-live state authoritative.

### 6. Prefer vector/GPU retention over generic bitmap caches

Do **not** make per-object bitmap caching the normal architecture.

Noon already has better primitives for most content:

- analytic GPU primitives;
- packed vector meshes;
- glyph atlases;
- lazy cached outlines;
- immutable shared geometry resources.

Rasterized subtree caching may be added later as an optional rendering strategy for demonstrably expensive, immutable subtrees, but it must remain an optimization over canonical vector/resource state rather than become source truth.

### 7. Static/dynamic partitioning should be draw-plan level, not DOM/canvas topology

Current 3b1b/manim provides useful evidence for this direction: its WebGPU renderer keeps mutable buffer contents separate from stable draw topology and can reuse a render bundle after the draw sequence settles.

Noon should benchmark an analogous retained visible draw plan through #847 before adopting it. The likely reusable identity is draw topology/pipelines/bindings/ranges and renderer layout generation; ordinary transform/style/reveal/camera buffer changes should not invalidate it when those references remain valid.

Do not introduce extra canvases merely to imitate immediate-mode systems. Do not commit to render bundles until profiling proves command encoding/submission is a material cost after Noon's existing analytic batching and mega-mesh work.

## Current gaps

### Viewport consumption

The retained spatial-query stack exists, and #569 owns its end-to-end consumption. The desired renderer behavior is:

```text
ExecutionSpatialIndex
       |
query_viewport(camera bounds)
       |
ordered execution candidates
       |
map to resident renderer rows
       |
candidate-sized draw projection
       |
GPU
```

The renderer must retain all-live packed/cache state while reducing preparation/submission work to visible candidates. The prerequisite Rust/mirror pieces are already tracked in #569; the remaining web integration is represented by #605 and should be replayed on current master rather than replaced with another culling architecture.

### Retained text dirty ranges

#362 already owns the broader dirty-object text/mixed-renderer migration. Ordinary transform/paint/opacity locality has progressed substantially, but remaining work includes preserving dirty GPU instance ranges, localized prepared-structure transitions, and avoiding parent-level global work.

Family-animation realization must satisfy the same invariant rather than becoming a parallel whole-frame path.

### Family Text reveal realization

The current family renderer can reuse the cached source glyph outline, but it still materializes transformed transient path geometry for partial glyphs and rebuilds family-local scratch/painter state. This is correct but not the desired steady-state performance architecture.

#835 owns making family-member render bindings resident and proving that progressing one family animation does not regenerate immutable glyph/path resources or rebuild unrelated mixed-frame state.

## Required performance gates

Performance claims should be deterministic and structural rather than based only on wall-clock timing.

### Large mostly-static viewport

Use a scene with at least 100k resident objects where a small fraction is visible.

After warmup, assert:

- spatial query candidate count is proportional to visible objects;
- renderer draw projection walks/submits candidates rather than all live objects;
- off-screen packed resources remain resident;
- panning objects into view does not cause geometry/text resource cache misses when content is unchanged;
- painter order remains exact.

### Text-heavy one-object change

Use 10k+ visible/static text objects and mutate exactly one object per frame.

Assert:

- no global text-object/glyph scan on compatible frames;
- GPU text upload bytes are proportional to that object's dirty instance ranges;
- no atlas/raster/outline misses after warmup when effective resource identity is unchanged;
- mixed painter order is not rebuilt for non-structural changes.

### Family animation locality

Use a mixed semantic family such as:

```text
VGroup
├── Text("AB")
└── Circle
```

with many unrelated static objects around it.

After warmup, advance Create/Uncreate/Write-like family progress and assert:

- family scheduling uses the authoritative global member plan;
- immutable glyph outlines/path geometry are reused;
- no unrelated object/resource preparation occurs;
- changed GPU bytes/ranges are proportional to affected family-member state;
- direct seek and incremental playback produce identical render state;
- WebGPU and WebGL paths preserve the same semantics.

### Stable draw-plan command reuse

#847 owns the experiment. Measure a large stable visible scene under transform/style/camera-only changes and separate:

- runtime evaluation cost;
- retained frame preparation cost;
- GPU upload cost;
- command encoding/submission cost;
- GPU execution cost where measurable.

Compare the normal WebGPU path against an experimental stable draw-plan/render-bundle path. The experiment must preserve exact painter order and invalidate correctly for topology/pipeline/binding/range/layout changes. WebGL2 is the control path and does not require equivalent command-bundle machinery.

A benchmark result showing negligible benefit is a valid outcome and should close the experiment without adding permanent complexity.

## Optional future optimization: rasterized subtree cache

A rasterized subtree cache can be useful for exceptionally expensive immutable content such as a complex static equation, imported illustration, or effect-heavy group.

If introduced, keep the architecture explicit:

```text
canonical retained vector/resources
          |
          v
render strategy selection
      /          \
 vector/GPU     cached subtree texture
```

Requirements:

- vector/resources remain canonical;
- cache key includes all visual inputs that affect the raster result;
- viewport/scale policy prevents visible undersampling;
- memory residency is bounded;
- export/high-quality paths may bypass the raster cache;
- rasterization is never required for ordinary transforms of already efficient vector/glyph content.

No implementation issue should be opened for this until profiling demonstrates a real bottleneck.

## Sequencing

1. Complete the #569 viewport-consumer path without introducing duplicate spatial ownership.
2. Continue #362 until text/mixed preparation and uploads are truly dirty-range local.
3. Implement #835 so family-animation render realization is resident and incremental.
4. Add/maintain structural benchmarks proving `O(visible + dirty)` behavior.
5. Run #847 before deciding whether stable draw-plan/render-bundle reuse belongs in the production architecture.
6. Consider optional rasterized subtree caching only from measured workloads.

## Architectural invariant

A useful review question for every renderer change is:

> Did this frame regenerate, traverse, upload, encode, or submit anything whose semantic/render inputs did not change and which did not need to be visible?

If the answer is yes, the burden is on the implementation to show why that work is unavoidable.
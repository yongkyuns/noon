# Manim 3D parity architecture

This document records the architecture decision required by #90 before broad 3D implementation begins. It deliberately defines contracts and staging only; it does not add 3D behavior to the current 2D renderer.

## Decision

Noon will target a **source-compatible common ManimCE v0.21 3D subset**, with unsupported APIs explicitly classified as deferred or intentionally divergent until their observable semantics are implemented and qualified.

This is option 2 from #90. It avoids two failure modes:

- claiming full Manim 3D compatibility before camera, depth, lighting, transparency, and surface semantics are proven;
- inventing a Noon-native 3D object/camera model that later makes source compatibility unnecessarily difficult.

For every API Noon marks supported, Manim-visible semantics remain the oracle under #176/#185. The implementation is free to use a GPU-native retained architecture internally; source compatibility does not imply reproducing Manim's internal renderer.

The first supported tranche is intentionally narrow:

1. `ThreeDScene` camera state and deterministic camera animation;
2. `ThreeDAxes`;
3. one immutable indexed mesh/surface representation;
4. one basic shaded `Surface` fixture;
5. fixed-in-frame 2D overlay composition.

Sphere/cube/prism/cone/cylinder/torus and broader helpers should be expressed on the same mesh/camera/material substrate after that tranche is qualified. Ambient/illusion camera helpers, transparency-heavy scenes, and vector-space scene helpers stay explicit follow-up work rather than being approximated.

## Architectural invariants

3D extends the existing semantic/resource/runtime architecture; it does not create a second scene engine.

```text
Python / Rust / JavaScript
          |
          v
 shared semantic scene
  ObjectId + family/order
          |
     +----+------------------+
     |                       |
 GeometryResource       MeshResource
 2D/path/analytic       positions/normals/indices
     |                       |
     +-----------+-----------+
                 |
           mutable frame state
      transform/style/presence/material
                 |
          retained renderer
                 |
        one WGPU surface/device
```

The following remain non-negotiable:

- one stable semantic `ObjectId` model across 2D and 3D;
- immutable heavy geometry resources outside per-frame state;
- local frame deltas for transforms/material/camera rather than scene reconstruction;
- one render-surface owner and one GPU device lifetime;
- Python/JS adapters do not implement projection, lighting, mesh generation, or depth ordering;
- no per-frame Python mesh projection or one-object-at-a-time readback/compositing path;
- direct seek and forward playback use the same semantic camera/object state.

## Semantic model

### World transform

The current 2D transform model must not be overloaded with implicit 3D conventions. The 3D tranche should introduce an explicit world-transform representation capable of ordinary affine 3D transforms while retaining a zero-cost/compatible path for existing 2D objects.

Conceptually:

```text
WorldTransform
  translation: Vec3
  rotation: quaternion (or equivalent canonical SO(3) value)
  scale: Vec3
```

The serialized/semantic representation should be canonical and deterministic. Renderer-specific 4x4 matrices are derived data, not the public semantic authority.

Existing 2D objects map naturally to z=0 with identity z scale/rotation unless an API intentionally gives them 3D placement. Do not silently reinterpret current 2D transform ordering.

### Camera

`ThreeDScene` needs a real retained camera semantic object/state, not a Python special case and not a transform applied to every object.

The semantic camera state must be sufficient to reproduce the supported Manim v0.21 camera surface, including the initially required fields for orientation, focal/projection behavior, frame center, and zoom. Exact public naming can follow the compatibility adapters, but the shared state must have one authoritative convention for:

- world handedness and axes;
- camera forward/up orientation;
- projection model;
- near/far clipping policy;
- frame dimensions/aspect;
- focal distance / zoom semantics where Manim exposes them.

Camera animations lower to normal retained timeline channels over this shared state. `move_camera`, ambient rotation, and later camera helpers must reuse that representation rather than introducing dedicated render-loop callbacks.

Before the first parity-qualified fixture, add focused numeric tests that pin world-to-view and view-to-clip results for known camera states so a handedness or matrix-order change cannot silently invalidate every 3D scene.

### Mesh resources

Add one immutable mesh resource boundary alongside existing geometry/text resources. The minimum useful resource is:

```text
MeshResource
  positions: packed Vec3
  normals: packed Vec3
  indices: packed integer triangles
  optional vertex attributes required by qualified materials
  local bounds
```

Topology and large vertex/index buffers are immutable resource content. Object transform, color/opacity, material parameters, and presence remain frame/object state when they do not alter topology.

Generated primitives and sampled surfaces should produce deterministic mesh resources at authoring/compile time. A static sphere or surface must disappear from frame CPU work after installation.

Mesh density is part of observable quality and cache identity. Do not let browser/backend defaults silently choose different tessellation. Each generator needs an explicit deterministic resolution policy that can later be compared against Manim's visible output.

## Renderer integration

### One renderer and one surface

3D is a new render lane inside the existing GPU renderer architecture, not another canvas, overlay renderer, worker, or GPU device. The persistent surface work from #492/#517 should remain valid unchanged.

The renderer consumes a mixed retained frame containing existing 2D/path/text items plus depth-aware mesh items and fixed-in-frame items. Mode changes must not transfer the canvas again.

### Depth

The first 3D tranche requires a depth attachment with a backend-portable baseline format supported through wgpu on WebGPU and WebGL. Depth allocation follows the render-surface size/device generation and is recreated only when those change.

Opaque 3D world items use depth testing and depth writes. 2D world objects that participate in the 3D world are projected through the same camera and assigned their semantic z placement rather than being composited later as an unrelated image.

Depth state belongs to renderer pipeline identity, not semantic objects.

### Painter order and compositing domains

A single total 2D painter order cannot correctly model arbitrary intersecting 3D geometry, while pure depth order cannot model fixed-in-frame overlays. The semantic model therefore distinguishes **render/compositing domains** without creating separate scene identities:

1. **world** — camera-projected objects participating in depth semantics;
2. **fixed overlay** — viewport/frame-attached 2D objects rendered after the world pass.

Within the world domain, opaque geometry is depth-resolved. Semantic order remains relevant for equal-depth/coplanar behavior and transparent fallback ordering, but it does not replace the depth buffer.

`add_fixed_in_frame_mobjects` changes the object's presentation domain; it does not clone the object into a second scene.

Do not add arbitrary extra layers until a concrete Manim behavior requires them.

### Transparency

Correct arbitrary translucent 3D compositing is not solved by merely enabling alpha blending. The initial qualified subset should therefore prefer opaque surfaces plus ordinary 2D/fixed overlays.

For the first translucent tranche, define and test an explicit policy rather than relying on backend draw order. A practical staged policy is:

- opaque world pass: depth test + depth write;
- transparent world pass: depth test, no depth write, deterministic back-to-front item ordering by camera-space bounds/representative depth;
- fixed overlay pass: existing painter-order semantics.

This is not claimed to solve order-independent transparency or intersecting translucent triangles. Examples requiring those cases remain deferred until a stronger algorithm is justified and benchmarked.

The parity manifest must distinguish "API exists" from "transparency configuration is qualified".

### Lighting and materials

Start with one intentionally small material contract capable of the first Manim surface examples:

- base color + opacity;
- surface normal;
- ambient contribution;
- one directional/point-light model as required by the pinned fixtures;
- deterministic light position/state owned by the scene/camera semantics where Manim exposes it.

Shader constants and transfer functions must be explicit and shared across WebGPU/WebGL. Do not tune fixture-specific material constants to match screenshots.

If Manim's observable shading cannot be reproduced by the initial simple model, improve the shared material model and keep the fixture blocked rather than introducing per-example shader branches.

## Fixed-in-frame and mixed 2D/3D scenes

Mixed scenes are a first-class requirement, not a later overlay hack.

Existing text/vector/analytic rendering should continue to work in a 3D scene. A world-space 2D object may be projected by the 3D camera; a fixed-in-frame object bypasses world projection and uses the existing frame/canvas mapping.

The same semantic object cannot be simultaneously authoritative in both domains. Any API that changes fixed/world status must be a deterministic state transition with explicit lifecycle behavior.

This contract is necessary for `FixedInFrameMObjectTest` and later labels/UI overlays.

## Runtime and performance contract

The architecture reset's dirty-work invariant applies unchanged to 3D:

```text
static mesh frame       ~ O(0) CPU semantic/mesh rebuild work
camera animation        ~ O(camera state + visible draw submission)
object transform edit   ~ O(changed object instance state)
mesh replacement        ~ O(changed resource + affected object)
```

Do not reproject mesh vertices on the CPU every frame. World/view/projection transforms are GPU inputs. Normals should be transformed in shader or through bounded per-object data, not rewritten into vertex resources for ordinary transforms.

The first benchmark gate should include at least:

- 10k static mesh instances with a moving camera;
- one changing mesh transform among many static meshes;
- one moderate sampled surface with camera rotation;
- mixed 2D overlay + 3D world content;
- seek/rewind across a camera animation.

Track CPU frame preparation, instance/uniform upload bytes, draw count, resource rebuild count, and GPU time where stable. Establish measurements before adding aggressive batching/instancing optimizations.

## Backend contract

Supported 3D semantics must be backend-neutral. WebGPU is not allowed to become the only correct implementation while WebGL silently approximates a different depth/projection/material path.

Every promoted 3D fixture declares its supported backend set. For the common browser target, qualification requires the same semantic camera/mesh state and comparable visible result on WebGPU and WebGL. A backend limitation must be explicit and owned, not hidden by backend-specific scene constants or relaxed example tolerances.

## Exact-parity strategy

Use the same external correctness discipline as 2D (#176/#185):

- pin ManimCE v0.21 and the reference renderer/configuration used for each fixture;
- pin pixel dimensions, frame dimensions, camera parameters, background, mesh resolution, light/material inputs, and FPS;
- capture normalized semantic state below raster level;
- compare camera matrices/projection observables, object/world bounds, mesh/resource metadata, scene membership, and material state;
- compare raster output at deterministic times;
- for animation, sample begin, 25%, 50%, 75%, final rendered sample, exact end, and camera/lifecycle discontinuities;
- require direct seek == forward playback for deterministic scenes.

Do not compensate for a projection, tessellation, depth, or lighting mismatch by changing the upstream example constants.

## Staged implementation

### Stage 0 — contracts and oracle probes

Land this decision, then add no-renderer semantic types/tests for camera conventions and deterministic mesh-resource validation. Establish a tiny pinned Manim oracle for projected points/camera state before GPU work broadens.

Exit criteria:
- canonical camera convention is numerically pinned;
- mesh resource validates finite vertices/normals/indices/bounds;
- 2D scene behavior is unchanged.

### Stage 1 — minimal depth-aware renderer

Add one depth-aware mesh lane, resize/device-generation depth lifecycle, opaque material, and a basic camera.

First fixture: a minimal `ThreeDScene` containing `ThreeDAxes` plus one simple mesh/surface.

Exit criteria:
- WebGPU and WebGL render the same semantic scene;
- camera rotation/seek are deterministic;
- no second canvas/device/worker is introduced;
- static mesh resources are retained.

### Stage 2 — Manim surface + lighting

Implement/qualify `Surface`, basic generated solids, scene light-source state, checkerboard/material behavior required by the gallery, and camera movement helpers.

Promote only APIs that pass semantic + raster gates.

### Stage 3 — mixed composition

Qualify fixed-in-frame objects and mixed 2D/3D scenes. Add `FixedInFrameMObjectTest`-equivalent coverage with real text once the shared text backend is ready; geometry-only probes may establish the compositing contract earlier.

### Stage 4 — transparency and broader camera helpers

Add the explicit transparent world pass and qualify only non-pathological supported cases. Ambient camera rotation can reuse retained camera tracks. `begin_3dillusion_camera_rotation` remains deferred until its exact v0.21 behavior is characterized; do not alias it to ambient rotation by name alone.

### Stage 5 — vector-space scenes

`VectorScene` and `LinearTransformationScene` are semantic/composite APIs built on shared vectors, matrices, labels, and transforms. They should not own a separate 3D renderer. Start them only when their underlying common 2D/3D primitives are already shared and qualified.

## Explicit non-goals for the first tranche

The first implementation must not include:

- arbitrary order-independent transparency;
- volumetric rendering;
- shadows or physically based materials unless a pinned Manim-visible contract requires them;
- a new scene graph for 3D;
- Python-owned matrices/projection/mesh sampling;
- a separate WebGPU-only scene implementation;
- compatibility class names backed by unrelated 2D approximations;
- broad `ThreeDScene`/solid API exposure before the minimal fixture and benchmark pass.

## Review gates for future 3D PRs

A 3D PR should be rejected or split if it simultaneously changes unrelated semantic identity, worker topology, text layout, or 2D renderer contracts without a demonstrated dependency.

Each implementation slice should state:

- semantic/resource boundary it owns;
- exact Manim behavior it is qualifying;
- WebGPU/WebGL behavior;
- dirty-work/resource-lifetime impact;
- new deterministic semantic/raster tests;
- what remains explicitly deferred.

This keeps 3D a controlled extension of Noon's retained architecture rather than a second architecture hidden behind Manim-compatible class names.

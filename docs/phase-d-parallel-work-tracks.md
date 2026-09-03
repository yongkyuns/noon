# Phase D parallel work tracks

## Status

This document is an execution/scheduling overlay for Phase D of `docs/architecture.md` and #956.

It does **not** define a second architecture, roadmap, goal set, acceptance criterion, 3D model, renderer architecture, or frontend requirement. The architecture, Phase D cases, sequencing constraints, gates, and completion checklist remain exactly those stated in `docs/architecture.md` and the owning issues. If this document conflicts with them, the architecture and owning case win.

The purpose of this document is only to expose safe concurrency inside the existing Phase D plan and to separate the future JS/TypeScript frontend lane from the 3D capability lane.

## Principle

The existing D1 -> D2 -> D3 -> D4 -> D5 capability progression remains authoritative. Parallelism exists inside and beside that progression; it does not erase the stated readiness gates.

```text
D1 world/camera/numeric ----+
                            +--> D2 retained mesh/depth --> D3 --> D4 --> D5
D1 mesh-resource substrate -+

D6 JS/TS facade ------------------------------------------ independent after stable handle ABI

qualification/evidence runs beside each promoted slice.
```

A mesh/depth/material lane remains a capability of the same engine. Parallel work must not create a `3DSceneSpec`, second runtime/canvas, renderer-owned semantic scene, frontend-owned projection/shading math, or browser-only 3D semantics.

## Shared handoff contracts

### DH1 — world/camera numeric contract

Owned by D1.1/D1.2/D1.4 and #698.

The handoff covers:
- coordinate handedness and composition conventions;
- canonical high-precision world transform semantics;
- semantic camera position/orientation/projection/frame/clip conventions;
- deterministic world -> view -> clip numeric oracle;
- preservation of existing 2D behavior when lifted into the 3D-capable model.

Renderer and public-API work consume these semantics; they do not own alternate matrix conventions.

### DH2 — immutable mesh-resource contract

Owned by D1.3/#698.

The handoff covers:
- validated immutable mesh topology/attributes/bounds;
- stable resource identity/versioning;
- deterministic rejection of invalid/non-finite input;
- use of the existing shared resource ownership model.

DH1 and DH2 are separate D1 work streams where implementation dependencies allow, but D1's existing gate remains the requirement before the first real D2-qualified scene.

### DH3 — retained mesh/depth renderer contract

Owned by D2/#699 and consuming DH1 + DH2.

The handoff covers:
- retained vertex/index residency;
- compact per-instance world/material state;
- depth attachment lifecycle on the existing device/surface/viewport generation;
- derived camera/world GPU state;
- local instance updates without topology regeneration;
- one qualified narrow 3D fixture through the existing runtime/renderer.

D3/D4 public breadth must not outrun the existing D2 qualification gate.

### DH4 — mesh/material capability contract

Owned by D3 after D2.

Once D2 is complete enough for D3 under the existing plan, surface/solid generation and backend-neutral material/lighting semantics may be split into narrow subtracks that converge on one mesh/material resource model.

### DH5 — mixed composition/camera-helper contract

Owned by D4 after the existing D3/D2 substrate is ready as required by the owning cases.

The handoff covers fixed-in-frame/fixed-orientation composition, supported transparency/order policy, and camera helpers that still lower to the same semantic camera/timeline model.

### DH6 — stable frontend handle ABI

Owned by the Phase A semantic-handle architecture and consumed by D6/#259.

This handoff is independent of the 3D renderer progression. D6 may expose only capabilities whose shared semantic operations are stable; it does not wait for complete D1–D5 breadth and D1–D5 do not wait for JS parity.

## Parallel tracks

### Track DW — world and camera semantics

**Owner:** D1.1/D1.2/D1.4 under #698.

**Owns:** world values/transforms, semantic camera state, projection/frame conventions, and the numeric oracle.

**Can proceed independently from:** GPU mesh/depth implementation and broad public 3D class surface.

**Primary output:** DH1.

### Track DM — immutable mesh-resource substrate

**Owner:** D1.3/#698.

**Owns:** renderer-independent immutable mesh resources, validation, bounds, resource identity, and shared resource integration.

**Can proceed in parallel with DW** once common value/resource conventions required by both are settled. It must not define renderer pipeline state or a second semantic scene.

**Primary output:** DH2.

### Track DR — retained depth/mesh renderer

**Owner:** D2/#699.

**Consumes:** DH1 + DH2.

**Owns:** the first depth-aware retained mesh lane on the existing renderer/device/surface, including depth lifecycle, resource residency, derived world/camera state, and the first qualified fixture.

DR can be developed against narrow D1 fixtures rather than waiting for broad ThreeDScene/Surface/solid APIs. The existing D2 gate still precedes D3 breadth.

**Primary output:** DH3.

### Track DB — surfaces, solids, materials, and lighting

**Owner:** D3 under #956/#90.

**Starts according to the existing plan after D2.** Parallelism is internal to D3, not a change to that sequencing.

Useful subtracks after DH3 exists:
- deterministic surface sampling and generated-solid mesh construction;
- normals/backend-neutral material semantics;
- deterministic light semantics;
- qualification/resource-locality fixtures.

These subtracks converge on one DH4 mesh/material capability and must not create class-specific renderer paths.

### Track DC — mixed 2D/3D composition and camera behavior

**Owner:** D4 under #956/#90.

**Owns:** fixed-in-frame/fixed-orientation composition, supported transparency/order policy, camera helpers, and associated gallery integration.

This track consumes the existing D1–D3 substrate as required by its cases. Individual camera-helper semantic tests may use DH1 early, but promoted D4 capability must not bypass the existing renderer/composition readiness requirements.

**Primary output:** DH5.

### Track DA — vector-space and advanced scene capability

**Owner:** D5/#90.

**Starts after core 3D is stable as already stated in the roadmap.** Within D5, ThreeDAxes, vector-space scenes, linear-transformation helpers, and advanced examples may be split according to the shared primitives they consume rather than implemented as one monolith.

DA composes existing objects/animations/resources; it does not define a new advanced-scene engine.

### Track DJ — JavaScript/TypeScript facade

**Owner:** D6/#259.

**Consumes:** DH6, the stable shared semantic-handle ABI.

This track is intentionally independent of the D1–D5 3D capability chain. It may proceed once the Phase A handle ABI is stable enough for the supported shared core it intends to expose.

DJ must not:
- block Python/Rust completion;
- require every D1–D5 feature to have JS parity;
- expose migration-era scene JSON as authoring API;
- implement semantic behavior in JavaScript that belongs in shared Rust.

### Track DQ — qualification and regression evidence

**Owner:** existing B7-style qualification infrastructure and Phase D gallery/reference cases such as #254.

**Runs continuously.** It pins numeric world/camera behavior early, renderer/depth behavior at D2, and source-equivalent public cases as D3–D5 slices are promoted. Existing 2D regression coverage remains part of every relevant handoff.

## Dependency view

```text
Track DW: world/camera ---- DH1 ----+
                                    |
Track DM: mesh resources -- DH2 ----+--> Track DR: depth/mesh renderer -- DH3 --> DB --> DC --> DA

Track DJ: JS/TS facade <-- DH6 from stable Phase A handle ABI
          (independent of the D1-D5 capability chain)

Track DQ: qualification/regression evidence runs beside every promoted slice.
```

This preserves the existing Phase D sequencing. The main new scheduling clarification is that D1 itself has separable semantic/numeric and mesh-resource work, D2 does not need broad API breadth to start once D1 contracts are usable, and D6 is a distinct frontend lane rather than part of the 3D critical path.

## Practical isolation rules

1. **Pin numeric semantics before renderer convenience.** Renderer code consumes DH1; it does not invent camera/world conventions.
2. **Keep meshes renderer-independent at D1.** Mesh topology/resource validation belongs to the shared resource layer; residency/pipelines belong to D2.
3. **Use narrow fixtures to unblock D2.** A hand-built semantic camera + mesh fixture is sufficient to develop/qualify the retained mesh lane; broad public class APIs are not a prerequisite.
4. **Respect the D2 -> D3 -> D4 -> D5 readiness sequence.** Parallelism inside those stages must not expose unsupported public breadth ahead of the existing gates.
5. **Split D3 by capability, not by class.** Surfaces/solids should reuse common mesh generation; materials/lights should reuse common semantic/render inputs.
6. **One presentation surface.** Mixed 2D/3D work does not add overlay canvases or duplicate scene/runtime authority.
7. **Treat D6 as frontend work.** JS/TS consumes shared semantic handles and remains independent of 3D breadth.
8. **Keep 2D regressions continuously green.** Extending the engine to 3D must not silently change established 2D transform/camera/render semantics.

## Suggested integration cadence

```text
DW and DM land narrow D1 contracts in parallel
        |
        +--> DQ pins numeric/resource regression evidence
        +--> DR develops against narrow fixtures

DR reaches the existing D2 gate
        |
        +--> DB splits D3 capability work into narrow shared-resource/material PRs
        +--> DQ qualifies each promoted slice
        +--> DC and later DA consume the stable substrate in existing roadmap order

DJ progresses independently whenever the shared frontend ABI/capability it needs is stable.
```

The final Phase D exit remains the existing #956 completion checklist. Parallel execution changes scheduling only; it does not change the 3D architecture, readiness gates, capability scope, or optional status of the JS/TypeScript frontend.
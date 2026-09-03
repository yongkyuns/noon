# Phase A parallel work tracks

## Status

This document is an execution/scheduling overlay for Phase A of `docs/architecture.md` and #953.

It does **not** define a second architecture, roadmap, goal set, acceptance criterion, or compatibility policy. The architecture, Phase A cases, and exit gates remain exactly those stated in `docs/architecture.md` and the owning issues. If this document conflicts with them, the architecture and owning case win.

The purpose of this document is only to make explicit which existing Phase A cases can be pursued largely independently once a small number of typed contracts are available.

## Principle

Phase A should not be implemented as one long repository-wide rewrite.

The target architecture already provides separation boundaries:

```text
Rust authoring ───────┐
                      v
Python facade ───> Semantic Scene ──> Execution Plan ──> Runtime/Renderer ──> platform host
                      ^                    ^
                      |                    |
                shared semantics      typed execution contract
```

Parallel work must preserve one authority at each layer. Parallel tracks are not permission to create alternate semantic stores, lowerers, execution models, transport mirrors, or compatibility facades.

## Shared handoff contracts

The tracks below synchronize at a few narrow handoff points rather than waiting for whole roadmap phases to finish.

### H1 — semantic handle and operation contract

Owned by A1/#957.

Enough of the Semantic Scene exists for frontend work when the relevant operation has:

- stable semantic `NodeId`/generation behavior;
- explicit detached versus scene-owned lifecycle;
- shared mutation/query operation semantics;
- deterministic errors for stale/invalid handles;
- no dependency on `SceneDefinition`, wire IDs, execution slots, or renderer IDs as semantic authority.

A2/#958 and A3/#61 may migrate operation families incrementally as their corresponding shared semantic operations land. They do not need to wait for every A1 feature to be complete.

### H2 — typed lowering contract

Owned by A1.6/#957.

Execution-side work may proceed once there is one explicit typed semantic-to-execution entry point and representative fixtures that can produce/validate `ExecutionPlan` data without a serialization hop.

The concrete internal storage of Semantic Scene and Execution Plan may continue evolving. Consumers depend on documented behavior and typed APIs, not on migration-era struct layout.

### H3 — typed execution-session contract

Owned by #969 in conjunction with A1.6/A2.5.

Native-host and direct Rust/WASM-host work may proceed independently once they can drive the same typed execution/runtime/renderer session. Neither host waits for the other and neither owns semantic/runtime truth.

### H4 — deletion ownership

Owned by A4/#959.

Migration models/adapters are deleted only after their production callers have moved to the authoritative semantic/execution path. A4 can inventory and delete already-dead seams continuously; it does not need to wait for all of A2/A3/A6 before starting.

## Parallel tracks

### Track S — Semantic authority and shared operations

**Owner:** #957 / A1

**Existing cases:** A1.1–A1.6.

**Owns:**
- one generational semantic identity/store;
- semantic content/value ownership;
- family/lifecycle/shared authoring operations;
- animation/reactive/host declarations;
- one mutation transaction vocabulary;
- one explicit `SemanticScene -> ExecutionPlan` lowering boundary.

**Can proceed independently from:** frontend wrapper shape, native/web platform lifecycle, Python migration details, crate cleanup.

**Must not duplicate:** semantic identity/store or lowering semantics.

**Primary outputs to other tracks:** H1 and H2.

### Track R — Rust authoring migration

**Owner:** #958 / A2 authoring cases

**Existing cases:** A2.1–A2.4, A2.7–A2.8; A2.5/A2.6 integrate with Track E.

**Owns:**
- public `Scene`/`Mobject` semantic handles;
- constructors/common operations;
- copy/state/target semantics;
- `.animate`/`Scene.play` construction;
- Rust internal caller migration;
- deletion of `noon::legacy` after callers move.

**Can proceed independently from:** Python facade migration and native/web host implementation.

**Handoff:** consume H1 operation-by-operation. Do not wait for all of A1 before migrating already-defined operation families.

### Track P — Python thin facade

**Owner:** #61 / A3

**Existing cases:** A3.1–A3.5 and the detailed #61 cases.

**Owns:**
- typed Python/WASM/shared-handle facade;
- removal of Python-owned semantic snapshots/IDs/scheduling state;
- migration of Python object behavior and `.animate` syntax onto shared semantics;
- explicit host-callback wrapper behavior;
- Python side of paired executable evidence.

**Can proceed independently from:** Rust public facade implementation details and platform-host implementation.

**Handoff:** consume the same H1 semantic operations as Track R. Rust and Python frontends must converge on shared semantics, not on each other's wrapper internals.

### Track E — Typed execution and platform hosts

**Owner:** #969 with A2.5/A2.6 and A6.7

**Existing cases:** common typed execution session, native Rust host, direct Rust/WASM host, execution-target examples/ratchets.

**Owns:**
- common typed execution/runtime/renderer session;
- native window/surface/event-loop/frame/present integration;
- direct Rust/WASM browser-canvas integration;
- keeping `noon-render-wgpu` reusable and platform-lifecycle-free.

**Can proceed independently from:** Python facade migration, most public Rust authoring migration, and migration-model deletion by using representative semantic/execution fixtures at the typed boundary.

**Internal parallelism:** after H3 is available, native-host and direct-WASM-host work are separate subtracks and should not block one another.

**Handoff:** consume H2; expose H3.

### Track C — Migration deletion and repository normalization

**Owners:** #959 / A4 and #960 / A5

**Existing cases:** A4.1–A4.8 and A5.1–A5.7.

**Owns:**
- serialization-consumer inventory;
- deletion of obsolete scene/IR/sidecar/adaptor paths after caller migration;
- codec ownership decisions;
- module/crate normalization once ownership is stable;
- cleanup of migration-era exports/naming.

**Can proceed independently from:** unfinished work in unrelated operation families.

**Rule:** clean up behind migrated seams, not ahead of them. Do not preserve an obsolete abstraction merely so this track can move independently, and do not reorganize a still-moving ownership boundary just to make the tree look final early.

### Track V — Ratchets and executable evidence

**Owner:** #961 / A6

**Existing cases:** A6.1–A6.9.

**Owns:** structural/source checks, locality/atomicity assertions, paired semantic evidence, native/direct-WASM exit checks, and the final Phase A meta gate.

**Runs continuously alongside every other track.** A ratchet should land when its prohibited migration seam becomes unnecessary rather than being deferred to the end of Phase A.

Track V does not redefine implementation APIs. It encodes the existing architecture/acceptance conditions as executable checks.

## Dependency view

```text
                         ┌──────────── Track R: Rust facade ────────────┐
                         │                                              │
Track S: Semantic ──H1───┼──────────── Track P: Python facade ─────────┤
 authority               │                                              │
     │                   └──────────────────────────────────────────────┘
     │
     H2
     │
     v
Track E: typed execution session
     │
     H3
     ├──────── native host
     └──────── direct Rust/WASM host

Track C: cleanup/normalization follows each migrated seam as it becomes dead.
Track V: ratchets/evidence runs beside all tracks and tightens incrementally.
```

This is a dependency graph, not a phase-order rewrite. Existing A1–A6 scope and acceptance remain unchanged.

## Practical isolation rules

To keep the tracks genuinely separable:

1. **Depend on behavior, not migration layout.** Frontends and execution hosts consume typed shared operations/contracts rather than concrete legacy scene/document shapes.
2. **Use representative fixtures at boundaries.** Compiler/runtime/host tests may construct semantic/execution fixtures directly so platform work is not blocked by frontend migration.
3. **One producer of semantic truth.** Do not implement parallel semantic stores or a second lowering semantics to increase concurrency.
4. **Multiple consumers are encouraged.** Rust/Python frontends and native/web hosts should independently exercise the same shared contracts.
5. **Delete only after caller migration.** A4 cleanup follows ownership transfer; temporary adapters must retain a named deletion owner.
6. **Avoid repository-wide mixed PRs.** Prefer PRs owned by one track plus the smallest typed handoff needed by another track.
7. **Ratchet each completed handoff.** Once a legacy route is no longer required, add the structural check that prevents its return.

## Suggested integration cadence

Parallel work should merge in narrow vertical increments rather than long-lived mega-branches. A typical cycle is:

```text
Track S lands one shared semantic operation/contract
        |
        +--> Track R migrates the Rust operation family
        +--> Track P migrates the Python operation family
        +--> Track V adds ownership/parity checks
        +--> Track C deletes the now-unused migration seam

Track S lands/extends typed lowering
        |
        +--> Track E extends common execution session
              +--> native host integration
              +--> direct-WASM host integration
        +--> Track V adds direct-path checks
        +--> Track C removes obsolete transport/mirror use
```

The final Phase A exit remains the existing #953/#961 checklist. Parallel execution changes how work is scheduled, not what must be true when Phase A is complete.
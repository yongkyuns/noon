# Phase B parallel work tracks

## Status

This document is an execution/scheduling overlay for Phase B of `docs/architecture.md` and #954.

It does **not** define a second architecture, roadmap, goal set, acceptance criterion, compatibility policy, or feature scope. The architecture, Phase B cases, dependencies, gates, and completion checklist remain exactly those stated in `docs/architecture.md` and the owning issues. If this document conflicts with them, the architecture and owning case win.

Phase B still begins only after the Phase A exit gate. This document only makes explicit which existing Phase B cases can proceed independently once the exact primitives they consume are stable.

## Principle

The existing Phase B ordering is a dependency graph, not a requirement to complete every case in one lane before another lane starts.

```text
                         +--> geometry/resources ----+
                         |                           |
B1 shared primitives ---+--> deterministic anim ----+--> dependent B5 slices
                         |                           |
                         +--> text/math -------------+
                         |
                         +--> scene/camera

B7 qualification/evidence runs beside every lane.
```

Parallel work must continue to implement semantics once in shared Rust and must not create feature-local scene engines, runtimes, identity systems, renderer semantic types, or frontend-owned behavior.

## Shared handoff contracts

### BH1 — core semantic primitive contract

Owned by B1/#74.

A downstream feature lane may consume a B1 primitive when the relevant behavior is stable enough to be exercised through shared semantic handles, including as applicable:

- family identity/order/traversal;
- semantic bounds/query behavior;
- shared transform/style mutation behavior;
- layout behavior;
- state/copy/target behavior used by `.animate` and Transform.

B2, B3, B4, and B6 do not need to wait for every B1 case to close. They wait only for the B1 primitives they actually consume.

### BH2 — retained path/content resource contract

Owned by B2/#76/#77/#78/#79.

Enough geometry/resource behavior exists for consumers when the relevant feature has:

- immutable/versioned retained content;
- stable path/content queries needed by consumers;
- deterministic resource identity/diagnostics;
- bounded mutation/replacement semantics;
- no frontend or renderer-only semantic representation.

B3 transform/creation work and B5 plotting/Graph work may consume these capabilities independently as they land.

### BH3 — deterministic animation/member contract

Owned by B3/#80/#82 and the existing family/member animation work.

The handoff covers shared lifecycle/timing/composition semantics and content-local member descriptors needed by consumers such as text Write/Create/matching behavior. Consumers depend on semantic behavior, not execution-layout internals.

### BH4 — compiled text and part-identity contract

Owned by B4/#83/#369 and the existing text backend cases.

The handoff is the backend-neutral text artifact/resource and semantic part/source identity needed by labels, composites, plotting, and text animation. Native text, Typst, LaTeX, and numeric text may progress independently where they share this contract.

### BH5 — qualification/evidence contract

Owned by B7/#185/#109/#91 and focused qualification cases.

A feature is qualified as soon as its implementation slice is real enough to test. B7 is not a tail phase and does not wait for all B1–B6 breadth.

## Parallel tracks

### Track BF — core object/family foundation

**Owner:** B1/#74.

**Owns:** the common family/query/layout/state/style primitives consumed by other Phase B lanes.

**Can proceed independently from:** specialized geometry, text backend implementation, camera host details, plotting breadth, and qualification infrastructure.

**Primary output:** BH1, incrementally by primitive family.

### Track BG — geometry and retained resources

**Owners:** B2/#76/#77/#78/#79 and their children.

**Owns:** common shapes, lines/arrows/dashes, VMobject/path behavior, SVG/images, and retained geometry/resource semantics.

**Can proceed independently from:** text/math breadth, most camera work, and unrelated animation families once the exact BH1 primitives it consumes are available.

**Primary output:** BH2.

### Track BA — deterministic animation and lifecycle

**Owners:** B3/#80/#82 and related deterministic animation cases.

**Owns:** deterministic animation leaves, Transform family, family/member animation, lifecycle, and composition semantics.

**Can proceed independently from:** text backend implementation and plotting breadth by using ordinary semantic/path fixtures. Text-specific integration consumes BH4 later rather than blocking the core animation lane.

**Primary output:** BH3.

### Track BT — text and math

**Owner:** B4/#83 and children #364/#369/#365.

**Owns:** native text, compiler/cache, Typst/LaTeX normalization, numeric text, semantic part identity, and text-resource behavior.

**Internal parallelism:** once the backend-neutral artifact contract is usable, native text shaping, LaTeX/Typst backend work, numeric text, and renderer/resource qualification can progress as separate subtracks. B4.5 animation/matching integration consumes BH3 rather than blocking the resource/backend work.

**Primary output:** BH4.

### Track BM — plotting, composites, graphs, and static visualization

**Owners:** B5/#85/#84/#87/#88 and their children.

B5 is itself several largely independent consumer lanes:

- **plotting/coordinate systems** consume BH1 + BH2 and BH4 when labels are required;
- **composite mathematical objects** consume family/layout + geometry + text primitives as needed;
- **Graph/DiGraph** consumes family identity + line/arrow/path primitives, with text only for labeled cases;
- **static visualization** consumes the ordinary geometry/text/family substrate appropriate to each case.

These subtracks should not wait for unrelated B5 capability. Each begins when its own stated dependencies are ready.

### Track BC — 2D Scene and camera

**Owner:** B6/#89.

**Owns:** scene ordering/lifecycle surface, one semantic 2D camera/frame state, MovingCameraScene-compatible behavior, and compatible compositing that stays on the shared runtime/renderer.

**Can proceed independently from:** text/math and plotting breadth once the family/order and camera/runtime primitives it consumes are stable.

### Track BQ — qualification and examples

**Owners:** B7/#185/#109/#91/#251–#256 and focused qualification cases.

**Runs continuously beside BF–BC.** It owns source-equivalent semantic/timing/raster evidence and paired Rust/Python examples; it does not own alternate implementations.

A lane should hand its first representative executable slice to BQ immediately rather than accumulating unqualified breadth.

## Dependency view

```text
                         Track BG: geometry/resources
                       /            |
                      /             +------+
Track BF: B1 --BH1---+                     |
                      \             Track BM: plotting/composites/graphs
                       \                    ^
                        +--> Track BA ------+ where animation is required
                        |
                        +--> Track BT ------+ where text/labels are required
                        |
                        +--> Track BC: scene/camera

Track BQ: qualification/examples runs beside every track.
```

Dependencies are per capability, not per whole tracker. For example, an explicit-position Graph slice need not wait for all plotting or all text work, and native text shaping need not wait for every geometry class.

## Practical isolation rules

1. **Consume the smallest stable primitive set.** A downstream PR should state the B1/B2/B3/B4 capabilities it depends on rather than depending on completion of an entire parent tracker.
2. **Use semantic fixtures at handoffs.** Animation, renderer, and qualification work may use focused semantic/content fixtures instead of waiting for every frontend convenience API.
3. **Keep resource contracts backend-neutral.** Plotting, Graph, text, and geometry do not get feature-specific renderer architectures.
4. **Do not make B5 one mega-lane.** Plotting, composites, Graph, and static visualization integrate independently when their own dependencies exist.
5. **Qualify continuously.** B7 evidence should land with representative slices, not only at Phase B exit.
6. **Avoid mixed mega-PRs.** Prefer one owning track plus the smallest shared-contract extension genuinely needed by another.
7. **Preserve Phase A invariants.** No feature may reintroduce frontend-owned semantics, serialized in-process boundaries, second identities, or O(total-scene) convenience fallbacks.

## Suggested integration cadence

```text
Track BF lands one stable shared primitive
       |
       +--> BG/BA/BT/BC consume only where relevant
       +--> BQ adds focused differential/evidence cases

Track BG/BT/BA land a reusable capability
       |
       +--> one corresponding BM subtrack consumes it
       +--> BQ promotes representative source-equivalent cases
```

The final Phase B exit remains the existing #954 completion checklist. Parallel execution changes scheduling only; it does not change what Phase B means or what must be true when it is complete.
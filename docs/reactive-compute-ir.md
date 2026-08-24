# Native compute IR

Noon's serializable `ReactiveExpr` remains an authoring/dependency description,
but it is not the intended frame-critical execution representation. Native
reactive expressions lower to a compact typed register program before runtime.

`ComputeProgram` currently supports the same bool/scalar/`Vec2` value vocabulary
as native reactivity and the existing arithmetic/trigonometric operations. Signal
references are resolved to dense signal indices during lowering. Runtime
instruction execution therefore performs neither recursive AST traversal nor
semantic `SignalId` map lookup.

The format is deliberately backend-neutral. It is the common compute IR for:

- native reactive dependencies;
- future tracing of pure Python expressions;
- a SIMD/native CPU backend;
- future WGSL lowering for sufficiently large kernels.

A separate later "kernel IR" should not duplicate this compiler path.

## Scheduling

`DenseDirtyQueue` provides the scheduler primitive for dependency-local updates.
A dense pending bitset prevents duplicate work and a compact rank heap preserves
topological order. Scheduling operates on dense node indices/ranks only; ordered
maps/sets of semantic IDs are not part of the update path.

Propagation still follows Noon's change-stopping rule: if a recomputed derived
value is unchanged, downstream nodes are not scheduled.

## Validation strategy

The IR owns a deterministic reference interpreter. Tests cover typed lowering,
scalar/vector operations, invalid-type rejection, queue deduplication/order, and
many-input agreement with direct reference math. During migration, randomized
or corpus-based comparisons should keep the old expression evaluator available
in tests only until all native reactive execution goes through `ComputeProgram`.

## Backend contract

The current interpreter is intentionally simple. Backend implementations must
preserve:

- typed instruction semantics and finite-value validation;
- deterministic input/output ordering;
- exact dependency-local invalidation behavior;
- change stopping at unchanged derived nodes.

GPU/SIMD work should specialize the execution backend rather than introduce a
second semantic expression language.

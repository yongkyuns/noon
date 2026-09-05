# noon-compile

`noon-compile` owns typed, renderer-independent semantic analysis, lowering, and specialization into derived execution data.

## Ownership

The primary Phase A path lowers `noon_core::SemanticStore` into `SemanticExecutionLoweringOutput`, which `ExecutionSession` installs into the existing runtime machinery. This crate owns canonical reactive and animation lowering, execution specialization, and validation that fails before a new execution result is published.

It also owns deterministic lowering work such as semantic-derived mapping, dense execution indices, and renderer-independent geometry/transform specialization. Durable `ExecutionSlotTable` identity belongs to the runtime. Runtime advancement, effective frame state, input/reactive execution, GPU resources, drawing, platform lifecycle, and language adaptation are outside this crate.

`SceneDefinition` remains at temporary compatibility seams during migration. It is neither the primary authored input nor a peer scene model, and it does not create a serialization boundary.

## Why this is a crate

Lowering is deterministic, renderer-independent work with a concrete reuse and test boundary: the public Rust facade and runtime/session integration consume its derived execution data, while `noon-runtime` owns mutable execution state. The crate must not grow a parallel authoring IR.

This boundary follows `docs/architecture.md` and the Phase A5 normalization rules in #960.

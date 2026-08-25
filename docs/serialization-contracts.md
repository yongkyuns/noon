# Serialization and wire compatibility

Noon's persistent/cross-language wire surface is intentionally smaller than its Rust type surface. The machine-readable inventory is [`compat/wire-contracts-v1.json`](../compat/wire-contracts-v1.json); checked-in positive and negative examples live under `compat/wire/` and are executable tests, not illustrative documentation.

## Version domains

There are two independent version domains today:

- **Noon IR v1** covers scene documents, semantic/reactive scene documents, and patch batches. Rust readers preflight the top-level `version` before decoding the payload, so a future document containing future-only enum variants still fails as `UnsupportedVersion` rather than malformed current-version JSON.
- **Authoring protocol v5** covers messages between `PythonAuthoringClient` and the Python worker. Callback slots, callback frame payloads, authoring result metadata, and the request/response envelope are scoped by that protocol. Callback mutation commits themselves use Noon IR v1 `PatchBatch` sequence ordering.

Changing one domain does not implicitly bump the other.

## Evolution rules

Required fields remain required for that version. Fields explicitly optional/defaulted in the manifest may be absent. Current Rust serde readers ignore unknown record fields, so a producer may add non-semantic metadata without a version bump; adding a new required field, changing meaning/type, or adding a variant old readers must understand requires a version bump or explicit backward-compatible default.

Arrays whose ordering affects semantics retain producer order. Object/track declaration order remains deterministic for serialization, and patch arrays are applied in order. Do not rely on JSON object-key ordering for semantics.

Rust wire IDs are JSON integers backed by `u64`. Because the browser/worker boundary uses JavaScript numbers, the cross-language compatibility range is **0 through `Number.MAX_SAFE_INTEGER` (9,007,199,254,740,991)**. Producers must not emit larger IDs or sequences even though a native Rust parser can represent them.

JSON floating-point fields must be finite. `NaN` and infinities are not wire values. Preserve full JSON numeric precision on transport; individual runtime fields may intentionally narrow to `f32` after decoding.

## Canonical fixtures and migrations

`compat/wire/v1/` is the reviewed v1 fixture set. Exact compact text is asserted only where Noon deliberately produces deterministic canonical serialization (currently the minimal scene and patch envelopes); semantic equivalence is the contract elsewhere. `compat/wire/invalid/` contains future-version and malformed examples that fail deterministically.

When v2 is introduced, keep v1 fixtures. Add v2 fixtures and a migration/compatibility test stating explicitly whether v1 remains directly readable or is upgraded through a migration function. Never silently reinterpret an old fixture under new semantics.

## Cross-language CI

Rust contract tests decode fixtures through `noon-ir`. Node tests run the same v1/future fixtures through browser authoring validators. Existing Python→JSON→Rust/WASM parity and browser authoring jobs remain executable producer/consumer integration tests; this fixture suite pins the schema those jobs are expected to produce.

Internal Rust-only structs, renderer cache data, GPU resources, and execution indices are not compatibility contracts unless added to the manifest.

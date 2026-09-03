# Noon agent operating contract

This file is the operating contract for coding agents working on Noon, especially agents entering the repository without prior conversation context.

It is intentionally short and imperative. It does **not** define a second architecture or roadmap.

## 1. Read this first

Before making architectural or cross-cutting changes, read `docs/architecture.md`.

`docs/architecture.md` is the sole architecture and roadmap authority. If this file, an old issue, an old PR, a test, or existing code appears to conflict with it, the architecture document wins.

GitHub issues contain current implementation detail and task decomposition. Git history is reference material, not an architecture authority.

## 2. Greenfield rule

Noon is greenfield.

Do not preserve historical Noon APIs, internal scene models, wire formats, compatibility aliases, migration adapters, crate boundaries, or implementation patterns merely because they already exist.

Prefer deletion over compatibility scaffolding.

If temporary migration code is unavoidable, the PR must identify the current issue that owns its deletion. Do not create an adapter with no explicit removal condition.

## 3. Product invariant: Rust is first-class

The complete native product must work as a normal Rust library/runtime without Python, JavaScript, WASM, browser infrastructure, JSON, or another transport layer.

The native path is:

```text
Rust API
  -> Semantic Scene
  -> Execution Plan
  -> Runtime
  -> Renderer
```

That path is typed and in-memory.

Do not introduce serialization/deserialization, JSON, scene documents, wire payloads, IPC, WASM bindings, or frontend bridges as ordinary boundaries between these layers.

Python and future JavaScript/TypeScript APIs are optional language wrappers over shared Rust semantic operations. They are not required engine components.

Serialization is acceptable only at explicit external boundaries such as debugging, export/import, test fixtures/goldens, or genuine cross-context transport. Serializable data types do not imply that serialization is an engine boundary.

## 4. The four authorities

There is exactly one authority at each architecture layer:

1. **Semantic Scene** — authored meaning and semantic identity.
2. **Execution Plan** — lowered, specialized renderer-independent execution data.
3. **Runtime** — current mutable execution state.
4. **Renderer** — derived retained GPU state and draw work.

Do not create:

- another authored scene representation;
- another semantic ID allocator;
- frontend-owned semantic state;
- frontend-owned animation scheduling/interpolation;
- renderer-owned semantic truth;
- feature-specific scene engines;
- feature-specific runtimes;
- feature-specific mutation protocols;
- a serializable transport structure that becomes an in-memory authority.

## 5. Semantic identity and authoring

Use one scene-global generational semantic identity space.

Execution indices, GPU indices, resource handles, Python wrapper identities, JavaScript identities, and transport IDs are derived identities only.

Rust authoring is a direct first-class API over shared Rust semantic operations.

Shared high-level behavior belongs in Rust semantic code: scene membership, family behavior, layout, bounds, target state, animation semantics, lifecycle, ordering, signals, and mutation rules.

## 6. Frontend ownership

Python may own Python-specific ergonomics such as class shape, signatures, argument coercion, exception mapping, Python callable identity, and invocation of arbitrary Python callbacks.

Python must not own a second valid scene, semantic object state, semantic IDs, family traversal semantics, painter ordering, layout/bounds, animation scheduling, interpolation, rollback state, or renderer/runtime state.

Future JavaScript/TypeScript follows the same rule.

If equivalent Rust and Python authoring produce different semantic behavior, fix the shared Rust semantics rather than implementing two engines.

## 7. Mutations and live work

Authored and live structural/property changes should converge on the shared semantic mutation transaction model.

Host callbacks, native input, editor actions, graph topology changes, and hot reload must not invent parallel patch systems when the shared mutation vocabulary can represent the change.

Mutations are atomic: validate before commit, preserve coherent snapshots, and propagate only the required dirty work.

## 8. Locality is an architecture requirement

A local change must remain local through the full pipeline:

```text
Semantic Scene
  -> lowering
  -> Runtime
  -> Renderer
  -> GPU upload
```

Do not silently introduce O(total-scene) work for a local operation.

Treat these as design constraints, not optional optimizations:

- clean paused/static frame: approximately O(0) meaningful CPU work;
- reactive work: proportional to the dirty dependency closure;
- property mutation: proportional to affected slots;
- structural mutation: proportional to local dependencies plus genuinely required relowering;
- visibility: proportional to index query plus candidates;
- renderer preparation/uploads: proportional to dirty resident state and changed resources/ranges.

## 9. Current roadmap ownership

Use the current roadmap issues to locate ownership before implementing broad work:

- `#953` — Phase A: architecture consolidation;
- `#954` — Phase B: common 2D semantics;
- `#955` — Phase C: native interaction, locality, and live authoring;
- `#956` — Phase D: 3D and broader capability on the same engine.

During Phase A, avoid broad feature expansion unless the work is a correctness fix or architecture-neutral.

Do not resurrect closed architecture issues or old PR branches wholesale. Replay only the useful behavior onto the current architecture.

## 10. Existing code is not automatically correct

The repository is in active architectural consolidation.

Do not infer the target design solely from existing types, tests, modules, JSON paths, compatibility functions, crate boundaries, or browser plumbing.

In particular, legacy scene models, `SceneDefinition`/`SceneSpec`-style structures, retained sidecars, migration JSON APIs, and compatibility adapters may exist only because Phase A has not deleted them yet.

When new architecture makes old code unnecessary, remove it rather than wrapping it.

## 11. Crates and modules

Crates require a real dependency, compilation-target, or reuse boundary.

Prefer modules over crates for conceptual organization alone.

Do not preserve a crate merely because another crate currently imports it. First ask whether the architecture still requires the boundary.

Module structure should make ownership obvious. Avoid `#[path]` and `include!` structures that hide unrelated domains behind one module.

## 12. Renderer rules

The renderer consumes runtime/execution state and owns only derived/disposable retained GPU state.

Transform/style/visibility-only changes must not regenerate immutable geometry or text resources unnecessarily.

Use stable resource generations, dirty-range uploads, retained residency, and execution-owned visibility/spatial information.

Text, Graph, interaction, and 3D are features of the same renderer/runtime architecture, not separate engines.

## 13. Local validation

Use the repository validation entrypoint instead of reconstructing CI commands from memory:

```bash
bash scripts/check.sh fast
```

Use `bash scripts/check.sh full` before merge when the change affects Rust plus browser integration or when the owning issue/PR calls for the full local gate. Run narrower focused tests while iterating, but report the exact commands actually run in the PR.

GitHub CI has additional parallel browser, parity, golden, differential, performance, and platform-specific jobs. A local `check.sh` pass does not replace required GitHub checks.

## 14. Tests and validation

Prefer deterministic structural tests over tests that merely make current implementation details permanent.

Protect invariants such as:

- stale generational handles are rejected;
- semantic identity survives valid detach/re-add flows;
- direct seek agrees with forward execution for deterministic semantics;
- mutations are atomic and rollback-safe;
- local mutations remain local;
- retained resources are reused when content did not change;
- Rust and Python authoring share semantic behavior;
- Manim compatibility is checked at supported semantic/visual boundaries;
- native Rust execution does not depend on serialization or language frontends.

JSON-based fixtures are fine when testing an explicit codec/transport/export boundary. Do not make a legacy serialized scene path the canonical engine qualification path.

## 15. PR discipline

Keep PRs narrow enough that their architectural effect is reviewable.

A PR should state:

- which roadmap issue/case it advances;
- which architecture layer owns the change;
- whether it introduces any temporary migration seam;
- the issue that deletes that seam;
- relevant locality/complexity implications;
- focused validation added or updated.

Do not add a permanent compatibility layer to make a PR easier to merge.

Do not add new permanent roadmap/architecture documents. Update `docs/architecture.md` only when the architecture itself changes. Put implementation checklists and remaining work in GitHub issues.

## 16. Before finishing a task

Before declaring work complete:

1. Re-read the relevant section of `docs/architecture.md`.
2. Check whether the change created a second authority, identity space, scheduler, runtime, renderer model, or serialization boundary.
3. Search for obsolete code made unnecessary by the change and delete it when safe within scope.
4. Ensure temporary compatibility code has an explicit deletion owner.
5. Add focused tests for the invariant or behavior changed.
6. Check that local operations remain local.
7. Run the appropriate `scripts/check.sh` mode and any focused validation required by the owning issue.
8. Update the owning GitHub issue/PR with what landed and what remains.

When uncertain, choose the design that keeps one semantic authority, one lowering boundary, one runtime, one renderer, and a fully Rust-native typed in-memory path.
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

## 3. Product invariant: Rust is first-class on native and web

The complete native product must work as a normal Rust library/runtime without Python, JavaScript, WASM, browser infrastructure, JSON, or another transport layer.

The native path is:

```text
Rust API
  -> Semantic Scene
  -> Execution Plan
  -> Runtime
  -> Renderer
  -> native platform host / surface
```

That path is typed and in-memory.

The same Rust-authored scene semantics must also be able to run when the Rust engine is compiled to WASM:

```text
Rust/WASM API
  -> Semantic Scene
  -> Execution Plan
  -> Runtime
  -> Renderer
  -> browser/WASM host / canvas
```

When these layers share one process or one WASM execution context, every arrow above is a typed in-process Rust boundary.

Do not introduce serialization/deserialization, JSON, scene documents, execution mirrors, wire payloads, IPC, WASM bindings, or frontend bridges as ordinary boundaries between these layers.

A browser target by itself is **not** a transport boundary. JavaScript may bootstrap WASM and supply browser objects such as a canvas, but it must not receive and re-send scene/runtime state between Rust engine layers.

Serialization is acceptable only at explicit external boundaries such as debugging, export/import, test fixtures/goldens, persisted interchange, or genuine cross-context transport such as a separate Python/Pyodide worker communicating with an execution/render worker. Serializable data types do not imply that serialization is an engine boundary.

Python and future JavaScript/TypeScript APIs are optional language wrappers over shared Rust semantic operations. They are not required engine components.

Direct native/web execution work is owned by `#969` during Phase A.

## 4. The four authorities

There is exactly one authority at each architecture layer:

1. **Semantic Scene** — authored meaning and semantic identity.
2. **Execution Plan** — lowered, specialized renderer-independent execution data.
3. **Runtime** — current mutable execution state.
4. **Renderer** — derived retained GPU state and draw work.

Platform hosts are integration shells around those layers. They own platform lifecycle, not semantic/runtime truth.

Do not create:

- another authored scene representation;
- another semantic ID allocator;
- frontend-owned semantic state;
- frontend-owned animation scheduling/interpolation;
- platform-host-owned semantic or scheduling state;
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

## 6. Frontend ownership and paired examples

Python may own Python-specific ergonomics such as class shape, signatures, argument coercion, exception mapping, Python callable identity, and invocation of arbitrary Python callbacks.

Python must not own a second valid scene, semantic object state, semantic IDs, family traversal semantics, painter ordering, layout/bounds, animation scheduling, interpolation, rollback state, or renderer/runtime state.

Future JavaScript/TypeScript follows the same rule.

If equivalent Rust and Python authoring produce different semantic behavior, fix the shared Rust semantics rather than implementing two engines.

For significant common semantics supported by both public APIs, maintain equivalent executable Rust and Python examples:

- the Rust example is the direct first-class product/engine proof;
- the Python example proves the wrapper reaches equivalent shared Rust semantics;
- both should exercise the normal shared lowering/runtime/renderer path;
- target bootstrap may differ, but scene semantics should remain equivalent;
- representative Rust examples should run through both native and direct Rust/WASM targets where applicable;
- migration-era `SceneDefinition`, scene-document, `noon-ir`, or other serialized scene paths are not the canonical parity mechanism.

An implementation is not complete merely because only one frontend can demonstrate shared behavior that is intended to exist in both.

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
- `#956` — Phase D: 3D and broader capability on the same engine;
- `#969` — Phase A2/A6: direct Rust execution hosts for native and web.

During Phase A, avoid broad feature expansion unless the work is a correctness fix or architecture-neutral.

Do not resurrect closed architecture issues or old PR branches wholesale. Replay only the useful behavior onto the current architecture.

## 10. Existing code is not automatically correct

The repository is in active architectural consolidation.

Do not infer the target design solely from existing types, tests, modules, JSON paths, compatibility functions, crate boundaries, browser plumbing, or renderer mirrors.

In particular, legacy scene models, `SceneDefinition`/`SceneSpec`-style structures, retained sidecars, migration JSON APIs, execution mirrors, and compatibility adapters may exist only because Phase A has not deleted them yet.

The current browser renderer using JSON/transport does not make that transport mandatory for direct single-context Rust/WASM execution.

When new architecture makes old code unnecessary, remove it rather than wrapping it.

## 11. Crates, modules, renderer, and platform hosts

Crates require a real dependency, compilation-target, or reuse boundary.

Prefer modules over crates for conceptual organization alone.

Do not preserve a crate merely because another crate currently imports it. First ask whether the architecture still requires the boundary.

Module structure should make ownership obvious. Avoid `#[path]` and `include!` structures that hide unrelated domains behind one module.

`noon-render-wgpu` owns reusable retained GPU rendering. It may own renderer-local camera/viewport data, GPU resources, preparation/upload logic, command encoding, and disposable renderer caches.

It must not become the application/platform shell merely for convenience. Platform integration owns concerns such as:

- native OS window or browser canvas lifecycle;
- application/browser event loop and frame scheduling;
- wgpu instance/surface/adapter/device/queue/configuration where platform integration owns them;
- resize and input translation;
- surface/frame acquisition, queue submission, presentation, and recovery policy.

Native host integration may be a separate crate only if window/platform dependencies create a real compilation/dependency boundary. Do not create `noon-native` merely for naming symmetry with `noon-web`.

`noon-web` owns browser/WASM platform integration, not semantic or renderer authority. Its direct Rust/WASM path must not require a serialized execution transport when all engine layers share one WASM context.

Text, Graph, interaction, and 3D are features of the same renderer/runtime architecture, not separate engines.

## 12. Local validation

Use the repository validation entrypoint instead of reconstructing CI commands from memory:

```bash
bash scripts/check.sh fast
```

Use `bash scripts/check.sh full` before merge when the change affects Rust plus browser integration or when the owning issue/PR calls for the full local gate. Run narrower focused tests while iterating, but report the exact commands actually run in the PR.

GitHub CI has additional parallel browser, parity, golden, differential, performance, and platform-specific jobs. A local `check.sh` pass does not replace required GitHub checks.

## 13. Tests and validation

Prefer deterministic structural tests over tests that merely make current implementation details permanent.

Protect invariants such as:

- stale generational handles are rejected;
- semantic identity survives valid detach/re-add flows;
- direct seek agrees with forward execution for deterministic semantics;
- mutations are atomic and rollback-safe;
- local mutations remain local;
- retained resources are reused when content did not change;
- Rust and Python authoring share semantic behavior through paired representative examples;
- Manim compatibility is checked at supported semantic/visual boundaries;
- native Rust execution does not depend on serialization or language frontends;
- direct Rust/WASM execution does not serialize between in-process engine layers;
- platform hosts do not become semantic/runtime authorities;
- `noon-render-wgpu` does not become the native/browser application event-loop owner.

JSON-based fixtures are fine when testing an explicit codec/transport/export boundary. Do not make a legacy serialized scene path the canonical engine qualification path.

## 14. PR discipline

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

## 15. Before finishing a task

Before declaring work complete:

1. Re-read the relevant section of `docs/architecture.md`.
2. Check whether the change created a second authority, identity space, scheduler, runtime, renderer model, platform-host authority, or serialization boundary.
3. If browser-targeted, ask whether there is a genuine cross-context boundary; do not introduce transport merely because the code runs in a browser.
4. For common semantics supported in Rust and Python, add/update equivalent executable examples and shared parity evidence.
5. For Rust execution work, preserve both the native and direct Rust/WASM typed paths where applicable.
6. Search for obsolete code made unnecessary by the change and delete it when safe within scope.
7. Ensure temporary compatibility code has an explicit deletion owner.
8. Add focused tests for the invariant or behavior changed.
9. Check that local operations remain local.
10. Run the appropriate `scripts/check.sh` mode and any focused validation required by the owning issue.
11. Update the owning GitHub issue/PR with what landed and what remains.

When uncertain, choose the design that keeps one semantic authority, one lowering boundary, one runtime, one reusable renderer, platform hosts as lifecycle-only shells, and typed in-process Rust boundaries on both native and direct single-context Rust/WASM paths.
# Continuous integration policy

Noon separates fast pull-request feedback from exhaustive post-merge validation. Pull requests should get one high-signal result quickly; broader compatibility, recovery, coverage, release, and cross-browser matrices remain available without forcing every change to pay their startup cost.

## Pull-request fast gate

`.github/workflows/pr-fast.yml` is the only workflow triggered for every pull request. It targets a warm-run wall clock of at most two minutes.

The workflow runs four independent lanes in parallel:

- **Rust format and lint:** `cargo fmt --all -- --check` and workspace Clippy with all targets/features;
- **Rust unit tests:** `cargo test --workspace --all-features --lib`;
- **Web static and unit checks:** ownership ratchets, JavaScript syntax/unit tests, and Python compile/unit tests;
- **Browser fast checks:** one symbol-free dev-profile WASM/browser package build followed by deterministic replay, Manim-style authoring, primary WebGPU smoke coverage, and measured high-risk integration escalation when the changed files require it.

The four real lane results are the PR validation signals; there is deliberately no fifth aggregate runner whose only work is to re-check already-published job conclusions. If branch protection is configured, require these lane checks directly. Rust lint and unit-test compilation remain separate because Clippy and the test profile cannot reuse enough artifacts to justify serializing roughly independent compile work on the critical path. The web preflight is similarly independent of the WASM build and browser runtime, so it runs beside rather than ahead of that work.

`scripts/build-web-demo.sh` remains the canonical full web validation entry point. `NOON_WEB_PREFLIGHT_ONLY=1` runs only its static/unit preflight, while `NOON_SKIP_WEB_PREFLIGHT=1` skips that preflight and proceeds directly to the package build. On that package-only path, Python-worker generation runs concurrently with the much longer `wasm-pack` build and the script waits for both before validating the package. This preserves the generated worker required by the browser page lifecycle while hiding its setup cost behind WASM compilation. Full/preflight builds retain their synchronous worker-before-validation ordering. `NOON_WASM_PROFILE=dev` explicitly selects wasm-pack's development profile for the PR feedback and ordinary master-integration paths; the script itself still defaults to `release` and rejects unknown values. Production release semantics remain covered by the dedicated platform/release workflow.

Deterministic replay keeps the full workload in the PR gate: the same playground corpus, all authored objects including the 1,000-object morph stress scene, every direct-seek target and rewind check, and 32 incremental forward samples per target. The browser harness calls one Rust verifier per scene. That verifier decodes and compiles the scene once, keeps independent direct/forward/rewind runtime instances resident, and compares renderer-observable `FrameState` values in Rust. This preserves the exact determinism contract while avoiding repeated scene compilation and large normalized-frame JSON serialization across the WASM boundary solely for equality checks.

### Change-aware browser escalation

Expensive integration checks belong in the existing browser lane only when their unique failure surface is affected. `scripts/pr-risk-classifier.mjs` receives the pull request's changed paths and publishes the browser-risk decisions consumed by the workflow. Its policy is unit-tested in the normal web preflight so path ownership cannot silently drift.

The first escalation is the canonical retained-execution worker smoke. It exercises canonical `SceneSpec` startup, retained resource installation, transferable/shared execution transport, engine reconnect, render recovery, and the canonical retained startup contract. The current browser topology uses the shared `ExecutionWorkerClient`, the retained engine worker, and the shared render-owner implementation reached through `execution-render-worker.js` and `authoring-render-worker.js`. Changes to those ownership boundaries, retained routing/transport support, `noon-web`'s retained WASM export boundary, or retained-specific Rust source paths require the smoke. Generic documentation, CI, demo, geometry, the legacy-only execution engine worker, and Manim bridge work remain on the ordinary fast path.

This classification is intentionally dependency-oriented rather than a copy of one historical pull request's file list. When a retained execution dependency moves or a new ownership boundary is introduced, update the classifier and its tests in the same change. Do not broaden it to an entire language or top-level directory merely to avoid maintaining the architectural boundary. PR #811 is an example of the boundary evolving: it retired the duplicate retained client/render-worker topology in favor of the shared execution client and render owner, and the classifier was updated to follow that authoritative path.

The retained smoke was initially unconditional and measured about 16 seconds in PR #808, pushing that browser path to 117 seconds. Making it an escalation restores that margin for ordinary changes while preserving the full retained integration contract on the changes most capable of breaking it. The timing summary reports the retained phase as `skipped` when the classifier does not require it.

## Compiler caches

Native and WASM compilation use different cache transports because measurements showed different behavior.

The Rust lint/test lanes continue to consume the GitHub Actions sccache backend in `READ_ONLY` mode. Native PR measurements already show high hit rates and remain under the two-minute budget, so adding large archive restores to both Rust jobs would increase bandwidth without solving the current critical path. Main CI remains the trusted native GHA-cache writer.

The browser lane restores one trusted WASM build bundle through `actions/cache/restore`. `.github/workflows/compiler-cache-seed.yml` is the only workflow allowed to publish that bundle. Relevant pushes to `master` restore the latest trusted bundle, compile the same symbol-free dev WASM profile used by PRs, and save an immutable archive keyed by the master SHA. The bundle contains both the local sccache object directory and `wasm-pack`'s global helper-tool cache. PRs request the bundle for their exact base SHA and may fall back to the latest default-branch seed. Because PRs use the restore-only action and `SCCACHE_LOCAL_RW_MODE=READ_ONLY`, they cannot publish branch-local build archives.

The archive boundary is intentional. The per-object GHA backend successfully reused native objects, but an exact-master WASM build could see 315/315 hits while a source-neutral PR saw 0/315. Packaging the local sccache directory into one default-branch cache entry gives the WASM path an explicit master-to-PR restore contract and avoids hundreds of per-object cache uploads. Keeping `wasm-pack`'s helper cache in the same bundle avoids another steady-state cache action and makes tool lookup effectively free.

The combined cache rollout has completed, so there is no legacy sccache-only restore path in steady state. A source-neutral probe against the first combined master seed restored the exact bundle and produced 315/315 WASM compiler-cache hits. Instrumented follow-up showed that wasm-pack's `wasm-bindgen` helper lookup from `~/.cache/.wasm-pack` completed essentially immediately; the remaining roughly 18-second phase was actual `wasm-bindgen` execution over Noon's roughly 147 MB development WASM module, not a cache miss or helper download. Disabling wasm-bindgen's dev JS-glue debug flag did not improve that runtime and is therefore not part of the CI profile.

The WASM compiler cache remains capped at 1 GiB; the helper-cache portion is versioned by `wasm-pack` itself inside its global cache. Seed runs are non-cancelling so rapid master merges cannot abort the only archive writer in progress; newer relevant master pushes may queue another seed. Main CI and specialized validators are consumers rather than WASM archive owners.

When changing compiler-cache policy, validate it with a source-neutral pull request after a default-branch seed has completed so cache transport behavior is measured independently from source invalidation. Keep that probe outside Cargo manifests, Rust sources, workflow definitions, and build scripts so it does not perturb the compiler inputs being measured.

## Main CI workflow

`.github/workflows/ci.yml` runs on pushes to `master` and manual dispatch. It is the full Linux integration workflow rather than the pull-request inner loop.

Its Rust lane runs formatting, full workspace Clippy, and the complete workspace test set while maintaining the native compiler cache. Its browser build runs independently, restores the latest trusted WASM archive read-only, produces the same dev-profile integration package used by the PR fast path, and uploads one reusable package artifact that fans out to authoring, reactive/editor, WebGPU, WebGL, and backend-parity jobs.

The browser build no longer waits for the Rust lane merely to serialize compiler-cache writes: native still uses the GHA backend, while WASM uses a separate archive owned by the seed workflow. This removes an unnecessary post-merge dependency and prevents the browser cache from being starved by cancellation before it starts.

This workflow is intentionally broader than the PR fast gate. A failure after merge is still a correctness failure to fix immediately, but expensive validation and cache population do not delay every review iteration.

## Specialized validation

Feature-specific workflows under `.github/workflows/` do not independently cold-start on every pull request. They run on their documented `master`, scheduled, or manual triggers.

This keeps their focused contracts intact while avoiding repeated checkout, Rust/WASM setup, browser installation, and `build-web-demo.sh` invocations for the same PR head. If a future class of change proves too risky to defer, add measured change-aware escalation to the fast workflow instead of adding another unconditional PR workflow.

## Platform and release validation

`.github/workflows/platform-release.yml` is the portability and production-artifact lane. It runs on relevant pushes to `master` and by manual dispatch.

It provides three distinct contracts:

- Linux, macOS, and Windows native workspace validation;
- release-mode testing for core execution crates;
- a production browser artifact built with pinned `wasm-pack` and Binaryen/`wasm-opt`, then exercised in Chromium.

The optimized job records WASM size as diagnostic trend data. Size should become a ratchet only after a stable baseline exists.

## Manim compatibility workflows

Manim compatibility workflows validate Noon against the pinned Manim Community reference independently of the fast PR loop. API inventory, semantic differential, raster differential, and reference-inventory checks remain separate because surface presence, semantic behavior, rendered output, and corpus provenance are different contracts.

The expensive reference environment belongs on `master` or manual validation unless a measured change classifier explicitly escalates a PR.

## Test inventory and coverage

`.github/workflows/test-coverage.yml` is the test-observability workflow. On `master` and manual runs it produces:

- generated test inventory data;
- Rust LCOV coverage;
- Python coverage;
- JavaScript/V8 coverage.

Coverage is not duplicated in the required fast gate. Prefer changed-code or per-layer non-regression ratchets over an arbitrary repository-wide percentage.

See `docs/testing.md` for the verification hierarchy and local commands.

## Fuzzing and recovery

Fuzzing remains scheduled/manual, and recovery/cross-browser workflows remain post-merge/manual validation. Their purpose is to exercise failure and environment matrices that are valuable but structurally unsuitable for a two-minute review loop.

## Performance policy

Shared hosted runners are noisy, so correctness tests should prefer deterministic structural invariants over tiny wall-clock thresholds. CI architecture itself is different: the PR feedback budget is a developer-productivity contract, so workflow wall time should be measured and regressions in the critical path should be visible.

Useful runtime invariants include objects/tracks visited, dirty slots, upload bytes, cache misses, callback payload sizes, and retained resource counts. Named-machine p50/p95 measurements remain diagnostic/release evidence and are documented in `docs/performance.md`.

## Development cadence

Use local focused tests as the inner loop, the four `PR Fast Gate` workflow lanes as the pre-merge integration signal, and the exhaustive master workflows as the full correctness matrix. Extend CI by adding tests to existing ownership lanes or change-aware escalation; avoid introducing another independently bootstrapped unconditional pull-request workflow.
# Continuous integration policy

Noon separates fast pull-request feedback from exhaustive post-merge validation. Pull requests should get one high-signal result quickly; broader compatibility, recovery, coverage, release, and cross-browser matrices remain available without forcing every change to pay their startup cost.

## Pull-request fast gate

`.github/workflows/pr-fast.yml` is the only workflow triggered for every pull request. It targets a warm-run wall clock of at most two minutes.

The workflow runs four independent lanes in parallel:

- **Rust format and lint:** `cargo fmt --all -- --check` and workspace Clippy with all targets/features;
- **Rust unit tests:** `cargo test --workspace --all-features --lib`;
- **Web static and unit checks:** ownership ratchets, JavaScript syntax/unit tests, and Python compile/unit tests;
- **Browser fast checks:** one symbol-free dev-profile WASM/browser package build followed by deterministic replay, Manim-style authoring, and primary WebGPU smoke coverage.

The four real lane results are the PR validation signals; there is deliberately no fifth aggregate runner whose only work is to re-check already-published job conclusions. If branch protection is configured, require these lane checks directly. Rust lint and unit-test compilation remain separate because Clippy and the test profile cannot reuse enough artifacts to justify serializing roughly independent compile work on the critical path. The web preflight is similarly independent of the WASM build and browser runtime, so it runs beside rather than ahead of that work.

`scripts/build-web-demo.sh` remains the canonical full web validation entry point. `NOON_WEB_PREFLIGHT_ONLY=1` runs only its static/unit preflight, while `NOON_SKIP_WEB_PREFLIGHT=1` skips that preflight and proceeds directly to the package build. On that package-only path, Python-worker generation runs concurrently with the longer `wasm-pack` build and the script waits for both before validating the package. This preserves the generated worker required by the browser page lifecycle while hiding its setup cost behind WASM compilation. Full/preflight builds retain their synchronous worker-before-validation ordering. `NOON_WASM_PROFILE=dev` explicitly selects wasm-pack's development profile for the PR feedback and ordinary master-integration paths; the script itself still defaults to `release` and rejects unknown values. Production release semantics remain covered by the dedicated platform/release workflow.

Deterministic replay keeps the full workload in the PR gate: the same playground corpus, all authored objects including the 1,000-object morph stress scene, every direct-seek target and rewind check, and 32 incremental forward samples per target. The browser harness calls one Rust verifier per scene. That verifier decodes and compiles the scene once, keeps independent direct/forward/rewind runtime instances resident, and compares renderer-observable `FrameState` values in Rust. This preserves the exact determinism contract while avoiding repeated scene compilation and large normalized-frame JSON serialization across the WASM boundary solely for equality checks.

Cargo source downloads and Rust compilations use the shared cache/sccache setup. PR jobs consume the GitHub Actions sccache backend in `READ_ONLY` mode: review iterations should benefit from trusted default-branch compiler artifacts without creating hundreds of branch-local cache entries or competing for the repository upload-rate budget. The master CI workflow owns compiler-cache population. Its native writer uses the same symbol-free `dev`/`test` profile overrides as the PR Rust lanes, and its browser writer uses the same symbol-free dev WASM profile as the PR browser lane. The two master writers are serialized so they do not compete for cache uploads. Main CI runs on `master` are explicitly `READ_WRITE`; manual dispatches from other refs are read-only. Master CI uses a non-cancelling concurrency queue so a rapid sequence of merges cannot repeatedly kill the only writer before its dependent WASM seed runs. Debug assertions and overflow checks remain those of the ordinary profiles; only debugger symbols are omitted. Production release behavior and optimized browser artifacts remain exercised separately by platform/release validation.

## Main CI workflow

`.github/workflows/ci.yml` runs on pushes to `master` and manual dispatch. It is the full Linux integration workflow rather than the pull-request inner loop.

Its Rust lane runs formatting, full workspace Clippy, and the complete workspace test set while seeding the native compiler cache used by PRs. Its browser build follows that native writer, produces the same dev-profile integration package used by the PR fast path, seeds the WASM compiler cache, and uploads one reusable package artifact that fans out to authoring, reactive/editor, WebGPU, WebGL, and backend-parity jobs. Master runs queue rather than cancelling an in-progress CI run, which guarantees that a cache-population cycle can reach the dependent browser writer even when merges arrive close together. Non-master manual runs may validate CI but cannot publish compiler-cache entries.

This workflow is intentionally broader than the PR fast gate. A failure after merge is still a correctness failure to fix immediately, but expensive validation and cache population do not delay every review iteration.

## Specialized validation

Feature-specific workflows under `.github/workflows/` do not independently cold-start on every pull request. They run on their documented `master`, scheduled, or manual triggers.

This keeps their focused contracts intact while avoiding repeated checkout, Rust/WASM setup, browser installation, and `build-web-demo.sh` invocations for the same PR head. Specialized workflows that use sccache are read-only consumers even on `master`; main CI is the only cache writer. If a future class of change proves too risky to defer, add measured change-aware escalation to the fast workflow instead of adding another unconditional PR workflow.

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

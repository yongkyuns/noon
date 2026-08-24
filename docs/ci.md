# Continuous integration policy

Noon uses CI as a correctness gate, but not as the inner development loop. The required workflow is split by subsystem so expensive browser work fans out after one reusable WASM build instead of rebuilding the package in every job.

## Main CI workflow

`.github/workflows/ci.yml` runs on pull requests, pushes to `master`, and manual dispatch.

### Rust

The Rust job runs on Ubuntu and requires:

- `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-features`.

Cargo sources and compiled Rust work are accelerated with sccache. The workspace is treated as one current architecture; there is no separate legacy compatibility gate.

### Browser package build

The web-build job:

- syntax-checks the checked-in JavaScript harnesses;
- runs Node unit tests;
- compiles/checks the Python compatibility modules and examples;
- runs the Python unittest suite;
- builds `noon-web` for `wasm32-unknown-unknown` with pinned `wasm-pack`;
- validates the generated JavaScript/TypeScript/WASM package surface;
- uploads the package once for downstream browser jobs.

PR CI intentionally skips `wasm-opt` in this fast package build. The production-optimized artifact is validated separately by the release/platform testing lane rather than making every browser job pay the optimization cost.

### Browser authoring

The authoring job consumes the shared browser package and runs:

- Rust/Python semantic parity;
- Manim-style Python compatibility;
- composition authoring;
- the executable Manim tutorial corpus.

### Browser reactive/editor

The reactive/editor job runs:

- native reactive Python authoring;
- native reactive browser runtime;
- Python updater callback bridge;
- Python editor syntax-highlighting/linting smoke coverage.

### Browser rendering

Rendering is a two-entry matrix over WebGPU and WebGL2 fallback. Both run the deterministic playground/browser smoke corpus and upload screenshots on success or failure. The required Linux browser path uses Playwright Chromium/SwiftShader so it is repeatable on hosted runners.

## Manim compatibility workflows

Manim API-surface and semantic compatibility are separate workflows because they require a pinned ManimCE/Python environment. Manim Community v0.21.0 is the reference version. API inventory and semantic differential tests should remain separate: surface presence does not prove behavior, and behavioral fixtures should not become a second API manifest.

## Test inventory and coverage

`.github/workflows/test-coverage.yml` is the observability workflow introduced by #104. It runs in parallel with normal CI when production/test files change and produces:

- a generated machine-readable/Markdown test inventory;
- Rust LCOV data from `cargo llvm-cov`;
- Python coverage from the normal unit suite;
- V8/c8 coverage for browser JavaScript unit modules.

The inventory has a small required ratchet now: every production module must belong to an explicit test strategy. Line coverage is initially diagnostic. After a stable baseline is established, use changed-code or per-layer non-regression ratchets rather than an arbitrary global percentage.

See `docs/testing.md` for the verification hierarchy and local commands.

## Performance policy

Shared GitHub runners are too noisy for small wall-clock regressions to be a reliable correctness gate. Required CI should prefer deterministic structural invariants such as:

- objects/tracks/reactive nodes visited;
- dirty slots and instances repacked;
- upload bytes;
- geometry/cache misses and rebuild counts;
- host callback slots/payload size;
- retained resource counts.

Named-machine p50/p95 measurements remain useful diagnostic/release evidence and are documented in `docs/performance.md`.

## Development cadence

Use local focused tests as the inner loop and update branches in coherent batches. A change should run the cheapest relevant subsystem tests first; the full required CI is the integration gate before merge. Avoid serializing tiny formatting/lint-only commits solely to trigger Actions repeatedly.

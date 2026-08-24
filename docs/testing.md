# Testing strategy

Noon uses several different correctness oracles because no single test style is sufficient for an interactive animation engine. Prefer the cheapest deterministic oracle that can prove a behavior and escalate to browser/raster testing only when the lower layer cannot observe the contract.

## Test pyramid

1. **Unit and structural tests** validate pure semantic, geometry, compiler, runtime, cache, and protocol invariants.
2. **Integration tests** exercise crate boundaries, authoring lowering, patch/reconcile behavior, and shared runtime semantics.
3. **Property/model tests** cover stateful mutation spaces where hand-written examples are incomplete.
4. **Cross-layer differential/equivalence tests** compare independently implemented paths such as Rust/Python, native/WASM, direct seek/playback, and Noon/ManimCE.
5. **Browser E2E tests** validate the deployed authoring/player stack and user-visible workflow.
6. **Visual goldens** are reserved for raster-visible behavior that cannot be asserted structurally.
7. **Performance measurements** use deterministic work counters in required CI and wall-clock measurements only on named/diagnostic runners.

## Current required gates

The main `CI` workflow runs:

- workspace formatting, strict Clippy, and Rust tests;
- the browser/WASM package build and package-surface validation;
- Node unit tests and Python unit tests as part of the web build;
- Rust/Python semantic parity, Manim compatibility, composition authoring, and the executable tutorial corpus;
- native reactive authoring/runtime, updater callback bridge, and Python editor smoke tests;
- browser rendering on both WebGPU and forced WebGL2 fallback paths.

The separate Manim workflows pin ManimCE v0.21.0 for API-surface and semantic differential coverage.

## Test inventory

`scripts/test-inventory.py` scans production modules and test entry points across Rust, JavaScript, Python, browser smoke, and compatibility tooling. It emits JSON and Markdown reports and fails `--check` when a production module has no explicit layer/module test strategy.

Run it locally with:

```bash
python3 scripts/test-inventory.py \
  --check \
  --json test-artifacts/test-inventory.json \
  --markdown test-artifacts/test-inventory.md
```

The inventory is intentionally not a line-coverage number. It answers a different question: **which verification layer owns each production module, and what kinds of tests exist?**

## Coverage reports

`.github/workflows/test-coverage.yml` produces diagnostic line-coverage artifacts for the three source-language layers:

- Rust: `cargo llvm-cov` LCOV output;
- Python: `coverage.py` JSON/text output from the unit suite;
- browser JavaScript unit modules: V8 coverage through `c8`.

Generated wasm-bindgen output is not included. Coverage is initially a baseline/reporting signal rather than a global pass/fail percentage. Once the baseline is stable, ratchets should be applied per layer or to changed code; Noon should not optimize for an arbitrary repository-wide percentage.

## Adding production code

A new production module must have an explicit test strategy visible to the inventory. Depending on the behavior, that may be an inline unit suite, crate integration suite, Python/JS module suite, browser smoke/E2E coverage, or a combination.

New compatibility surface should update the existing Manim coverage/differential infrastructure rather than adding a separate compatibility tracker. New renderer-visible behavior should reuse the shared browser/golden infrastructure rather than inventing feature-local screenshot tooling.

## Performance testing policy

Shared GitHub runners are noisy, so required correctness CI should not fail on small p50/p95 timing differences. Gate complexity with deterministic counters—objects/tracks visited, slots repacked, upload bytes, cache misses, callback slots, resource churn—then use the named-machine harnesses in `docs/performance.md` to measure wall-clock trends.

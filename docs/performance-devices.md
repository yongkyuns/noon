# Named-device performance runs

Shared CI protects correctness, not wall-clock budgets. Browser performance baselines must come from an identified physical machine and be saved as artifacts with the commit, host CPU/OS, browser user agent, renderer backend, backing resolution and observed DPR.

The known profiles live in `benchmarks/performance-devices.json`. A new machine does not need to be committed before experimentation, but a result used as a long-lived baseline should get a stable profile ID.

## Record a baseline locally

Build the optimized release web package and install the pinned Playwright dependencies used by the browser smoke tests, then run:

```bash
NOON_PERF_DEVICE_ID=my-machine node scripts/perf-device-run.mjs
```

The default records both the canonical synthetic frame matrix and the realistic authored-scene corpus on WebGPU and WebGL2, so a named-device bundle contains object-scaling data and normal user-facing scenes together. Control the matrix with the existing `NOON_PERF_*` variables, for example:

```bash
NOON_PERF_DEVICE_ID=my-machine \
NOON_PERF_BACKENDS=webgpu \
NOON_PERF_WIDTH=1920 \
NOON_PERF_HEIGHT=1080 \
NOON_PERF_DPR=2 \
NOON_PERF_COUNTS=1000,10000,100000 \
node scripts/perf-device-run.mjs
```

`NOON_PERF_DPR` drives Playwright's `deviceScaleFactor` for the canonical frame suite and is recorded both in its report and in the named-device manifest. Use DPR 1 for the normal resolution matrix and a separate DPR 2 run for the high-density display case.

Use `NOON_PERF_SUITES` to choose `frame`, `corpus`, and/or `authoring`. A complete release characterization can run all three:

```bash
NOON_PERF_DEVICE_ID=my-machine \
NOON_PERF_SUITES=frame,corpus,authoring \
node scripts/perf-device-run.mjs
```

The bundle manifest and per-suite JSON files are written under `perf-artifacts/<device>/<timestamp>/` by default.

## Run on a physical self-hosted GitHub runner

`.github/workflows/perf-physical-device.yml` is a manual `workflow_dispatch` workflow that targets **only** `self-hosted` runners. It installs the pinned browser tooling, builds optimized production WASM with `scripts/build-web-demo.sh`, runs the named-device bundle, and uploads the result as a workflow artifact.

When registering a real machine as a self-hosted runner, dispatch **Physical device performance baseline** and provide a stable device ID, backend/suite set, backing resolution, DPR, target refresh rate, and power/thermal notes. Use separate dispatches for the 960x540 DPR1, 1920x1080 DPR1, and representative DPR2 cases so same-host trends remain comparable.

Do not change that workflow to a GitHub-hosted runner for baseline collection. A hosted macOS runner can report an Apple CPU while still being `VirtualMac`/`VMAPPLE`, and software adapters such as SwiftShader are useful only for correctness/diagnostics, not real-GPU throughput evidence.

## Compare two runs

```bash
node scripts/perf-compare.mjs old.json new.json
```

The comparator aligns equivalent workload cases and reports deltas for frame p95/p99, effective FPS, CPU/GPU timing, or authoring time-to-visible metrics. An optional diagnostic threshold can make the command non-zero:

```bash
NOON_PERF_REGRESSION_PCT=10 node scripts/perf-compare.mjs old.json new.json
```

That threshold is appropriate for controlled same-machine comparisons. It should not be used as a shared-runner CI gate.

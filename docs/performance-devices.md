# Named-device performance runs

Shared CI protects correctness, not wall-clock budgets. Browser performance baselines must come from an identified physical machine and be saved as artifacts with the commit, host CPU/OS, browser user agent, renderer backend, backing resolution and observed DPR.

The known profiles live in `benchmarks/performance-devices.json`. A new machine does not need to be committed before experimentation, but a result used as a long-lived baseline should get a stable profile ID.

## Record a baseline

Build the release web package and install the pinned Playwright dependencies used by the browser smoke tests, then run:

```bash
NOON_PERF_DEVICE_ID=my-machine node scripts/perf-device-run.mjs
```

The default records both the canonical synthetic frame matrix and the realistic authored-scene corpus on WebGPU and WebGL2, so a named-device bundle contains object-scaling data and normal user-facing scenes together. Control the matrix with the existing `NOON_PERF_*` variables, for example:

```bash
NOON_PERF_DEVICE_ID=my-machine \
NOON_PERF_BACKENDS=webgpu \
NOON_PERF_WIDTH=1920 \
NOON_PERF_HEIGHT=1080 \
NOON_PERF_COUNTS=1000,10000,100000 \
node scripts/perf-device-run.mjs
```

Use `NOON_PERF_SUITES` to choose `frame`, `corpus`, and/or `authoring`. A complete release characterization can run all three:

```bash
NOON_PERF_DEVICE_ID=my-machine \
NOON_PERF_SUITES=frame,corpus,authoring \
node scripts/perf-device-run.mjs
```

The bundle manifest and per-suite JSON files are written under `perf-artifacts/<device>/<timestamp>/` by default.

Do not relabel SwiftShader, software rasterization, or a generic hosted runner as a physical-device baseline. If the browser cannot expose the selected GPU identity, record the result at host/backend level rather than guessing the adapter.

## Compare two runs

```bash
node scripts/perf-compare.mjs old.json new.json
```

The comparator aligns equivalent workload cases and reports deltas for frame p95/p99, effective FPS, CPU/GPU timing, or authoring time-to-visible metrics. An optional diagnostic threshold can make the command non-zero:

```bash
NOON_PERF_REGRESSION_PCT=10 node scripts/perf-compare.mjs old.json new.json
```

That threshold is appropriate for controlled same-machine comparisons. It should not be used as a shared-runner CI gate.

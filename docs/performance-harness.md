# End-to-end performance harness

This document describes the browser FPS/frame-time harness tracked by #125. It complements `docs/performance.md`: deterministic structural counters remain the right CI gate for complexity regressions, while wall-clock FPS and stage timings belong on named machines or controlled real devices.

## What the harness measures

Every measured browser case reports:

- effective FPS and requestAnimationFrame interval p50/p95/p99/max;
- long-frame rate and estimated missed vsyncs against a configurable target refresh rate;
- browser-visible synchronous `renderFrame` call duration;
- Noon CPU frame time split into runtime evaluation, frame preparation, upload, and encode/submit;
- draw calls, rendered instances, upload bytes, and geometry-cache misses;
- WebGPU render-pass p50/p95 (and p99 when exposed by the runtime) when timestamp queries are supported;
- browser/backend/resolution/DPR metadata.

The runner records host commit/OS/CPU metadata in the aggregate artifact. Do not compare absolute timing numbers across unlike machines as if they were regressions.

## Canonical analytic workloads

The initial workload family deliberately contains three layouts so different bottlenecks can be isolated:

- `fit`: circles fill the view and shrink as object count rises. This is useful for instance/object-count scaling with bounded pixel pressure.
- `fixed`: camera scale and circle radius remain fixed while object count rises. This prevents high-count cases from becoming artificially cheap simply because every object becomes sub-pixel.
- `overdraw`: many translucent circles overlap in a small screen region. This intentionally stresses fragment shading and alpha blending.

These are synthetic attribution workloads, not claims about normal authored scene complexity. Realistic Manim-style cases are tracked separately by #134 and should consume the same result schema.

## Run the matrix

Build the optimized browser package first:

```bash
bash scripts/build-web-demo.sh
```

Install Playwright if it is not already available in the working tree, then run:

```bash
node scripts/perf-profile.mjs
```

By default this measures 1k, 10k, and 100k objects for all three analytic layouts, with 30 warmup frames and 300 measured frames at a 960x540 backing resolution.

Useful environment overrides:

```text
NOON_PERF_BACKEND=webgpu|webgl
NOON_PERF_COUNTS=1000,10000,100000
NOON_PERF_LAYOUTS=fit,fixed,overdraw
NOON_PERF_WARMUP=30
NOON_PERF_FRAMES=300
NOON_PERF_TARGET_HZ=60
NOON_PERF_WIDTH=960
NOON_PERF_HEIGHT=540
NOON_PERF_ARTIFACT=perf-artifacts/perf-profile-webgpu.json
```

For a quick smoke run on a developer machine:

```bash
NOON_PERF_COUNTS=1000 NOON_PERF_LAYOUTS=fit NOON_PERF_FRAMES=60 node scripts/perf-profile.mjs
```

## Interpretation

Do not attribute a bottleneck from FPS alone. Compare the frame interval distribution with the stage timings:

- high runtime evaluation points toward timeline/reactive/host execution work (#126);
- high prepare/upload/encode time points toward renderer CPU work (#127);
- low CPU time with high GPU timestamps points toward raster/overdraw/backend limits (#128);
- frame intervals much larger than both engine CPU and GPU time point toward browser scheduling, worker/Python activity, long tasks, or GC (#130/#132);
- vector-heavy workloads require the dedicated geometry/morph attribution in #129;
- Run/edit/seek latency is measured separately by #131.

The initial browser runner profiles static analytic scenes. Other performance lanes should add workload generators and counters while preserving this result vocabulary rather than creating incompatible one-off formats.

## Regression policy

Do not hard-gate shared CI on wall-clock FPS from hosted runners. Use #113 deterministic work counters for structural/locality regressions. Store named-machine artifacts for trend comparison and add deterministic counters whenever profiling discovers an accidental full-scene scan/repack/upload or other algorithmic regression.

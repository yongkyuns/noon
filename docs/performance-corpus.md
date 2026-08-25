# Realistic performance corpus

The synthetic 1k/10k/100k matrices isolate scaling laws. Release performance also needs authored scenes that exercise the same user-facing Python/Manim-style path as the playground. `benchmarks/performance-scenes.json` is the versioned corpus for that purpose.

Each normal playback case names its Python source, stress dimensions, P1-P7 bottleneck domains, classification (`representative`, `adversarial`, or `scalability-only`) and tier. `interactive60` is the normal 60 Hz product target, `heavy60` allows modest misses for intentionally complex user-facing scenes, and `scalability` is diagnostic only. The stress tier must never be used to weaken common-scene budgets.

Run the steady-playback corpus on a named physical machine after building the release web package:

```bash
node scripts/perf-corpus.mjs
```

Select cases or a backend with environment variables:

```bash
NOON_CORPUS_BACKEND=webgpu \
NOON_CORPUS_CASES=getting-started,filled-path-transform \
node scripts/perf-corpus.mjs
```

The runner records Python-worker setup/authoring, scene serialization and player creation, then measures steady production `renderFrame` calls with runtime, preparation, upload, encode/submit, GPU timestamps when available, frame p50/p95/p99, missed-vsync/long-frame rates and browser long tasks.

Budgets are evaluated and stored in the artifact but are diagnostic by default. On a controlled same-machine release baseline they can be enforced with:

```bash
NOON_CORPUS_ENFORCE_BUDGETS=1 node scripts/perf-corpus.mjs
```

Do not enable wall-clock budget enforcement on generic shared CI runners. Correctness CI validates the manifest, source paths, domain coverage and harness syntax. Wall-clock release decisions use the named-device workflow documented in `docs/performance-devices.md`.

## Realtime policy

For the reference 60 Hz run, `interactive60` currently requires frame-interval p95 <= 20 ms, p99 <= 25 ms and a long-frame rate <= 2%. The canonical `FrameMetrics` long-frame definition is 1.5x the target interval, or 25 ms at 60 Hz. CPU/GPU p95 budgets are also recorded so average FPS cannot hide a stage-specific regression.

`heavy60` remains a 60 Hz measurement but has relaxed p95/p99/long-frame limits. `scalability` has no absolute FPS release gate: it exists to expose asymptotic behavior and root cause. A 120 Hz run is a headroom diagnostic on capable displays/devices, not an initial compatibility gate. The manifest records that policy explicitly so a future runner or dashboard can consume it without changing the schema.

## Coverage map

The corpus spans the major root-cause lanes rather than maximizing scene count:

- P1 runtime/timeline/reactivity: getting-started, positioning, staggered choreography and ValueTracker;
- P2 renderer CPU: common analytic scenes, instanced fields and mixed painter order;
- P3 GPU/raster/overdraw: instanced fields plus the adversarial transparent overlap scene;
- P4 vector/reveal/morph: Create shapes, path reveal, filled-path transform and the shared/unique morph stress pair;
- P5 browser/host scheduling: ValueTracker provides the native-reactive browser baseline and `perf_host_updater.py` exercises the Python host-callback fallback through `updater-callback-smoke.mjs`;
- P6 authoring/edit/seek: realistic scene cold-authoring setup is reported by the normal corpus while `authoring-perf.mjs` measures warm rerun, one-object edit, reconciliation and seek latency explicitly;
- P7 allocation/churn: instancing, vector reveal/morph, host callback snapshots/patches and the authoring identity path all contribute representative coverage.

The manifest's `domainCoverage` section is executable policy: CI requires every P1-P7 domain to retain at least one representative workload. Text/math (#83) and axes/plotting (#85) are listed as future coverage and can be added without changing the runner design.

## Anti-benchmark-gaming rules

Keep visible object size and backing resolution fixed when investigating pixel scaling, retain overlapping transparent content as well as distributed grids, preserve visual quality and semantics, and report first-run/edit latency separately from steady playback. Do not remove Python/host work from a scene unless the measured optimization replaces it with a semantically equivalent native path.

Host callback work is not fed to the standalone `NoonCanvasPlayer` profiler because doing so would omit callback phases and report falsely optimistic FPS. Instead the same reusable Python fixture is executed by the host callback runner and is declared as an auxiliary corpus case.

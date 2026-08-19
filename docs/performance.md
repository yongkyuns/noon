# Performance measurement

Timing measurements are intentionally separate from correctness CI because shared runners are noisy. CI protects deterministic structural invariants; this harness establishes release-mode wall-clock baselines on a named machine.

## Scene replacement and patching

Run the native `ScenePlayer` benchmark from the repository root:

```bash
cargo run --release -p noon-web --example scene_player_perf
```

The default workload uses static analytic circles at 1k, 10k, and 100k objects, with two warmups and 10 measured samples per operation. It reports median and p95 latency for:

- initial versioned JSON decode and compile;
- full transactional scene replacement;
- Rust full-document reconciliation containing one style change;
- a one-object style patch;
- a one-object transform patch.

Scene construction, scene serialization, and patch serialization happen outside the timed region. Patch application includes JSON decoding and the runtime's transactional work. Full replacement includes JSON decoding, compilation, and seeking the new runtime to the existing playhead. Rendering and Python/Pyodide execution are not included.

For a quicker or more focused run, pass sample counts and object counts explicitly:

```bash
cargo run --release -p noon-web --example scene_player_perf -- --warmups 2 --samples 10 10000 100000
```

Record the commit, CPU, operating system, Rust version, power mode, and results when using a run as a comparison baseline.

## Baseline: 2026-08-19

Host: Intel Core i7-9750H at 2.60 GHz, macOS 15.5, Rust 1.95.0. The benchmark used the default two warmups and 10 samples in an otherwise normal developer session.

| Objects | Scene JSON | Operation | Median | p95 | Replacement / operation |
|---:|---:|---|---:|---:|---:|
| 1,000 | 0.23 MiB | initial load | 0.890 ms | 0.928 ms | 0.98x |
| 1,000 | 0.23 MiB | full replacement | 0.875 ms | 0.906 ms | 1.00x |
| 1,000 | 0.23 MiB | one style patch | 0.071 ms | 0.088 ms | 12.38x |
| 1,000 | 0.23 MiB | one transform patch | 0.068 ms | 0.081 ms | 12.80x |
| 10,000 | 2.35 MiB | initial load | 7.658 ms | 11.322 ms | 1.23x |
| 10,000 | 2.35 MiB | full replacement | 9.413 ms | 14.378 ms | 1.00x |
| 10,000 | 2.35 MiB | one style patch | 0.698 ms | 0.765 ms | 13.48x |
| 10,000 | 2.35 MiB | one transform patch | 0.849 ms | 1.646 ms | 11.09x |
| 100,000 | 23.64 MiB | initial load | 102.826 ms | 137.976 ms | 1.04x |
| 100,000 | 23.64 MiB | full replacement | 106.447 ms | 140.390 ms | 1.00x |
| 100,000 | 23.64 MiB | one style patch | 19.139 ms | 22.223 ms | 5.56x |
| 100,000 | 23.64 MiB | one transform patch | 17.619 ms | 20.373 ms | 6.04x |

The benchmark exposed quadratic object validation in `SceneDocument` import. Before the bulk-import fix, the same 100k scene took 7,698.540 ms median to replace; afterward it took 106.447 ms, a 72.3x reduction.

Current interpretation:

- full replacement is comfortably interactive at 1k objects and near a 60 Hz frame budget at 10k, but not at 100k;
- one-object patches materially outperform replacement, supporting identity-based reconciliation;
- patches still clone the complete semantic/runtime state transactionally, so their 100k latency is also just above a 16.7 ms frame budget;
- these figures cover CPU-side native scene operations only and make no claim about Pyodide turnaround, renderer preparation, GPU upload, or presentation.

The stable-authoring follow-up also measured Rust-side full-document reconciliation. It was slower than replacement because it pays for full JSON decode, diffing, and transactional cloning together: 1.159 ms at 1k, 13.922 ms at 10k, and 130.763 ms at 100k. Therefore the browser's normal compatible-rerun path diffs the already-parsed worker result on the main thread and sends the small semantic `PatchBatch` measured above. Rust full-document reconciliation remains the correctness fallback for the first keyed run and unsafe changes.

## Value-patch fast path

After adding preflighted in-place transactions for style and transform patches, the same baseline machine produced:

| Objects | Style patch median | Transform patch median |
|---:|---:|---:|
| 1,000 | 0.001 ms | 0.001 ms |
| 10,000 | 0.013 ms | 0.014 ms |
| 100,000 | 0.127 ms | 0.116 ms |

At 100k objects this is roughly 150x faster than the original cloning path and more than 130x below a 16.7 ms frame budget. The fast path preflights every referenced object before mutation and then updates only the affected compiled/base/frame fields, reapplying active position, rotation, or opacity tracks for that object. Structural create/remove/track batches still use the conservative whole-state clone fallback.

## Incremental frame preparation and upload volume

Run the renderer preparation benchmark from the repository root:

```bash
cargo run --release -p noon-render-wgpu --example frame_preparation_perf
```

It compares a complete packed-instance rebuild with an unchanged frame and a one-object transform change. The upload column is the exact payload passed to `wgpu::Queue::write_buffer`; it is a structural byte count, not a GPU completion-time estimate.

On the same 2026-08-19 baseline machine (10 warmups, 100 samples):

| Objects | Operation | Median | p95 | Instances repacked | Upload bytes |
|---:|---|---:|---:|---:|---:|
| 1,000 | full rebuild | 0.011037 ms | 0.011116 ms | 1,000 | 88,000 |
| 1,000 | unchanged | 0.000065 ms | 0.000067 ms | 0 | 0 |
| 1,000 | one changed | 0.000090 ms | 0.000091 ms | 1 | 88 |
| 10,000 | full rebuild | 0.113858 ms | 0.114862 ms | 10,000 | 880,000 |
| 10,000 | unchanged | 0.000061 ms | 0.000062 ms | 0 | 0 |
| 10,000 | one changed | 0.000086 ms | 0.000088 ms | 1 | 88 |
| 100,000 | full rebuild | 1.539001 ms | 2.058757 ms | 100,000 | 8,800,000 |
| 100,000 | unchanged | 0.000061 ms | 0.000062 ms | 0 | 0 |
| 100,000 | one changed | 0.000079 ms | 0.000080 ms | 1 | 88 |

For a static 100k-object scene, frame preparation is now constant-time after the initial build and the per-frame instance upload falls from 8.8 MB to zero. A single transform/style change repacks and uploads one 88-byte instance record. At 60 Hz, that removes the previous static-scene upload pressure of roughly 528 MB/s. These measurements isolate CPU instance preparation; they do not include runtime evaluation, browser/Pyodide work, command encoding, rasterization, or presentation.

## Browser worker transfer and scene diff

Run the JavaScript scene-pipeline benchmark from the repository root:

```bash
node web/scene-pipeline-perf.mjs
```

The benchmark uses a Node `MessageChannel` to isolate structured-clone delivery from JSON parsing and the remaining main-thread stages. It is a repeatable V8/host baseline, not a substitute for browser flame charts. The default workload uses two warmups and 10 samples.

The first baseline showed that a 100k-object, 25.13 MiB scene spent 926.711 ms cloning the parsed object graph across the worker boundary and 375.385 ms diffing one style change. The protocol now sends the JSON string Pyodide already produced, avoiding a worker-side parse followed by object-graph cloning. Semantic field comparisons and a linear append-compatibility check replace repeated `JSON.stringify` equality calls.

On the same 2026-08-19 baseline machine after those changes:

| Objects | Payload | Encoded message clone | Parse result | Validate | Stabilize | Diff one style | Main rerun pipeline |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 0.24 MiB | 0.255 ms | 2.513 ms | 0.262 ms | 0.148 ms | 0.589 ms | 3.139 ms |
| 10,000 | 2.48 MiB | 1.831 ms | 23.392 ms | 3.061 ms | 1.579 ms | 4.646 ms | 35.348 ms |
| 100,000 | 25.13 MiB | 25.975 ms | 220.244 ms | 44.030 ms | 29.724 ms | 79.639 ms | 394.628 ms |

At 100k, encoded transport is about 36x faster than object-graph cloning and semantic diff is about 4.7x faster. Clone plus main-thread processing falls from roughly 1.38 seconds to 0.421 seconds, about a 3.3x improvement. JSON parsing is now the dominant measured stage; getting substantially below this ceiling requires a compact/binary scene transport or worker-produced incremental deltas rather than more tuning of the Rust patch fast path.

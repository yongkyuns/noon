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

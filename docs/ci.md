# Continuous integration policy

Noon's redesign uses CI as a correctness gate, but not as the inner development loop.

## Development cadence

During active architecture work:

1. group several related edits into one coherent batch;
2. stage connector-based changes as unreferenced Git objects when useful, so intermediate files do not trigger Actions;
3. update the implementation branch once per batch;
4. run the fast architecture gate on every draft-PR update;
5. use the full legacy compatibility gate only at milestone boundaries, before review/merge, or when legacy code is touched intentionally.

A batch can contain several implementation steps when they share one architectural seam. A failed fast gate blocks the next batch, but individual formatting or lint repairs should be folded into the next repair batch rather than serialized into separate CI cycles.

## Fast architecture gate

Every draft PR update validates the new engine crates only:

- `cargo fmt --all -- --check`;
- compile `noon-core`, `noon-compile`, and `noon-runtime` with all targets/features;
- strict Clippy with `-D warnings` for those crates;
- tests for those crates.

As new architecture crates are introduced, they are added to this fast gate immediately.

## Full compatibility gate

The original `noon` and `examples` crates predate the redesign and carry substantial lint debt and heavy Nannou/wgpu dependencies. Full-workspace validation therefore runs when:

- the pull request is no longer draft;
- CI is manually dispatched;
- changes land on `master`.

The full gate compiles the complete workspace, runs legacy Clippy with lint levels capped to warnings, and runs all workspace tests except the explicitly documented broken legacy Lyon partial-path test.

## Why this split exists

The new architecture has no Nannou dependency and should iterate at Rust-library speed. Recompiling hundreds of legacy graphics dependencies after every semantic-core edit adds latency without increasing confidence in the code being changed. The split keeps new code strict while preserving a full compatibility check before integration.

Cargo registry and target caches are enabled in both jobs to reduce repeated dependency work.

Browser target checks, structural renderer checks, differential CPU/GPU checks, and visual regression tests will be added to the fast gate as those subsystems enter the workspace.

Timing benchmarks remain separate from required correctness CI because shared GitHub runners are noisy. Required CI should prefer deterministic structural performance invariants such as batching counts, cache behavior, active-track work, and buffer-upload counts.

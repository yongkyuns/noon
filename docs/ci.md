# Continuous integration policy

The realtime/browser redesign is developed under a CI-first rule: architectural implementation does not advance while the branch is red.

For each implementation step:

1. make one narrowly scoped implementation commit;
2. let the draft PR run the repository CI gate;
3. inspect failures before making any unrelated change;
4. repair the same step until green;
5. only then begin the next step.

## Baseline and strictness policy

The original `noon` and `examples` crates predate this CI policy and currently contain substantial Clippy lint debt. They remain subject to formatting, compilation, Clippy execution, and workspace tests, but their existing warnings are not promoted to errors.

Every new architecture crate is held to a stricter standard from its first commit:

- `cargo fmt --all -- --check` must pass;
- the full workspace must compile with all targets/features;
- legacy crates must complete Clippy without compilation errors;
- new architecture crates run Clippy with `-D warnings`;
- all workspace tests/all features must pass.

As additional architecture crates are introduced, they are added to the strict Clippy gate. This prevents legacy lint cleanup from blocking the redesign while ensuring no new lint debt is introduced in the new engine.

Browser target checks, structural renderer checks, differential CPU/GPU checks, and visual regression tests will be added when those subsystems enter the workspace.

Timing benchmarks are kept separate from required correctness CI because shared GitHub runners are noisy. Required CI will prefer deterministic structural performance invariants such as batching counts, cache behavior, active-track work, and buffer-upload counts.

The baseline workflow is installed on `master`, so implementation-branch pull-request commits are validated by a trusted base workflow before the next architectural step begins.

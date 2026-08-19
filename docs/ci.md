# Continuous integration policy

The realtime/browser redesign is developed under a CI-first rule: architectural implementation does not advance while the branch is red.

For each implementation step:

1. make one narrowly scoped implementation commit;
2. let the draft PR run the repository CI gate;
3. inspect failures before making any unrelated change;
4. repair the same step until green;
5. only then begin the next step.

The initial Rust quality gate checks formatting, Clippy with warnings denied, and all workspace tests/all features. Browser target checks, structural renderer checks, differential CPU/GPU checks, and visual regression tests will be added when those subsystems enter the workspace.

Timing benchmarks are kept separate from required correctness CI because shared GitHub runners are noisy. Required CI will prefer deterministic structural performance invariants such as batching counts, cache behavior, active-track work, and buffer-upload counts.

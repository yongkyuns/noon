# CI evidence tooling

Development qualification and CI evidence tooling, not another architecture or
roadmap. The engine contract remains `docs/architecture.md`.

## Local architecture and iteration gate

The #1272 R3/R4 guardrails are owned by #961. The ordinary local command remains
`bash scripts/check.sh fast`; it now runs the common architecture gate before any
compilation. For architecture-only feedback or an explicit comparison base:

```sh
bash scripts/check.sh architecture origin/master
bash scripts/check.sh fast origin/master
# Equivalent architecture-only entrypoint:
bash scripts/check-architecture.sh origin/master
```

This composes the existing layer, core-module, renderer-host, active-perf and
migration/identity ratchets plus the crate-private export check. All `check.sh`
modes run it before their existing work. Prerequisites: Bash, Git, Python 3.10+,
Cargo using `rust-toolchain.toml`, and grep/sed/wc/tr/mktemp. Install the repository
toolchain explicitly before offline use. Dependency inspection uses
`cargo metadata --no-deps --format-version 1 --offline`; it does not compile or
download dependencies. All declared **normal/build/dev** edges obey the existing
layer policy, including aliases, workspace inheritance, optional and inactive
target dependencies. Missing/malformed metadata is an error, not an empty graph.

### Comparison and candidate

The optional final argument is an existing commit/ref, defaulting to
`origin/master`, printed and resolved once. There is no automatic fetch, merge-base
selection or `HEAD`/`HEAD^` fallback. Missing refs and unavailable shallow history
fail: obtain the intended history explicitly and rerun. Supply
`"$(git merge-base origin/master HEAD)"` explicitly for a branch-point comparison,
or `HEAD` deliberately for working-tree-only checks (also valid on a first commit).
The latter does not check earlier commits on the branch.

The candidate is the **current working tree**, not an index-only/pre-commit
snapshot. Staged edits/additions, unstaged edits and nonignored untracked files are
checked at their current contents. A staged version subsequently overwritten by
an unstaged edit is not separately validated. Git-based scans exclude ignored
untracked output, but include explicitly staged ignored files; existing stricter
source-directory scans remain intact. A disposable intent-to-add Git index makes
untracked files visible to existing structural scans. The real index, worktree,
refs and history are untouched, and the temporary index is cleaned on success or
failure. Run on a stable working tree, not concurrent editor writes.

CI supplies the exact PR base or push-before SHA. Manual Architecture Ratchets
runs require an explicit `base` input. Existing regression suites remain, with
real-Cargo and common-entrypoint negative fixtures for staged/untracked violations,
malformed manifests, missing bases, shallow history and index preservation.

### Focused iteration and timing

After the architecture gate, use the relevant existing test path:

```sh
# Shared semantics and authoring.
cargo test -p noon-core -p noon --lib --all-features
# Compiler/runtime.
cargo test -p noon-compile -p noon-runtime --lib --all-features
# Native and browser platform compilation.
cargo check -p noon-native
cargo check -p noon-web --target wasm32-unknown-unknown
# Rust/Python adapter parity after building the browser package and installing
# the browser-test dependencies used by CI.
node scripts/cross-language-parity.mjs
```

These are explicit iteration commands, not change-based task selection. Use
`bash scripts/check.sh fast BASE` for the normal local gate and
`bash scripts/check.sh full BASE` when required before merge. Existing native/WASM
feature, browser, parity, golden, differential, performance and platform workflows
remain qualification requirements; focused passes do not replace them.

The `rust`, `test`, and `full` entrypoints run workspace unit/integration/binary
tests with `--lib --tests --bins`, followed by a separate `--doc` test command.
This avoids linking non-test example programs into test builds; Cargo's `--tests`
selection also includes examples configured with `test = true`. A tiny isolated
Cargo fixture confirms that such an example runs alongside library, binary, and
integration tests. Current workspace metadata has 144 examples and none marked
`test = true` or `doctest = true`; workspace `check` and Clippy still use
`--all-targets` and compile every example. Doc tests remain an explicit separate
step.

Each architecture guard reports wall time. Run the regression suites with:

```sh
bash scripts/layer-dependency-ratchet.test.sh
python3 scripts/test_check_entrypoint.py
bash scripts/check-architecture.test.sh
```

The Python entrypoint suite uses test doubles only for ordering/failure propagation;
the shell suite invokes real guards and real Cargo and reports representative
clean/staged/untracked edit-to-result timings. Record checkout, toolchain, command,
candidate and cache state when comparing development latency. Fixture times measure
guard feedback, not Rust recompilation or a claimed speedup. Cold/warm compiler and
mixed-change measurements remain #1265 work; no checks are omitted to claim faster CI.

### Public facade qualification

The R2 public-surface checks reuse `fixtures/provider-consumer` rather than adding
another harness. Its `public_facade` target tests ordinary construction, authored
versus effective observations, coherent live edits/completion, and typed rejection
of stale publication after explicitly opted-in raw integration. Provider CI runs
the consumer in native cells and compile-checks it in WASM cells. Only the
minimal/native cell additionally runs the facade's compile-fail documentation and
the paired `shared_authoring` Rust example:

```sh
cargo test --manifest-path fixtures/provider-consumer/Cargo.toml --test public_facade
cargo test -p noon --no-default-features --doc
cargo run -p noon --no-default-features --example shared_authoring
```

WASM compilation is not browser execution; existing product/browser and paired
Python tests still qualify the host path. See the consumer README for the public
versus integration boundary and the unchanged historical geometry-cost workload.
These tests do not claim all authoring error producers or Python exception mapping
have been converted; remaining R2 ownership stays with #958/#61.

## Focused public authoring error qualification

For #958 / #1272 R2 handle-validation and membership changes, the external
provider consumer exercises the real public Rust result types without workspace
feature unification or a new test engine:

```sh
cargo test --manifest-path fixtures/provider-consumer/Cargo.toml \
  --no-default-features --test authoring_errors
cargo test --manifest-path fixtures/provider-consumer/Cargo.toml \
  --no-default-features --test authoring_errors \
  --target wasm32-unknown-unknown --no-run
```

The existing provider matrix executes these tests in every native feature cell
and compiles them in every WASM cell. Assertions inspect typed causes and verify
that rejected batches, stale/foreign handles, stale publications and pending
callbacks/segments do not change membership, resources, published frames or
revisions. Recovery still uses the existing completion/publication path. WASM
`--no-run` is compile evidence, not a claim of browser execution; ordinary
browser/native product workflows remain required. Run `scripts/check.sh fast`
and the applicable full gate in addition to these focused commands.

## Completed-attempt timings

Run the collector with Node 22+ and a GitHub token with Actions read access:

```sh
GH_TOKEN=... node .github/ci/run-report.mjs collect owner/repo 123456:1 123457:2
```

The token is optional for public repositories when the API permits unauthenticated
reads. Do not put tokens in command arguments, committed files, or reports. Requests
are read-only and use fixed `api.github.com` endpoints without following redirects.
Every sample requires an explicit attempt. Pagination, job identity, completion,
and timestamps are validated; missing times stay `null`, not fabricated zeros.

`ci-artifacts/run-timing/report.json` contains the job and step accounting;
`summary.md` contains the review table. Collection can also run offline with an
array of `{ "run": <attempt API response>, "jobs": [...] }` snapshots:

```sh
node .github/ci/run-report.mjs offline snapshots.json
```

The **CI measurements** workflow accepts up to 20 comma-separated `run:attempt`
pairs on manual dispatch. Its PR trigger tests this tooling against a fixed,
explicitly historical sample set:

| Sample | Run / attempt | Context |
| --- | --- | --- |
| Main CI before the cache change | 34251123265 / 1 | Master 424cae4, failed |
| Main CI with the cache change | 34258854671 / 1 | #1267 head 41cf096, failed |
| Earlier fast-gate observation | 34251062699 / 1 | #1264 mixed change, passed |
| Fast gate with the cache change | 34258854689 / 1 | #1267 CI-only change, passed |

These are different workloads, not a controlled performance comparison. A green
measurement job means the collector/tests succeeded, **not** that the measured
product runs passed. Failures, canceled jobs, and skipped checks stay visible.
The output never automatically approves a PR or claims a percentage speedup.

### Meaning and limits of the measurements

- **Observed latency:** this attempt's `run_started_at` to its last executed job's
  `completed_at`. This includes initial scheduling but not workflow finalization.
  `updated_at` is not used as a completion timestamp.
- **Runner time:** the sum of each executed job's start-to-completion duration,
  including post-job work. Parallel jobs overlap in wall time but their runner
  time is additive. This is not billed time or a CPU-time measurement.
- **Scheduling delay:** job `created_at` to `started_at`, including dispatch and
  provisioning. Pure hosted-runner queue time cannot be isolated from this API.
  Time before a dependent job is created is reported separately, never added to
  runner time or mislabeled as queue time.
- **Step phases:** transparent name-based buckets. A composite build or `cargo
  test` step can contain setup, compile/link, and test work. Use the dev-WASM
  action's phase/sccache evidence for finer attribution; do not interpret the
  entire bucket as compiler work. Missing step metadata remains unavailable.
- **Source:** API `head_sha` is not a PR's tested synthetic merge SHA. Obtain the
  actual checkout from checkout logs and `web/ci-artifact.json`. The timing report
  deliberately leaves `testedCheckoutSha` unavailable rather than inferring it.
- **Attempts:** a rerun's executed jobs are one sample; prior successful jobs may
  be reused by Actions. Do not sum attempts as though each were a full run, or
  interpret one successful retry as complete qualification of all original jobs.

Representative cold/warm Rust/frontend/mixed trials, end-to-end comparison across
all required workflows, seed eviction/freshness, build-once producer consolidation,
and a measured tool-image decision remain separate #1265 acceptance work.

### Product measurement protocol

`playground-product-gate.yml` builds the baseline, pinned anchor, and candidate release packages
in one producer, preserving the job-local compiler cache between fresh source
builds. Its measurement job downloads the producer's immutable artifact IDs and
verifies each checkout SHA, release feature set, compiler pin, resolved lockfile,
and runtime-file hashes using the shared WASM artifact contract. The candidate is
GitHub's exact tested PR merge commit; the baseline is that merge's actual first
parent. The producer checks its second parent against the requested PR head and
records all three commits plus the event base in each product manifest. An older
event base remains provenance and is never substituted for the actual baseline.
Comparison retries check out the producer's pinned commits and verify the recorded
pair alongside package contents, so a moving PR ref cannot change the comparison.

Before the first product build, the producer resolves the baseline Cargo lock once.
It reuses that lock for checkouts whose tracked Cargo manifests, `.cargo` settings,
and Rust toolchain pin match; changed dependency inputs are resolved independently
and logged. Existing tracked `Cargo.lock` files are preserved. Every lock is checked
with `cargo metadata --locked` over the full dependency graph, so missing entries fail rather
than being accepted by a manifest-only check or silently regenerated. The
renderer-smoke fixture and production candidate share the candidate checkout's
prepared resolution.

The accepted cumulative source remains pinned to `58135c40` in
`product-performance-anchor.json`, independently of the immediate PR parent.
Both source comparisons use the same validated manifest workload list and retain
separate evidence. The source/configuration/package verifier remains shared.

The comparison uses three serial pairs in a fixed B/C, C/B, B/C order. Each
browser run completes a cold pass before measuring a warm pass. The existing
square/circle fixture scores authored seconds 1–4 with a half-second endpoint
hold. The curated camera lesson runs verbatim on both packages and scores only
its following segment, authored seconds 3.7–6.9. Its existing 0.4-second hold
exposes that endpoint before camera restoration and source completion at 9.8
seconds; setup, restoration and holds are excluded from FPS. Three additional
unmodified showcase lessons cover mixed text/geometry (First Scene, 2.6–6.8 s),
raster-image transforms (3.4–6.4 s), and vector-path construction/reveal (Bézier,
1.4–3.4 s). Each window is entirely animated and ends at an existing static hold.
These are representative product cases, not a dense-scene or live-allocation proof.
All five workloads reuse the same downloaded packages and three-pair protocol,
with no per-workload builds: 60 runs and ten comparisons across the two source
cohorts. The comparison job has a 55-minute execution allowance; this does not
change any latency/FPS/visual threshold.
The preview stays visible during measurement. The sampler sends read-only queries
through the existing renderer metrics channel, consumes only its test-owned replies,
and never adds aggregate source-owner queries to the callback lane. Replayable
lessons seek to their fixed endpoint; the camera lesson must keep seeking disabled
and reach its endpoint through normal source execution.
FPS uses the render worker's sampling clock and
presentation counter within one session/clock epoch; reply arrival and polling
times remain diagnostic observations. Duplicate settled endpoint replies do not
extend the window. A late start, missing endpoint, malformed clock, failed run,
changed package/configuration, or reordered cohort fails qualification.

Camera and the three additional workloads enable the existing bounded, optional publication-stage metrics.
The harness deduplicates worker-clock render-call timestamps and requires complete
presentation-counter coverage in the scored window. It reports p50/p95/p99/max
gaps and 60 Hz long-frame diagnostics; missing samples fail qualification. These
are CPU renderer-submission intervals, not GPU completion or physical scanout.
Shared software-GPU gap summaries remain diagnostic; no physical cadence budget
is applied to them. The comparison recomputes camera FPS and gaps from raw samples.

Protocol version 5 records the exact installed authored source path and SHA-256
before the edit-pass comment. Every source arm receives the same candidate-owned
fixture; wrong paths, missing hashes, or different bytes between arms/trials fail
before image decoding. Only the established square/circle duration/hold edit is
applied; the showcase sources are unmodified.

The protocol reports delta-apply, render-call and acknowledgment
CPU wall times over those same completely covered publications. It summarizes
existing draw/upload/cache/object counters from distinct renderer observations as
**sampled last-frame values**, not totals across frames missed between polls.
Renderer rebuild and mode-switch counts are lifetime-counter differences across
the window. No GPU timestamp queries or source-owner work are added. Reports retain
raw observations and the comparison recomputes these costs before image decoding.
Each run hashes the locally derived generated artifact inventory against its
producer manifest and reports uncompressed file bytes, excluding the dependency
lockfile. This is package size, not compressed HTTP transfer size. All three trials
must retain the same package inventory and sizes. These costs are descriptive;
the existing latency/FPS/visual thresholds remain unchanged.

The existing latency/FPS thresholds apply to arithmetic means across all three
runs. Every raw sample, per-run report and fixed-frame image is retained. The
comparison reports ranges and percentiles as dispersion, and checks every pair's
image independently. Dependency-free preflight runs invalid-report/cohort controls;
Product Gate explicitly enables the PNG/seeded-regression controls after its
existing dependency setup. This is software-WebGL qualification, not physical-device
60 FPS evidence. Physical camera pacing and denser/additional workload coverage
remain owned by #1653. All ten comparison invocations run even if an earlier
comparison rejects, retaining the negative result and failing the job afterward.
A failed measurement still fails its cohort; it is never replaced or retried.

Any diagnostic rerun must repeat the entire declared cohort with those exact
packages, without rebuilding Rust or selecting a passing observation. A passing
rerun does not erase the earlier failure. Re-running all jobs builds new packages. Keep
both package artifacts for the 14-day evidence window; after they expire, rerun
the full workflow. Measurement artifacts include the attempt number, so a retry
does not overwrite the original failure. All existing smokes, comparison
thresholds, and the required comparison check name remain in place. A failed
producer also fails that required check.

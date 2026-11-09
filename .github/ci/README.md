# CI evidence tooling

Development qualification and CI evidence tooling, not another architecture or
roadmap. The engine contract remains `docs/architecture.md`.

## Artifact retention

CI artifacts expire within three days. Browser and product packages keep their
shorter one- or two-day limits, and Pages deployment artifacts expire after one
day. Every upload declares `retention-days` explicitly, including shared actions
and failure diagnostics. The CI contract suite checks this policy with
`node --test .github/ci/artifact-retention.test.mjs`.

## Initial package candidates

`Package candidates` qualifies the files that users install. It performs no
registry upload, GitHub Release publication or deployment. Registry names remain
pending in [#2433](https://github.com/yongkyuns/noon/issues/2433); local artifacts
currently use version `0.1.0` and the existing checkout names.

The source job builds an sdist and checks its complete Rust/resource closure,
canonical Python modules, metadata, dependency declarations and notices. Each
wheel job builds **from that sdist** with cibuildwheel 4.3.1, applies its normal
auditwheel/delocate/delvewheel repair, audits the resulting archive and exercises
the installed command from outside the checkout. PRs test CPython 3.11 on Linux
x86_64, the native macOS runner architecture and Windows x64. Explicit dispatch
builds CPython 3.11–3.15 with the normal GIL on those same hosts. The Linux job
also renders a PNG, probes/decodes a rational-FPS MP4 and runs the real export
regressions against the repaired installed wheel. A successful run qualifies only
its recorded interpreter, platform tags and host; it is not physical-device
performance evidence or a free-threaded Python qualification.

The Rust job builds the publishable workspace archives and tests an independent
consumer patched exclusively to **extracted archives**. It covers minimal and
default authoring, native host/export compilation and the WASM/browser graph.
The reviewed dependency lock seeds the consumer, and the report rejects unreviewed
registry versions/checksums or a dependency that escapes back to checkout source.
`noon-python` is a wheel implementation package and is excluded from crates.io.
The consumer runs the selected public API fixtures against archive-origin
libraries. Repository integration tests that read root-level fixtures remain
checkout qualification in the existing workflows; the archive check does not
claim that each packaged crate's repository test suite is independently runnable.
Production resources and normal library builds must remain self-contained.

The Rust candidate also retains a dependency-notice inventory. The collector
verifies cached crate bytes against `Cargo.lock`, copies upstream notices exactly,
and can fill missing files only from the GitHub revision recorded in the verified
crate. Its all-target normal/build dependency graph is explicitly a conservative
superset, and it reports missing files and ancestor-file applicability separately.
Every repaired wheel report lists its native payloads and bundled notices so any
additional shared libraries can be matched to their redistribution terms. These
are review inputs; candidate CI's report-only collection is not a license approval.

```sh
cargo fetch --locked
python scripts/collect-release-licenses.py --output dist/release-licenses --fetch-missing --wheel dist/<wheel>.whl
```

Without `--report-only`, missing Cargo notice evidence returns a failing exit
status after preserving the report. Review the complete evidence set and actual
wheel repair output before publication; finding a license file alone does not
resolve every asset, dependency or native library obligation.

Local equivalents are:

```sh
python scripts/check-rust-packages.py --wasm --output dist/rust
python -m build
python scripts/check-python-package.py dist/<wheel>.whl --sdist dist/<sdist>.tar.gz --render --mp4
```

Use `--allow-dirty` explicitly for a local Rust packaging experiment; final
candidates must identify a clean source commit. The existing optimized-production
browser job additionally runs:

```sh
node scripts/package-browser-release.mjs --version 0.1.0 --output dist/browser
```

Run that command after the production `prepare`, build and `stamp` steps in
`optimized-replay-artifact.mjs`. It rejects a development/replay build, stale
hashes or a dirty/mismatched source. The archive retains the entire deployable
`web/` tree, notices, original runtime build identity, source SHA and checksums.
The browser's existing external CDN dependencies still require network access.
No npm publication is needed to serve its `web/` directory.

Candidate artifacts still expire after **three days** under the policy above.
At the approved release step, attach the exact qualified archives, checksum
files and reports to the matching source tag's GitHub Release and record the
hosted browser build identity there. Those retained release assets, rather than
expiring CI artifacts or the moving Pages URL, are the durable browser version.
Missing matrix cells, dependency redistribution evidence and physical-device
qualification remain explicit release gates in #2433.

### Assemble the approved release

After the registry names, version and supported matrix are settled, update all
package/dependency names together and qualify a new clean source commit. Record
that commit, its tag, each candidate's checksum and the completed matrix in #2433.
The current `0.1.0` filenames are candidates; their existence does not reserve a
registry name or approve a release. A name, source, dependency or build-profile
change requires fresh evidence for the affected artifacts.

Use the `internal_dependencies` entries in the Rust `release-report.json` to
confirm the publication order. With the current graph, dependency-first groups
are:

1. `noon-core`
2. `noon-geometry`, `noon-text`, `noon-typst`
3. `noon-compile`
4. `noon-runtime`
5. `noon`, `noon-render-wgpu`
6. `noon-export`, `noon-native`, and optionally `noon-web`

Publish each group only after its preceding dependencies are available from the
registry. `noon-python` remains excluded. Serving the browser application does
not require publishing `noon-web`; include that crate only if the release also
promises a downstream Rust/WASM library. Recompute this order after renaming or
changing dependency edges.

At the separately approved publication step, run Cargo's registry-backed
`publish --dry-run --locked` for each selected crate after its dependencies are
available. Inspect any archive or resolved-lock difference from the qualified
candidate and rerun the affected consumer checks before upload. After upload,
verify the downloaded archive against the submitted candidate and run a clean
consumer without local patches. The extracted-archive rehearsal cannot prove
that a not-yet-published registry graph resolves successfully.

For Python, upload the exact qualified sdist and repaired wheels; retain their
checksums, native-library inventories and installed-artifact reports. Verify an
installation from the published index outside the checkout, including the
documented command and supported output. For the browser, attach the qualified
static archive and reports to the same source tag, serve its `web/` directory,
and verify the hosted runtime build identity against the retained manifest.
Keep the release record's platform and feature claims limited to the matrix
that actually passed.

## Qualification cadence

The exhaustive pinned Manim raster oracle runs nightly at 06:17 UTC and on
explicit dispatch. Relevant PRs run its policy/manifest/capture preflight; the
full oracle is no longer a PR merge gate. Dispatch it against the exact candidate
for Manim parity, renderer rasterization or camera semantics changes, and triage
nightly failures as regressions. Its sources, thresholds and full stages remain
unchanged.

Every PR still runs the CI contracts and web unit tests in PR Fast Gate. Edits to
the cadence policy, timing reporter, physical-adapter tests or CI README do not
also select the heavyweight browser/Product Gate suites. Gallery topology checks
select browser CI, while web `*.test.mjs` edits use preflight. Runtime, artifact,
product protocol and mixed production changes keep their existing qualification.
Unknown CI helpers still select the heavyweight suites; exclusions are explicit.

Noon integration-test-only edits keep Rust qualification without selecting the
Product Gate's four production builds and measurement cohorts. PR Fast Gate runs
all workspace library tests and the shared CPU family target in one Cargo
invocation; production, toolchain, Cargo configuration and mixed changes retain
Product qualification.

Pages reuses the complete push-diff classifier on its existing runner. Pushes
containing only Cargo integration tests, CI policy tests, the CI README or
unrelated workflows skip compilation and publication. Production, mixed, unknown
and incomplete-history changes keep the optimized build/deploy/verification path.
Manual dispatch always deploys; PR validation is unchanged.

Master CI reads the complete before/after Git diff and skips its compiled/browser
jobs only for documentation, named preflight-only policies or web unit tests.
Unknown inputs, mixed changes, missing history and classification failures retain
full qualification. PR and manual runs remain full when selected. Recovery and
playback workflows keep isolated web unit-test edits in preflight as well.

CUDA demonstration fidelity runs on changes to its scene, oracle, controls or
qualification wiring. Other authoring PRs retain ordinary full CI without the
additional exhaustive CUDA pass. The existing nightly/manual raster oracle runs
all three positive and six negative CUDA cases against generic engine edits,
using its already built WASM package. Missing change evidence or classifier
output keeps the CUDA suite enabled; its original fifteen-minute budget remains.

## Required GPU numerical checks

Linux jobs that set `NOON_REQUIRE_GPU_TESTS=1` must install both `libvulkan1`
and `mesa-vulkan-drivers` before invoking the native renderer tests. The loader
and Mesa software adapter are required even when a later Playwright step installs
browser system packages. Missing adapters must remain a failure of the required
numerical gate; a skipped test does not qualify the production shader.

Software-adapter pixel/coverage checks establish numerical rendering behavior.
They do not qualify physical-device frame pacing or imply a renderer speedup.
Retain the original failure and measure the candidate on its exact source.

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
# Related CPU family integration cases share one ordinary Rust module tree.
cargo test -p noon --test family --all-features
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

The versioned workspace `Cargo.lock` is the shared dependency resolution for local,
CI, Pages and release builds. The browser build passes `--locked` to Cargo, and
release preparation validates the lock rather than regenerating it. Dependency
updates must include the lock change in the reviewed source revision.

For historical baselines without a tracked lock, the product producer resolves
the baseline Cargo lock once. It reuses that lock for checkouts whose tracked
Cargo manifests, `.cargo` settings, and Rust toolchain pin match; changed dependency
inputs are resolved independently and logged. Existing tracked `Cargo.lock` files
are preserved. Every lock is checked
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
cohorts. The adjacent-base and cumulative pinned-anchor cohorts run concurrently
on **different GitHub-hosted runners** to avoid GPU/CPU contention. Each
30-run cohort still runs its unchanged B/C, C/B, B/C pairs serially, in the
prescribed order, and uploads its full evidence under a distinct artifact ID.
The original required **Runtime, visual, latency, and FPS regression** job
downloads these exact same-run evidence IDs, re-verifies the immutable product
packages, and performs all ten unchanged comparisons. It fails closed unless
the producer and both complete measurement jobs succeeded; missing or failed
cohorts are never treated as skipped/passing evidence. The two measurement jobs
retain their 55-minute execution allowance; the aggregation/comparison job
has a 20-minute allowance. No latency/FPS/visual acceptance threshold changes.
Compiler cache remains job-local; release executables are not reused from
untrusted prior PR runs.
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
both package artifacts for the two-day package retention window; measurement and
comparison reports expire after three days. After packages expire, rerun the full
workflow. Measurement artifacts include the attempt number, so a retry
does not overwrite the original failure. All existing smokes, comparison
thresholds, and the required comparison check name remain in place. A failed
producer also fails that required check.

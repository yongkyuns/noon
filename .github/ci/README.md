# CI evidence tooling

Implementation tooling for #1265, not another architecture or roadmap. The engine
contract remains `docs/architecture.md`.

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

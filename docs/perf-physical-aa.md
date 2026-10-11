# Manual physical A/A precision acquisition (#1933)

**Diagnostic-only, not an approval of #1956 or any other PR.** This path is deliberately isolated from the production Playground Product Gate, source execution, graphics, and fixed performance scorers. It never builds alternative production binaries, changes budget thresholds, reruns a failed measurement, or merges code.

## Why this is needed

The completed hosted original same-source A/A controls blocked **12/20** product cohorts on Apple Paravirtual Metal and **27/27** Python host metrics, despite identical code on both sides. The merged [precision auditor](perf-aa-metrology.md) can check original evidence, but cannot make noisy hardware reliable. Before accepting any *new* candidate-versus-baseline regression claim, demonstrate that one physical, low-contention environment passes the **same original strict A/A bounds** with complete fixed samples.

## Admission: manual physical runner only

The [physical A/A workflow](../.github/workflows/perf-physical-aa.yml) has an automatic **protocol-test-only** job for PRs. Its measurement job is dispatched exclusively from default `master` with the exact typed acknowledgement `RUN_PHYSICAL_AA_ONCE`, a first-run attempt, and the labels `[self-hosted, macOS, noon-physical]`. It never runs GPU measurements on a GitHub-hosted PR runner.

Before requesting this manual run, independently verify that the self-hosted runner actually belongs to a stable, powered physical Mac (Intel x64 or Apple Silicon arm64) with an interactive display, adequate free space, pinned Node 22.23.3, pinned Chromium 151.0.7922.34 and no competing workload. The existing readiness probe checks AC, source-identifiable WebGL2, shader/readback integrity and software fallback, but labels the result only `eligible-for-separate-aa-diagnostic`—**it cannot independently certify physical GPU ownership, thermal stability, or scheduling precision**. The suite additionally rejects virtual/Paravirtual devices and rechecks the original observed renderer at each product pair and in host measurement. Absence of the runner means **do not dispatch**.

## Immutable registered material

The committed `benchmarks/perf-physical-aa-study.json` fixes one original source pair:

- The tested candidate source is a two-parent synthetic Git merge; its checkout fetches both parents (depth two), as required by the original source-provenance verifier. A default depth-one checkout is not sufficient.
- Original [strict #1956 run](https://github.com/yongkyuns/noon/actions/runs/38078042785), fixed base `0473399d`, tested candidate merge `e11ed304` (tree identical to #1956 head `0be750b7`).
- Frozen release ZIP artifact **11680476116** (baseline SHA256 `01148252e04015e801ee7ae404239fc534e0ea9080ca1a35590d3766db34428c`) and **11680486084** (candidate SHA256 `88dc597497c1cf39d45bf3d4539e3429c51caac6c897825d7de6d044eac05004`). Source checkout, build ID, manifest, artifact provider-run identity and ZIP digest must all match.
- Frozen published original measurement harness commit `439c4c4d` and its original Manim samples, pair protocol, renderer metrics, PNG comparison, shell/cold/warm/edit timings and shared scorer. The **only** registered harness edit converts the historical *Apple Paravirtual* GPU identity admission to a *real physical macOS hardware WebGL2* identity admission, pinned as a distinct Git blob. The two source builds, sample windows, telemetry, rendering and confidence estimator are not modified.

**Artifacts are subject to GitHub Actions retention (original producer: 14 days).** Missing or expired registered ZIPs block this study. Never substitute a rebuilt release, a different commit, or a newer favorable artifact under the existing study ID.

## Exact acquisition and scored original controls

The controller verifies source/build manifests, authored-source hashes and the physical browser identity **before any scored trial**, then runs all **100 original A/A product pair commands**: five workloads × two separate original release packages × (7 adjacent + 3 frozen cumulative), each pair serving **the very same original build on both sides**, with B/C then C/B alternating order. This yields 200 complete original E2E reports; every source, image, renderer/proof and per-pair log is retained. One malformed/missing trial aborts the acquisition, with no resume or replacement.

A second independently checked-out copy of the *same original baseline release* supplies the original Python host A/A: three workloads × async/portable/JSPI × seven pairs = **63 ordered pairs**, **126 scored reports**, and one warmup on each scored worker. The derived driver leaves `scripts/python-host-perf-protocol.mjs`, `pairedCost`, actor work, callback counts and seven-pair order untouched. The derivation stops before product scoring, intrusive profiling and worker-history diagnostics; the only host execution adjustment is the headed physical WebGL2 launch and pre-score real shader/readback proof.

The workflow archives the original ZIP bytes, independent manifest checks, original plan, physical admission report, every product journal event, all raw product/host results and any partial failure. Finally it invokes the already merged `scripts/perf-aa-precision-audit.mjs`, which **exits nonzero** unless all the original A/A product and host strict statuses pass. Its JSON always contains `qualification:false`, `performanceAcceptance:false`, `mergeApproval:false`; a positive A/A is not an A/B result.

### To run when an eligible physical runner is actually available

Open **GitHub → Actions → Physical A/A original precision acquisition → Run workflow**, select `master`, type exactly `RUN_PHYSICAL_AA_ONCE`, and submit **only one** attempt. No automatic retry, score selection, or repeated dispatch to obtain a favorable result. The source-pinned archive intake checks the GitHub workflow-run registry and fails closed unless exactly **one** manual dispatch exists, and its run ID, branch and attempt match the current run. A second manual dispatch is rejected even if its separate run attempt is numbered one. When it finishes, retain the entire `physical-aa-1933-<run>-1` artifact and independently verify the registered ZIP hashes, environment, journal and scorer integrity. A failed or inconclusive physical A/A means **do not run** a candidate-versus-baseline acceptance test.

## Limits and follow-up

This is a one-shot source-pinned protocol for the **specific original #1956 release pair**, not a general CLI for arbitrary artifacts. The setup cannot supply a physical machine; its manual runner remains a prerequisite. Physical-looking renderer strings are necessary checks, not proof of exclusive bare-metal hardware. Shell timing and OS load can still be unstable. The same-code controls must empirically demonstrate the original 7+3, 1.03/1.05 cost and FPS lower-95% precision with all observations retained.

Only after an independently credible identical-source study passes should a **separately preregistered, exact-new-head candidate-vs-baseline** comparison be attempted under the unchanged production Product Gate. [PR #1956](https://github.com/yongkyuns/noon/pull/1956) stays draft/unmerged until then. The original strict performance acceptance constraints remain unchanged.

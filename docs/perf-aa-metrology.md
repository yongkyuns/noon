# A/A measurement precision screen (#1933)

This is a **diagnostic-only prerequisite** for the unchanged strict Product Gate. It cannot qualify an implementation, replace candidate-versus-baseline evidence, certify a physical GPU, waive inconclusive intervals, or authorize a merge.

## Inputs and source identity

The read-only script `scripts/perf-aa-precision-audit.mjs` accepts **completed, frozen, pre-registered same-build A/A** evidence:

1. The byte-exact original *product* `inputs/registered-plan.json` (SHA-256 matched against the summary).
2. The corresponding `study/summary.json` and **all** `study/acquisition.jsonl` events.
3. The complete original *Python host* `host-cost/comparison.json`.

For a study with the existing pinned workload set, the auditor requires:
- Five product workloads × two independently authenticated packages × (7 adjacent + 3 cumulative) = **100 product pair commands**, all recorded in **200 ordered start/completion journal events**, without replacement.
- Genuine WebGL2 renderer/shader/pixel proofs on all 100 completed events, including the frozen clear RGBA [51,102,153,255] and shader-triangle RGBA [153,51,102,255] (at most 3 byte levels of backend rounding); a pinned Chromium version, no software fallback and one unchanged renderer identity across the full acquisition. The two registered product build sources must also match the plan's source-pair identities. Browser renderer strings and archived readback bytes are **not** independent proof of a physically dedicated GPU.
- All 20 original complete product A/A cohorts, including exact original FPS lower-95% ≥ **0.97**, renderer-cost point ≤ **1.03** / upper-95% ≤ **1.05** (with the original renderer status rule), unchanged latency and behavioral/visual verdicts. Counts and paired log intervals are verified from the already-stored original observations.
- The original nine Python workload×mode cases, each with seven ordered, warmed pairs: **63 host pairs / 126 scored observations / 27 host-cost metrics**, and original host point ≤ **1.03** / upper-95% ≤ **1.05**. The original per-pair scores, worker warmups and source/manifest identities must agree.

The auditor independently checks the original intervals and status labels as a **tamper/integrity check**, not an alternative scorer. It rechecks the paired arrays preserved in the archived summaries, not the full original per-frame files; original ZIP and release-package digests require independent external verification. It never drops observations, estimates a more favorable confidence interval, or edits the original archives. The reported median paired log standard deviations describe *why* a study may fail. For an actual point estimate of 1.000 and seven pairs, the original Student-t host upper 95% ≤1.05 condition needs a paired log SD at most approximately **0.05275**; for three pairs it is approximately **0.01964**. These are explanatory values, not relaxed acceptance thresholds.

## Run an already collected study locally

Extract only these original files from the retained ZIPs (leave the original ZIPs and SHA-256 evidence untouched):

```bash
unzip apple-product.zip \
  inputs/registered-plan.json study/summary.json study/acquisition.jsonl -d aa-product
unzip apple-host.zip \
  second/browser-smoke-artifacts/product-gate/host-cost/comparison.json -d aa-host

node scripts/perf-aa-precision-audit.mjs \
  aa-product/inputs/registered-plan.json \
  aa-product/study/summary.json \
  aa-product/study/acquisition.jsonl \
  aa-host/second/browser-smoke-artifacts/product-gate/host-cost/comparison.json \
  aa-precision-report.json
```

The output JSON is written **exclusively**; an existing report is never overwritten. The CLI exits nonzero on incomplete, malformed, inconsistent, or strict A/A-blocked evidence, and retains `qualification:false`, `performanceAcceptance:false`, `mergeApproval:false` even when all original self-comparisons pass. A positive A/A screen only supports *investigating* a subsequent separately preregistered A/B study.

The original hosted Apple identical-build studies (product [run 37985876890](https://github.com/yongkyuns/noon/actions/runs/37985876890); host [run 37988781224](https://github.com/yongkyuns/noon/actions/runs/37988781224)) reproduce **8/20 product** and **0/27 host** strict passes, so both remain blocked. The product journal also documents an Apple M1 virtual host with three visible CPUs and a one-minute load average greater than three at 52/100 recorded measurement starts. Load average alone does not establish who caused contention or provide an independent causal explanation for any individual sample.

## Physical measurement deployment

The existing manually dispatched [physical GPU readiness workflow](../.github/workflows/perf-physical-readiness.yml) uses the `[self-hosted,macOS,noon-physical]` label and verifies AC power, pinned browser, hardware-like WebGL2 and readback. It is a **prerequisite**, not precision qualification.

A future physical A/A acquisition must first pin the original source SHA, artifact bytes and manifest hashes, worker/browser/GPU identity, study order, precise hardware/thermal/CPU conditions and one-shot acquisition protocol. Then run all original 100 product commands and 63 host pairs as identical-source controls, **before** one genuine candidate-vs-baseline scored run. Do not substitute a normal hosted runner, choose favorable windows, retry failed pairs, or run the A/B study when A/A precision is blocked. Keep all artifact digests and every failed or partial record. #1933 remains open until a suitable measurement environment is demonstrated.

## Validation

```bash
node --check scripts/perf-aa-precision-audit.mjs
node --test scripts/perf-aa-precision-audit.test.mjs
```

The 22 unit tests include full positive synthetic A/A completeness; realistic noisy A/A rejection; tampered intervals or score labels; missing or reordered product/host pairs; changed compiler identities, worker history, false GL clear/triangle readback, pinned producer sources and original thresholds. No PR, production runtime, existing Product Gate source or scorer is modified by this diagnostic tool.

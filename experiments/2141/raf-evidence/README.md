# Frozen 10,000-ray playback: RAF and shader evidence

This is a temporary research evidence package for Noon PR #2175. It contains
the original corrected benchmark, one local cold/warm instrumented capture,
and a separate historical shader experiment. It changes no production source.
The current capture uses the **pre-follow-up-fix CI runtime**, synthetic merge
`68ef5ad7d0684e362fe070619506e59a38bc6168`, from artifact **12083686108**.

## Findings

The remaining delay is directly visible at the renderer's RAF boundary in the
local capture. For each of the 80 short ray flights, the interval from the
initial-state acknowledgement to receipt of the first advancing delta contains
exactly one renderer tick and one engine drive. A partition entirely on the
renderer worker's clock attributes **98.50% cold / 98.85% warm of those gaps to
waiting for the RAF callback**. This is a fraction of those selected gaps, not
of the whole playback or GPU execution time.

| Local Chromium 151 capture | Cold | Fresh scene, same page |
| --- | ---: | ---: |
| Click to observed endpoint | 31.1075 s | 26.5202 s |
| First actual presentation to completion handoff | 25.5168 s | 25.4931 s |
| Authored duration | 25 s | 25 s |
| 80 fast flights: authored / measured wall | 8.2 / 8.5041 s | 8.2 / 8.9320 s |
| Fast flights with interior presentations | 14 / 80 | 2 / 80 |
| Total interior presentations in those flights | 18 | 4 |
| Post-initial gaps, total | 5,168.9 ms | 6,987.8 ms |
| Before RAF request, clipped sum | 11.7 ms | 6.0 ms |
| Waiting for RAF callback, clipped sum | 5,091.3 ms | 6,907.4 ms |
| RAF callback to first advancing delta receipt | 65.9 ms | 74.4 ms |
| Rust drive time across those 80 gaps | 1.7 ms | 1.5 ms |
| All captured Rust drive calls: count / total | 313 / 29.8 ms | 294 / 25.0 ms |

Playback duration is now close to the authored clock in this local environment,
but short-flight presentation quality remains limited: all 48 colored fast
flights in the warm run still have zero interior presentations. Clock catch-up
and a near-25-second total do not establish smooth animation.

The older corrected artifact has a different profile. All 80 fast flights have
zero interior presentations and three endpoint publications in each of its four
warm traces. The two packed candidates spend **43.8838 / 45.2745 seconds** in
those flights, of which **41.6818 / 43.0272 seconds** occur after initial-state
acknowledgement and before the next advancing delta arrives. Between-play
cleanup-acknowledgement to next-initial-delta totals only **374.6 / 473.9 ms**.
Fresh renderer metrics continue arriving during the large gaps. This narrows
the old issue beyond an assumption that Python construction dominates, but the
old traces alone cannot distinguish RAF waiting from engine drive work.

## Separate controlled historical shader evidence

The preserved historical diagnostic compares baseline `6c6c0398c97fbbb0b09deb9a6e7485660ffdd5ed`
with candidate `3567f45cf651a429ac02d0668b83896120afc46f`. The only changed
production Rust/renderer source file is `polygon_coverage.wgsl`; the complete
source-comparison metadata also records the changed documentation, profiler,
test, and gallery files. Each case measures 90 replay frames after 24 warmup
frames, using Chromium 151.0.7922.34 and SwiftShader at 1152 × 648.

| Historical case | Mean frame interval, baseline → candidate | Mean draw calls |
| --- | ---: | ---: |
| Dot | 25.655 → 27.152 ms | 1 → 1 |
| Small fill | 28.838 → 26.217 ms | 2 → 2 |
| Large fill | 108.797 → 35.954 ms | 2 → 2 |
| Public full scene | 299.052 → 99.247 ms | 281.889 → 281.889 |

In the full scene, mean Rust drive time is 0.026 → 0.021 ms and the CPU renderer
call is 3.640 → 3.912 ms. This supports a fragment-work/cadence bottleneck in
that historical software-renderer experiment, even though synchronous renderer
call timing does not show the cost. PR #2057's shader change,
`e55ba8dae61aafd633575033b59c0224abafa458`, already supplies the newer shader in
the current PR. The old research branch has shader blob
`f9ac542b89ea3c714afc0dfd89a0d3df625921e0`; the historical candidate and current
head use `4e5cfa3217fc95080552cc49c09a063e2b165b2f`.

This historical full scene has source SHA256
`242698c9d9e6063dfc920be09af25b1112c615e3528507b920c3b1e1e062eaa7`,
**different from the frozen workload below**. Its fixed baseline-then-candidate
order and unretained isolated baseline dependency-lock comparison limit causal
precision. Full-scene screenshots differ in one channel by one level; this
experiment is not pixel-exact. The local old/current frozen captures are also
**not a controlled A/B**: do not claim the shader alone caused their elapsed-time
difference, or extend these software-renderer results to physical GPUs.

Sources: [historical run and job](https://github.com/yongkyuns/noon/actions/runs/38509400537/job/115674194479),
[source comparison](https://github.com/yongkyuns/noon/compare/6c6c0398c97fbbb0b09deb9a6e7485660ffdd5ed...3567f45cf651a429ac02d0668b83896120afc46f),
and [PR #2057](https://github.com/yongkyuns/noon/pull/2057).

## Reproduce the analysis offline

Requires Python 3.10 or later and its standard library. No browser, npm,
network, compiler, or repository checkout is needed to verify the preserved
data and regenerate the results. Use a new or empty output directory:

```bash
python3 -B tools/reproduce.py --output /tmp/noon-research-replayed
```

The command verifies the package manifest, decompresses the exact raw report,
validates both original artifact ZIPs and the frozen source, recalculates the
analyses, and checks the compact result against `summaries/findings.json`.
It writes complete per-flight and per-phase JSON analyses to the output
directory and emits one compact JSON object to stdout. The current raw report
contains both runs and both workers' full cumulative profiles; separate run
JSON copies would duplicate the same records. Endpoint screenshots are included.

## Reproduce the instrumented capture

The large compiled runtime and browser archives are not embedded. Supply the
already downloaded runtime artifact ZIP and an exact Chromium 151.0.7922.34
executable; their original archive digests are in `provenance.json`. The host
source is independently pinned to `798ae68bd29a4e22571f5f850261cf8739abe090`.
The reconstruction command uses that value directly, verifies all four runtime
manifest hashes, and verifies the two exact probe insertions. It does not
check out, edit, rebuild, or fetch repository source.

```bash
python3 -B tools/prepare-site.py \
  --checkout /path/to/noon \
  --runtime-zip /path/to/noon-pr2175-browser-runtime.zip \
  --site /tmp/noon-profile-reconstructed-site

npm install --ignore-scripts

NOON_BASELINE_SITE=/tmp/noon-profile-reconstructed-site \
NOON_BASELINE_SCENE="$PWD/workload/scene.py" \
NOON_BASELINE_OUTPUT=/tmp/noon-profile-recaptured \
NOON_BASELINE_REVISION=68ef5ad7d0684e362fe070619506e59a38bc6168 \
NOON_BASELINE_RUNS=2 \
NOON_PROFILE_CHROME=/path/to/chromium151/chrome \
node harness/frozen-scene-profile.mjs

python3 -B tools/analyze-instrumented.py \
  /tmp/noon-profile-recaptured/report.json \
  --scene workload/scene.py \
  --output /tmp/noon-profile-recaptured/analysis.json
```

The capture harness uses ordinary editor/Run execution, a 1280 × 800 viewport,
DPR 1, a 620 × 348 canvas in the preserved capture, WebGL2, forced SwiftShader,
50 ms observation polling, and the pinned Playwright 1.62.1 dependency. Browser
capture can require network access for the web app's external dependencies.
Timing will vary by host; the offline replay is the deterministic verification.

## Clock, session, and instrumentation rules

- Frozen source SHA256 is
  `32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e`.
  Flight boundaries are reconstructed from a hash-pinned AST allowlist of pure
  geometry helpers and constants; Noon scene classes are never executed by the
  analysis. The first four flights are slow, followed by 32 pink and 48 colored
  fast flights. Their authored boundaries are 0.8, 5.6, 8.8, 9.4, and 14.4 seconds.
- Current stage rows are filtered by `completion.fromSession`. Warm profiles
  are cumulative: prior event prefixes must match exactly and are removed;
  cumulative count/sum totals are differenced. Cumulative maxima are never
  subtracted. Other-session stage tails are excluded.
- Actual start and completion use renderer `execution_presented` posts for
  the relevant sessions at sequence 0. The first stage row can already have
  positive authored time, and a sequence-1 completion snapshot can arrive after
  endpoint detection. The first wrapped `liveSegmentWake` call is an observed
  clock boundary, not the unrecorded exact Rust anchor argument.
- Cross-worker events use the small time-origin offset plus worker-relative
  timestamps. The three RAF partition terms all use the same renderer clock;
  the request interval is clipped at the gap start. Each partition must sum
  to its measured gap within 0.000001 ms.
- The probe records all drive/drain/handoff/wake calls. Other API events are
  retained only when they take at least 1 ms, while cumulative totals cover
  every call. Both current profiles report zero dropped events.
- Phase render/apply sums, API durations, RAF waits, and whole-scene elapsed
  are overlapping measurements. Only the explicit validated partitions are
  additive. Renderer return and acknowledgement are not GPU completion or
  physical scanout. Click elapsed minus presentation span includes startup
  and observer timing; it is not pure Python CPU time.

The next useful experiment is a same-host, same-workload shader comparison
with the same RAF probe, followed by GPU/compositor tracing if the callback wait
remains large. These findings do not justify adding another framebuffer cache
or bypassing the established scheduler without evidence from that comparison.

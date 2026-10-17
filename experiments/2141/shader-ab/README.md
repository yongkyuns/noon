# Frozen-scene shader control: same-host ABBA

This package isolates the polygon coverage shader on the exact packed-rendering tree `495580e2197022701a9190bae4f20047193f5be6`. Restoring the historical shader increases the 80 fast ray flights from about **8.4 s to 19.3–19.8 s** on this host. Engine drive calls inside their post-initial gaps total only **1.0–2.6 ms** per playback. Almost all of the combined time in matched gaps is spent waiting for the renderer worker's next `requestAnimationFrame` callback. The controlled source change therefore establishes a shader-dependent browser scheduling delay in this SwiftShader configuration; it does not establish a Python execution bottleneck.

The newer shader was already introduced through PR #2057. This experiment diagnoses that existing improvement; it does not propose another product change. Both arms retain the same packed renderer, scheduler, Python source, and host code. This is separate from the packing pixel/sample oracles and the earlier historical measurements.

## Results and sample size

Order: **new, old, old, new**. Each block launches a fresh Chromium process and runs the frozen scene twice through the ordinary editor/Run path. There are **two browser contexts and four playbacks per arm**. First and repeat plays remain separate. The raw `cold` label means fresh-browser first run; OS, download, tool, and shader caches were not reset or proven cold.

| Block | Shader | Within-context run | Click → observed endpoint (s) | First presentation → handoff (s) | Fast-flight wall (s) | Fast flights with interior publication / 80 | RAF wait / matched gaps |
|---|---|---|---:|---:|---:|---:|---:|
| 0 | New | First | 30.5623 | 25.4660 | 8.3787 | 23 | 98.5225% (80 matched) |
| 0 | New | Repeat | 26.4071 | 25.4640 | 8.3592 | 24 | 98.6521% (80) |
| 1 | Old | First | 35.9789 | 31.2229 | 19.8312 | 1 | 99.3981% (80) |
| 1 | Old | Repeat | 32.6755 | 31.7778 | 19.3156 | 0 | 99.2612% (80) |
| 2 | Old | First | 35.8302 | 30.8267 | 19.4436 | 1 | 99.3461% (80) |
| 2 | Old | Repeat | 32.5122 | 31.6162 | 19.4543 | 1 | 99.4113% (80) |
| 3 | New | First | 30.3304 | 25.5035 | 8.4106 | 25 | 98.5478% (79) |
| 3 | New | Repeat | 26.3837 | 25.4115 | 8.3651 | 25 | 98.3466% (80) |

Descriptive medians have **n = 2 per arm and condition**, with no confidence claim:

| Metric | First: old → new | Same-page repeat: old → new |
|---|---:|---:|
| Click → observed endpoint | 35.9046 → 30.4464 s (15.20% lower) | 32.5939 → 26.3954 s (19.02% lower) |
| First presentation → handoff | 31.0248 → 25.4848 s (17.86% lower) | 31.6970 → 25.4378 s (19.75% lower) |
| Fast-flight wall | 19.6374 → 8.3947 s (57.25% lower) | 19.3850 → 8.3622 s (56.86% lower) |
| Clipped RAF wait in matched fast gaps | 17.5299 → 4.4665 s | 17.4935 → 4.3741 s |
| Engine drive inside presentation span | 11.10 → 34.45 ms | 8.35 → 27.35 ms |
| Engine drive inside fast gaps | 2.55 → 2.15 ms | 2.25 → 1.30 ms |

The new arm produces more presentations and therefore can accumulate more drive work while finishing sooner. The new arm still has **55–57 of 80 fast flights without an interior publication**, so these results do not imply smooth presentation of every short animation.

The authored scene lasts 25 s; the fast flights together account for 8.2 authored seconds. Subsequent phases are shorter in the late old-shader runs: for example, the 16–23 s dense phase takes roughly 4.1–5.2 s in old and 7.3–8.0 s in new. This is consistent with later playback catching up to its wall clock. Do not turn the roughly 11 s fast-phase difference into an additive whole-playback saving. The complete per-flight and disjoint phase timing is regenerated from the raw reports by the verifier.

## What the intervals mean

`click_to_endpoint_ms` is measured by the Node harness around click and endpoint polling. It includes startup, scene preparation, playback, and observer/host latency. It is not an interpreter CPU measurement. The presentation span uses renderer-worker publication posts for the first current-session scene frame and completion handoff, excluding an optional later snapshot. Publication acknowledgement is CPU-side evidence, not GPU completion or physical scanout.

The frozen source is hash-checked, and its exact 84 ray-flight boundaries are recovered with the restricted AST/geometry decoder in `tools/analyze-prior.py`. The first four flights are slow; the remaining 80 form this comparison. Interior and boundary publication counts are measured; the scripts impose no blanket zero-interior or triple-publication assumption.

Warm stage samples are selected by the completion's original session. Worker profiles are cumulative: the analyzer checks and removes the exact preceding event prefix and differences API counts/sums. It never differences cumulative maxima. Worker event times are converted using their recorded `performance.timeOrigin` onto the renderer's clock.

For each fast flight, the post-initial gap begins at its initial presentation acknowledgement post and ends at the renderer's receipt of the first advancing publication. For a gap with exactly one relevant tick and drive, the preceding RAF request/fire is matched. The interval is partitioned on the renderer clock into time before the request, request-to-callback waiting clipped to the gap, and time after callback through advancing-publication receipt. These three intervals sum to the matched gap.

**RAF fraction = sum of clipped RAF waiting / sum of matched post-initial gaps.** Its denominator is neither total scene time nor total fast-flight wall time. API totals, RAF durations, render/apply durations, and playback elapsed overlap; they must not be added or subtracted as if they were independent stages. A synchronous renderer return does not measure asynchronous shader/GPU completion.

The sole unmatched interval is block 3, first run, bounce 75: a **0.4000000004 ms** gap from renderer time `17309.900000000373` to `17310.300000000745`. It contains a tick at `17310.10000000149`. The engine drive converts to `17310.300048828125`, about **0.000048827 ms** beyond the strict upper bound, consistent with clock-origin floating-point/quantization effects. The strict guard consequently finds no drive start inside the gap. It remains excluded from **both** RAF numerator and matched-gap denominator, without imputation; that row is 79/80 matched. All other rows match 80/80.

## Source, build, and environment controls

`source-control.json` and `shader-only.patch` retain the comparison. The only production source difference is `crates/noon-render-wgpu/src/polygon_coverage.wgsl`:

- New source: `495580e2197022701a9190bae4f20047193f5be6`, shader blob `4e5cfa3217fc95080552cc49c09a063e2b165b2f`.
- Old control: `19dd538c61cd5f13a89e31aaa68ea383db958986`, shader blob `f9ac542b89ea3c714afc0dfd89a0d3df625921e0`, restored from `6af9661a2bf551170a0835aac9eeb32feb32dff5`.
- Reviewable source comparison: <https://github.com/yongkyuns/noon/compare/495580e2197022701a9190bae4f20047193f5be6...19dd538c61cd5f13a89e31aaa68ea383db958986>.
- Frozen scene SHA256: `32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e`.
- Exact retained Cargo.lock SHA256: `fdc4872fdf8ebc9ed9c7f634f95ee0dd98a2720b44bca3bfcf72c8ad0b5369f4`.
- Rust 1.98.1, wasm-pack 0.15.0, Binaryen 132, wasm-bindgen 0.2.129; release profile, no extra features, optimizer `-O`.
- New selected WASM: 50,121,763 bytes, SHA256 `ab3485e20f1c2c54dcbab9da26da3b66b736355c3134c34014584a84129c8822`.
- Old selected WASM: 50,133,059 bytes, SHA256 `49f6167ab264ee404e8b609989199bccbda7734b21b1a7707f2a1651729a5238`.
- Chrome for Testing 151.0.7922.34, Playwright 1.62.1, Node 24.19.0, forced ANGLE SwiftShader WebGL2, viewport 1280×800, DPR 1, backing buffer 620×348 (CSS 621.671875×349.6875). CDP GPU identity is collected after both timed runs.
- Every settled endpoint has 154 draws, 10,430 instances and 10,432 objects. All runtime, workload, backend, profile-completeness, error, and endpoint gates passed. Both timed sites contain 763 files and differ only in selected WASM and runtime identity. The probes and host hashes match across arms.

Both arms were rebuilt from clean `web/pkg` outputs with the retained lockfile and transparent `audit-bin/wasm-opt` wrapper. Each raw audit records exactly one `-O` invocation of the same Binaryen executable, no pre-existing temporary, real exit 0, and complete output section boundaries. The selected output equals the audited output and no optimizer temporary remains. Build package checks additionally execute `WebAssembly.compile` for semantic validation; all eight browser runs admit the expected runtime.

An earlier new-arm build left a truncated, unreferenced optimizer intermediate during disk exhaustion. Its metadata and original build log are retained. **The clean audited rebuild reproduces the original selected `ab3485…` byte-for-byte**, positively resolving optimization equivalence; the intermediate is excluded from timed sites. Merely checking the selected module's structural validity would not have established optimization. The wasm-pack 0.15.0 source enumerates every `.wasm` and renames the temporary over the selected input only after a successful child process, hence clean directories and a single audited invocation matter.

Local compiler/test and other browser work had finished before captures. `experiment.json` records compiler-process checks and load averages at block boundaries. ABBA reduces simple order effects but does not eliminate all host variation. No physical-GPU or broad user-device performance claim follows from this software-rendered experiment.

## Endpoint image check

`summaries/terminal-pixel-comparison.json` compares all eight saved full-page PNGs, plus a clearly labeled coarse preview-panel image region. Within that region, all four new endpoints are exact to one another; all four old endpoints are exact to one another. The arms differ in **two green-channel values**, each by **1/255**: at `(1135,393)`, new = 173 and old = 172; at `(1118,399)`, new = 123 and old = 124. Cross-shader endpoint PNGs therefore are **not pixel-exact**.

Three new full-page images are byte-identical. The last new repeat differs only in the Uploaded bytes UI label (`0 B` → `13.8 MiB`), outside the preview: 918 changed channels, maximum 235, bounds `[1020,689,1070,697]`. All old full-page images are byte-identical. This check rules out a visibly different terminal scene in these captures at the stated precision; it is not intermediate-frame, backing-buffer or MSAA parity. Independent/native shader and packing oracles remain separate evidence.

## Historical scope

The prior evidence is published at research commit `6b48919f52a1697d2f69abc7d1e579c81ee5f080`, `experiments/2141/raf-evidence`. The historical corrected frozen benchmark and these captures use the same frozen source hash, Chrome version, DPR, and 620×348 backing dimensions. Resolution therefore does not explain its old 43–45 s fast-flight wall versus the controlled old arm's 19.3–19.8 s. Other historical host/build differences remain unresolved. **Do not claim this shader explains the entire earlier 68.6 s playback or numerically combine the experiments.** The earlier controlled public-scene shader evidence also uses a different workload and is retained separately.

## Offline reproduction

No network, browser, build, or existing `/tmp` setup is needed to replay the retained raw evidence. Python 3 standard library is sufficient for timing, manifest, identity, and optimizer-record validation. The optional image arithmetic uses Pillow 12.3.0 (`requirements-image.txt`). Run from any location, with an empty output folder outside the package:

```bash
python3 -B /path/to/evidence-package/tools/verify-offline.py --output /tmp/noon-shader-replay
```

For the full timing and numeric image replay, install the declared image dependency in your Python environment and use:

```bash
python3 -B /path/to/evidence-package/tools/verify-offline.py --output /tmp/noon-shader-replay-with-pixels --pixels
```

The verifier checks every manifest entry, original decompressed report and lockfile hash, both optimizer records, captured runtime/environment/endpoint identities, session and clock handling, then regenerates all four per-block analyses and compares the eight-row summary exactly with `summaries/comparison.json`. `--pixels` also reproduces the image summary exactly. It writes only the chosen replay directory. The retained audits verify the captured build chain; without compiled modules in this compact package they do not independently rebuild those modules.

## Fresh build and capture reproduction

Fresh timing is a separate experiment and requires substantial build time, the pinned toolchain/browser, a quiet host, and network access for normal web runtime dependencies. This package deliberately omits large compiled WASM and browser binaries. Keep original evidence immutable: copy this package to a separate experiment directory for recapture.

1. Have a Noon clone containing both exact source commits above. Create two new detached worktrees. Do not change your primary checkout. Verify `git diff --name-only 495580e2197022701a9190bae4f20047193f5be6 19dd538c61cd5f13a89e31aaa68ea383db958986` lists only the shader; the retained patch is the complete source diff.
2. Decompress `build/Cargo.lock.gz` into **both** worktree roots. Use Rust 1.98.1 with `wasm32-unknown-unknown`, wasm-pack 0.15.0, wasm-bindgen 0.2.129, Binaryen 132 and Node 24.19.0. `optimizer-proof.json` includes the optimizer and wasm-pack executable hashes used here. The unchanged audit wrapper requires the real Binaryen executable at `/tmp/binaryen-version_132/bin/wasm-opt`; its SHA must match the proof. Keep that wrapper first on PATH. Start with clean/absent generated `web/pkg` in each new worktree and run the builds sequentially.
3. The exact audited build command below used preinstalled tools under the displayed temporary paths. Supply those prerequisites explicitly; they are not embedded in this package. `NOON_AB_PACKAGE` names your copied package; `NOON_AB_ARM` is `old` or `new`. Run in the corresponding detached worktree. Keep separate logs and append-free audit files for each arm:

```bash
env PATH="$NOON_AB_PACKAGE/audit-bin:/tmp/noon-wasm-tools/bin:/tmp/binaryen-version_132/bin:/tmp/noon-cargo/bin:$PATH" \
  RUSTUP_HOME=/tmp/noon-rustup CARGO_HOME=/tmp/noon-cargo \
  CARGO_TARGET_DIR=/tmp/noon-wasm-target CARGO_BUILD_JOBS=2 \
  NOON_SKIP_WEB_PREFLIGHT=1 NOON_WASM_PROFILE=release NOON_WASM_SKIP_OPT=0 \
  NOON_WASM_OPT_AUDIT_LOG="$NOON_AB_PACKAGE/optimizer-$NOON_AB_ARM.jsonl" \
  bash scripts/build-web-demo.sh > "$NOON_AB_PACKAGE/rebuild-$NOON_AB_ARM.log" 2>&1
```

4. Check successful package validation, unchanged lock hashes, one audit invocation per arm, no `output_validation_error`, selected/output hash equality, and absence of `*.wasm-opt.wasm`. The source/runtime identities must be the exact published commits, not an unrecorded local revision. Stage the complete sites and reproduce the exact two host edits with the included script:

```bash
python3 -B "$NOON_AB_PACKAGE/tools/stage-sites.py" \
  --new-web /path/to/new-worktree/web --old-web /path/to/old-worktree/web \
  --output "$NOON_AB_PACKAGE/sites"
```

5. The staging script checks exact runtime and instrumented-host hashes, equal 763-file path sets, and the two permitted inter-arm differences. It copies the probe, prepends its import in both host modules, and inserts the exact engine receive marker at a unique source boundary. It fails on unexpected source, runtime, extra optimizer temporary, or host differences. The retained metadata remains the admission reference.
6. Install the pinned Playwright dependency in the copied package (`npm install --ignore-scripts`). Supply Chrome for Testing 151.0.7922.34; the Linux archive SHA256 is `ae8736ac28bc69278551500f219fc749575648263c43ec5990749eff43b9fcf8`. Once all other compiler/test/browser work is idle, capture into an empty new directory:

```bash
python3 -B "$NOON_AB_PACKAGE/run-abba.py" \
  --chrome /path/to/chrome-linux64/chrome --output /tmp/noon-shader-new-capture
python3 -B "$NOON_AB_PACKAGE/tools/analyze-comparison.py" \
  /tmp/noon-shader-new-capture --output /tmp/noon-shader-new-comparison.json
```

`run-abba.py` performs identity gates before and after every block and invokes the per-block analyzer. Browser launch flags and normal-page endpoint admission are in the retained harness. `harness-change.json` documents the sole change to the earlier capture harness: CDP GPU information collected after both timed runs. The probe itself is unchanged.

## Files

`raw/` holds four compressed reports including both cumulative worker profiles and all stage samples, plus eight original PNGs. `build/` holds the exact lockfile, original and audited build logs, optimizer records, and capture log. `source-control.json`, `site-identities.json`, `optimizer-proof.json`, `experiment.json`, and `provenance.json` retain the controls. `tools/`, `harness/`, and `audit-bin/` retain executable methods. `summaries/` holds derived numeric results; `manifest.json` hashes every other retained file. An initial sandbox listener denial before any browser launch produced no timing and is explicitly excluded in provenance.

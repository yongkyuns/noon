# ManimCE raster differential oracle

Noon's semantic Manim differential suite (`scripts/manim-differential.py`) answers whether supported authoring semantics agree with pinned ManimCE. The raster differential lane adds the missing end-to-end question: **does the same scene occupy and color the same pixels at the same animation time?**

The reference is pinned to **Manim Community v0.21.0 using the Cairo renderer**. The canonical source lives under `parity/manim-v0.21/` and imports real `manim`. The Noon side changes only that import to `noon` and appends a scene-selection wrapper required by the browser authoring worker. Do not tune geometry, colors, runtimes, waits, or positions for Noon in these fixtures.

## Canonical corpus

The corpus started with the source-equivalent Manim quickstart subset and now also contains focused reveal, style, palette, and alpha-compositing probes. The fixture list and its expected durations live in `parity/manim-v0.21/manifest.json`.

These are intentionally separate from the demo's pedagogical Noon adaptations. A demo being `ready` means it executes; it does not mean its output is Manim-parity-qualified.

## Deterministic profile

`parity/manim-v0.21/manifest.json` fixes:

- ManimCE version: 0.21.0;
- renderer: Cairo;
- output: 960×540;
- frame rate: 30 fps;
- sampled reference-frame fractions;
- blocking raster tolerances, with fixture-specific overrides where required.

The harness renders real lossless Manim PNG frames, authors the same scene through Noon's Pyodide compatibility frontend, and seeks Noon to the matching Manim frame times. It captures both WebGPU and WebGL2. Each fixture gets an independent browser canvas player so capture state cannot leak between scenes.

The default 2D presentation contract is also part of parity: the camera is centered at the origin with an **8.0-world-unit frame height** (16:9 width at the default aspect) and a **black background**. All browser canvas player entry points must use that same default contract; the differential harness must not compensate for a runtime-specific camera or clear color.

## Artifacts and diagnostics

`node scripts/manim-raster-differential.mjs` writes `manim-raster-artifacts/` containing:

- `reference/<fixture>/...png` — real Manim frames;
- `webgpu/<fixture>/...png` and `webgl/<fixture>/...png` — Noon output;
- `diff-webgpu/` and `diff-webgl/` — amplified absolute pixel differences;
- `report.json` — timing, foreground bounds/centroid, background color, foreground color summary, pixel error metrics, and categorized mismatch hints.

`node scripts/manim-raster-semantic-state.mjs` then attaches renderer-independent state at those **same frame indices and times**:

- `semantic/manim-all-frames.json` — Manim state captured from the actual `Scene.play_internal()`/Cairo frame progression with pixel output disabled;
- `semantic/<fixture>/frame-XXXX.json` — the paired Manim and Noon state plus normalized deltas for one sampled raster frame;
- `semantic/index.json` — a compact machine-readable index of every sampled semantic comparison;
- each backend sample in `report.json` gets a `semantic` summary and a path to its full paired state.

The common semantic observations include top-level scene membership/order, object center and axis-aligned bounds, fill/stroke RGBA, normalized scene-unit stroke width, and object count. Noon additionally records its runtime `appearance`, `reveal`, and `morph` scalars. Noon snapshots are produced by a fresh deterministic `ScenePlayer.seek()` over the exact authored `SceneDocument`; they therefore exercise the same renderer-independent `FrameState` model consumed by browser rendering without requiring a GPU.

Semantic deltas are **diagnostic**, not a second visual baseline. Reveal/morph representations intentionally differ internally between the engines, so the existing focused `manim-differential.py` suite remains the blocking structural oracle while the raster ratchet remains the blocking final-presentation oracle. The paired frame-state files exist to make a pixel failure attributable to membership/timing, geometry/bounds, style, or rasterization without re-running either engine interactively.

The diagnostic raster categories are timing, background/color pipeline, camera/layout/geometry, and raster/style/animation-state. Focused semantic tests remain the preferred oracle for identifying exact causes; raster output catches what structural observations cannot.

After measurement and semantic capture, `node scripts/manim-raster-enforce.mjs` evaluates every fixture/backend/sample against the explicit manifest ratchet and writes `ratchet-report.json`. The dedicated CI workflow uploads all reports, semantic state, and reference/Noon/diff images together.

## Blocking ratchet policy

The raster workflow is now blocking on explicit metrics rather than on the old coarse category threshold. The manifest defines defaults and may tighten or relax individual fixture metrics with:

- `max_duration_delta_seconds`;
- `max_background_channel_delta_sum`;
- `max_bounds_delta_px`;
- `max_differing_ratio`;
- `max_mean_absolute_channel_error`.

A fixture override is expected to document **measured deterministic headroom**, not normalize a bad Noon image into a golden. `max_bounds_delta_px: null` is the only supported metric exemption and must be narrowly scoped to a known unconverged geometry gap. Other timing, background, and raster metrics remain blocking even when bounds are exempt.

`scripts/manim-raster-policy.test.mjs` deliberately injects timing, background, bounds, foreground-presence, and pixel-error regressions to prove the policy fails in the expected category. This test runs before the expensive Manim/browser setup in CI.

The legacy `NOON_MANIM_RASTER_ENFORCE=1` mode still makes any coarse diagnostic category fail during measurement and is useful for local investigation. CI does **not** rely on it; CI always runs the explicit per-metric ratchet after `report.json` is produced.

## Local run

The render command requires the same dependencies as the dedicated CI workflow: ManimCE 0.21.0 with Cairo/Pango/FFmpeg, the built Noon browser WASM package, Playwright Chromium, and `pngjs`.

```sh
node scripts/manim-raster-policy.test.mjs
node scripts/manim-raster-differential.mjs
node scripts/manim-raster-semantic-state.mjs
node scripts/manim-raster-enforce.mjs
node scripts/manim-seek-playback-raster.mjs
```

Useful environment variables:

```sh
NOON_MANIM_RASTER_BACKENDS=webgpu,webgl
NOON_MANIM_RASTER_ARTIFACTS=manim-raster-artifacts
NOON_MANIM_RASTER_ENFORCE=1  # optional coarse diagnostic mode
```

## Expansion rule

When Noon claims another Manim example as output-compatible, add a source-equivalent fixture to this corpus and give it a blocking tolerance supported by a current ManimCE-v0.21.0 differential run. Keep smaller semantic probes in `scripts/manim-differential.py`. Unsupported APIs and known unconverged metrics remain explicit gaps rather than approximate fixtures.
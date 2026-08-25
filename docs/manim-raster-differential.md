# ManimCE raster differential oracle

Noon's semantic Manim differential suite (`scripts/manim-differential.py`) answers whether supported authoring semantics agree with pinned ManimCE. The raster differential lane adds the missing end-to-end question: **does the same scene occupy and color the same pixels at the same animation time?**

The reference is pinned to **Manim Community v0.21.0 using the Cairo renderer**. The canonical source lives under `parity/manim-v0.21/` and imports real `manim`. The Noon side changes only that import to `noon` and appends a scene-selection wrapper required by the browser authoring worker. Do not tune geometry, colors, runtimes, waits, or positions for Noon in these fixtures.

## Initial corpus

The first tranche is the source-equivalent Manim quickstart subset:

- `CreateCircle`
- `SquareToCircle`
- `SquareAndCircle`
- `AnimatedSquareToCircle`

These are intentionally separate from the demo's pedagogical Noon adaptations. A demo being `ready` means it executes; it does not mean its output is Manim-parity-qualified.

## Deterministic profile

`parity/manim-v0.21/manifest.json` fixes:

- ManimCE version: 0.21.0
- renderer: Cairo
- output: 960×540
- frame rate: 30 fps
- sample fractions: begin, 25%, 50%, 75%, final rendered frame

The harness renders a real Manim MP4, extracts exact encoded frame indices with FFmpeg, authors the same scene through Noon's Pyodide compatibility frontend, and seeks Noon to the matching frame time. It captures both WebGPU and WebGL2.

The default 2D presentation contract is also part of parity: the camera is centered at the origin with an **8.0-world-unit frame height** (16:9 width at the default aspect) and a **black background**. All browser canvas player entry points must use that same default contract; the differential harness must not compensate for a runtime-specific camera or clear color.

## Artifacts and diagnostics

`node scripts/manim-raster-differential.mjs` writes `manim-raster-artifacts/` containing:

- `reference/<fixture>/...png` — real Manim frames;
- `webgpu/<fixture>/...png` and `webgl/<fixture>/...png` — Noon output;
- `diff-webgpu/` and `diff-webgl/` — amplified absolute pixel differences;
- `report.json` — timing, foreground bounds/centroid, background color, foreground color summary, pixel error metrics, and categorized mismatch hints.

The current categories are deliberately coarse: timing, background/color pipeline, camera/layout/geometry, and raster/style/animation-state. Focused semantic tests remain the preferred oracle for identifying exact causes; raster output catches what structural observations cannot.

## Ratchet policy

The first landing runs in **report mode** because current Noon output is known to differ substantially from Manim. Tooling failures, missing scenes, authoring failures, renderer failures, wrong output dimensions, and missing artifacts are blocking. Known visual differences are recorded rather than normalized into baselines.

Set `NOON_MANIM_RASTER_ENFORCE=1` to make any categorized mismatch fail. As #177–#184 converge, replace the coarse all-or-nothing enforcement with explicit per-metric tolerances and enable them incrementally. Do not create a Noon-generated golden and call it Manim compatibility.

## Local run

The command requires the same dependencies as the dedicated CI workflow: ManimCE 0.21.0 with Cairo/Pango/FFmpeg, the built Noon browser WASM package, Playwright Chromium, and `pngjs`.

```sh
node scripts/manim-raster-differential.mjs
```

Useful environment variables:

```sh
NOON_MANIM_RASTER_BACKENDS=webgpu,webgl
NOON_MANIM_RASTER_ARTIFACTS=manim-raster-artifacts
NOON_MANIM_RASTER_ENFORCE=1
```

## Expansion rule

When Noon claims another Manim example as output-compatible, add a source-equivalent fixture to this corpus and keep its smaller semantic probes in `scripts/manim-differential.py`. Unsupported APIs remain explicit gaps rather than approximate fixtures.

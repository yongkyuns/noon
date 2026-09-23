# Curated Noon animation showcase

## Status and scope

This is the first implementation slice, not a declaration that the entire gallery has been curated or visually approved. The ten new lessons are explicitly preview-only at `?catalog=showcase` (or an explicit `?example=showcase-...` deep link). The default catalog and all 49 existing reference entries remain unchanged until the new sources, real posters, playback and backend evidence are reviewed. No placeholder poster is acceptable for publication.

The original `example-gallery.js` reference implementation moves unchanged to `example-gallery-reference.js`. A small facade routes explicit showcase requests to their own manifest. It does not introduce a new scene model, authoring worker, execution session, renderer, or playback implementation. Legacy example IDs and explicit manifest callers retain their old path.

## Editorial contract

Every public lesson needs one primary learning outcome, a short storyboard, meaningful motion, a representative captured state, and a readable end state. A successful runtime test does not establish editorial quality. Exact Manim source equivalence and public readiness are separate questions.

All ten new scenes have animated introductions and deliberately timed sequences. Use smooth motion and fade/create/write transitions rather than abrupt presentation changes. Linear timing remains intentional where uniform timing or discrete subset thresholds are the concept being taught. Do not distort those semantics solely to apply easing everywhere.

Public scene source contains no regression assertions. The original matching-shapes identity and coordinate checks remain in their original fixtures; they are not stripped, disabled, or replaced by weaker tests. The new lesson teaches matching without an unexplained source re-add. The old static and micro API examples remain reference material, not newly approved showcase lessons.

## Dynamic scene is a featured composition

**A field in motion** remains near the front of the preview, not hidden as a test utility. Its default 600 shapes and 24 animated text labels participate in six paced sections:

1. An animated field assembly.
2. Full-field circle/square morphs.
3. Three waves of simultaneous position, rotation, color, and independent text changes.
4. A smooth full-field spiral recomposition.
5. A cross-faded exchange of one third of the geometry.
6. A second full-field morph resolving into a color-organized grid.

Headings fade out and in between sections. The end state is designed to remain legible. ROWS and COLS are explicit load controls in the source, not claims that an arbitrary count is a hardware limit; increasing them also requires reviewing spatial framing. The default layout is the target for qualification.

The existing playground's measured object/draw/upload/time counters are made visible for this scene. There is no second frame sampler, fake FPS display, hard-coded performance score, or isolated GPU timing claim. The thumbnail qualification harness drives deterministic samples; its durations are NOT real-time performance measurements. Device/backend-specific peak-load profiling remains a separate qualification step before advertising numeric performance limits.

## First ten learning homes

| Lesson | Primary purpose | Deliberately not included |
|---|---|---|
| Your first scene | Construction, layout, motion | Every primitive/API variant |
| A field in motion | Dense multi-sequence composition | A hardware-maximum claim |
| Together, sequential, staggered | Controlled timing comparison | Internal Add/Wait scheduler probes |
| Animate part of a group | Indexed slice selection | Identity assertions in source |
| Match by shape | Geometry matching despite changed order | Duplicate-key and unmatched edge cases |
| Accumulate or step through | Ordered visibility comparison | Smoothing away discrete semantics |
| Words and equations | Text, emphasis, real mathematics | Full LaTeX parity |
| Functions and samples | Function/sample overlay and legend | Pretending samples are measured data |
| Pixels in the scene | Recognizable array-backed raster | 2x2 alpha/resource-reuse fixture |
| Select a shape | Animated introduction plus host selection | Undelivered Indicate-on-click or wheel zoom |

The pointer example's Python code authors the scene only. Its manifest declares `pointer-fill-selection`, and the existing playground configures that host behavior after authoring completes. The host source is `web/main.js` / `web/authoring-execution-client.js`; no hidden callback is claimed to live in Python. The selected-example summary discloses this setup in the showcase UI. The poster must be captured after an actual click, and a subsequent background click must restore the base pixels.

## Preview generation and review

Build the existing web package, install the repository-pinned Playwright/pngjs dependencies, then run:

```sh
node --test web/showcase-gallery.test.mjs
python3 -m unittest discover -s web/python -p test_showcase_contract.py
node scripts/showcase-capture.mjs
```

The capture script uses the existing `manim-raster-host.html` / `SemanticPreviewSession` path for the exact Noon sources; despite the historical host filename, it does not render with Manim. The pointer poster uses the actual playground interaction path. Outputs include source hashes, served build identity, requested and genuinely published times, actual backend, beat PNGs, local poster PNGs and a contact sheet. Quiet-hold timestamps must not be relabeled as newly published frames. Failed sources or missing/empty frames fail qualification. Generated posters must be inspected at card size as well as full size.

Generated images are artifacts until reviewed. Before default promotion, retain the approved PNGs under `web/thumbnails/showcase/`, verify every image loads on the deployed path, and review full animation playback, introduction, transitions, endpoint, replay/reset, pointer click/clear, desktop/mobile framing, and supported backends. A generated contact sheet alone is not proof that the complete animation is good.

## Remaining coverage and migration

This first slice is intentionally NOT exhaustive. Finish and qualify these learning homes before describing the curated catalog as comprehensive:

- Appearance/disappearance comparisons, including grow, spin and reverse reveal.
- Transform versus replacement/copy and pivot/easing distinctions.
- Explicit curved/multi-contour vector paths and SVG import/morphing.
- Reactive relationships and dt-driven updater lifecycle.
- NumberLine transforms, NumberPlane, implicit contours, synchronized/gapped series.
- Vector fields, camera movement with stable world references, and foreground composition.
- Redesign the trigonometry tutorial around a persistent diagram; correct the three-check/four-item inconsistency.
- Complete host-interaction setup presentation and separately qualify any Indicate-on-click or wheel-zoom extension.

Consolidate the public learning experience, not canonical fixtures. The 49-entry audit's source-preservation rule still applies. Its former suggestion to move the dynamic scene out of the showcase is superseded: keep the polished dynamic composition featured, while retaining the unchanged five-second stress workload for regression and profiling comparability.

Do not switch the default gallery merely because these source files compile or CI artifacts exist. Review the resulting animations, fix shortcomings, and then make the publication change explicitly.

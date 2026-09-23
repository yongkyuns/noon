# Curated Noon animation showcase

## Status and scope

This is the first implementation slice, not a declaration that the entire gallery has been curated or visually approved. The fourteen authored lessons are explicitly preview-only at `?catalog=showcase` (or an explicit `?example=showcase-...` deep link). The default catalog and all 49 existing reference entries remain unchanged until the new sources, real posters, playback and backend evidence are reviewed. No placeholder poster is acceptable for publication.

The original `example-gallery.js` reference implementation moves unchanged to `example-gallery-reference.js`. A small facade routes explicit showcase requests to their own manifest. It does not introduce a new scene model, authoring worker, execution session, renderer, or playback implementation. Legacy example IDs and explicit manifest callers retain their old path.

## Editorial contract

Every public lesson needs one primary learning outcome, a short storyboard, meaningful motion, a representative captured state, and a readable end state. A successful runtime test does not establish editorial quality. Exact Manim source equivalence and public readiness are separate questions.

All showcase scenes have animated introductions and deliberately timed sequences. Use smooth motion and fade/create/write transitions rather than abrupt presentation changes. Linear timing remains intentional where uniform timing or discrete subset thresholds are the concept being taught. Do not distort those semantics solely to apply easing everywhere.

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

## Learning homes

| Lesson | Primary purpose | Deliberately not included |
|---|---|---|
| Your first scene | Construction, layout, motion | Every primitive/API variant |
| A field in motion | Dense multi-sequence composition | A hardware-maximum claim |
| Entrances and exits | Four appearance styles at a shared duration | Timing arrangements already taught separately |
| Together, sequential, staggered | Controlled timing comparison | Internal Add/Wait scheduler probes |
| Animate part of a group | Indexed slice selection | Identity assertions in source |
| Bend a vector curve | Cubic anchors, control handles and a smooth path morph | Callback-time path editing and SVG import |
| Transform an object or its copy | Explicit copy isolation and subsequent Transform input animation | Unsupported ReplacementTransform and TransformFromCopy |
| Match by shape | Geometry matching despite changed order | Duplicate-key and unmatched edge cases |
| Accumulate or step through | Ordered visibility comparison | Smoothing away discrete semantics |
| Words and equations | Text, emphasis, real mathematics | Full LaTeX parity |
| Functions and samples | Function/sample overlay and legend | Pretending samples are measured data |
| Relationships that follow motion | ValueTracker-driven dots and their attached connector | dt-driven integrators and input callbacks |
| Pixels in the scene | Recognizable array-backed raster | 2x2 alpha/resource-reuse fixture |
| Select a shape | Animated introduction plus host selection | Undelivered Indicate-on-click or wheel zoom |

The pointer example's Python code authors the scene only. Its manifest declares `pointer-fill-selection`, and the existing playground configures that host behavior after authoring completes. The host source is `web/main.js` / `web/authoring-execution-client.js`; no hidden callback is claimed to live in Python. The selected-example summary discloses this setup in the showcase UI. The poster must be captured after an actual click, and a subsequent background click must restore the base pixels.

## Preview generation and review

Build the existing web package, install the repository-pinned Playwright/pngjs dependencies, then run:

```sh
node --test web/showcase-gallery.test.mjs scripts/showcase-*.test.mjs
python3 -m unittest discover -s web/python -p 'test_showcase*.py'
node scripts/showcase-capture.mjs
node scripts/showcase-live-review.mjs
```

The capture script uses the existing `manim-raster-host.html` / `SemanticPreviewSession` path for the exact Noon sources; despite the historical host filename, it does not render with Manim. The pointer poster uses the actual playground interaction path. Outputs include source hashes, served build identity, requested and genuinely published times, actual backend, beat PNGs, local poster PNGs and a contact sheet. Quiet-hold timestamps must not be relabeled as newly published frames. Failed sources or missing/empty frames fail qualification. Generated posters must be inspected at card size as well as full size.

The additional live-review runner records a WebM of each exact source through the ordinary gallery Run path, without external time sampling, private scene mutation, or a separate renderer. It requires successful source completion and replay, pauses at the authored endpoint, then checks that restart/seek reproduces the same pixels. Videos and endpoint PNGs are retained under each backend's `live/` directory, including failure diagnostics. Video recording and software rendering perturb wall time; neither that time nor a smoothly sampled video is a peak-performance measurement.

The four feature additions have external syntax-only storyboard checks: explicit play/wait durations must sum to the declared duration and still intervals must correspond to real authored waits. These reject ambiguous timing rather than guessing it. They do not validate runtime rendering or prove perceptual smoothness. The updater lesson also pairs each registered callback with its exact removal.

Generated images are artifacts until reviewed. Before default promotion, retain the approved PNGs under `web/thumbnails/showcase/`, verify every image loads on the deployed path, and review full animation playback, introduction, transitions, endpoint, replay/reset, pointer click/clear, desktop/mobile framing, and supported backends. A generated contact sheet alone is not proof that the complete animation is good.

## Runtime findings that still block publication

Run `35903486282` at head `3343197` produced matching results on WebGPU and WebGL: twelve of fourteen deterministic scene captures passed. Ordinary-playback recordings also exposed failures that isolated frames cannot detect. Treat the following as separate qualification concerns, not interchangeable success metrics.

- The canonical composition path rejects `ReplacementTransform` and `TransformFromCopy`; exported class names were not sufficient evidence of support. The revised transform lesson deliberately teaches public `copy()` plus ordinary `Transform`, and says so in its title, source and metadata. It does not emulate or claim the rejected animation APIs.
- The reactive lesson must register **and scene-bind** all three callback targets before its first play begins. The revised source does that and includes the targets in the first FadeIn, preserving an animated introduction. Late first-time callback enrollment remains outside this lesson's demonstrated scope.
- Group slicing, text/math and coordinate plotting complete their source animations but return `UnsupportedDomain` for retained replay. `crates/noon-runtime/src/replay.rs` explicitly excludes domains including family-animation plans and reactive property bindings. The exact invalidation path for each scene still needs qualification; do not remove those demonstrations' features to manufacture a replay pass. Unavailable replay remains a failing live-review result.
- A live seek uses the actual authored endpoint, which may be a few floating-point bits above the decimal storyboard duration. The live endpoint check compares those representations within roundoff, while keeping the requested and published values unchanged. Deterministic sampling keeps its existing strict bounds.
- Parent run `35899027235` had a WebGL exact-clear failure even though the child run passed pointer selection on both backends. Do not dismiss this as fixed by a later pass. Capture now retains the base, selected and last clear-attempt images before assertions, including on failure, so this discrepancy can be diagnosed without relaxing pixel equality.

Live review retains an unseeked first-pass PNG and state before attempting replay. This preserves evidence for a completed animation when a later replay check fails; it is not a substitute for that check. Post-repair backend results, visual review and publication approval are still required.

## Remaining coverage and migration

This first slice is intentionally NOT exhaustive. Finish and qualify these learning homes before describing the curated catalog as comprehensive:

- Finish rendered review of the new entrances/exits, transform-ownership, cubic-path and reactive-relationship lessons.
- Qualify real replacement/copy animation APIs before adding them; explicit copy-and-Transform is not a substitute claim.
- Pivot/easing distinctions, multi-contour vector paths, and SVG import/morphing.
- dt-driven updater lifecycle, beyond the authored ValueTracker relationship lesson.
- NumberLine transforms, NumberPlane, implicit contours, synchronized/gapped series.
- Vector fields, camera movement with stable world references, and foreground composition.
- Redesign the trigonometry tutorial around a persistent diagram; correct the three-check/four-item inconsistency.
- Complete host-interaction setup presentation and separately qualify any Indicate-on-click or wheel-zoom extension.

Consolidate the public learning experience, not canonical fixtures. The 49-entry audit's source-preservation rule still applies. Its former suggestion to move the dynamic scene out of the showcase is superseded: keep the polished dynamic composition featured, while retaining the unchanged five-second stress workload for regression and profiling comparability.

Do not switch the default gallery merely because these source files compile or CI artifacts exist. Review the resulting animations, fix shortcomings, and then make the publication change explicitly.

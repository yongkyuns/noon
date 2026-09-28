# Curated Noon animation showcase

## Status and scope

The default catalog contains fourteen authored lessons. `?catalog=reference` opens the reference catalog; existing reference example deep links and explicit manifest callers retain that catalog. This is a focused introduction, not exhaustive feature coverage. Publication requires the runtime, replay-capability, visual and responsive checks below; no placeholder poster is acceptable.

The original `example-gallery.js` reference implementation moves unchanged to `example-gallery-reference.js`. A small facade routes the default and explicit showcase requests to their own manifest. It does not introduce a new scene model, authoring worker, execution session, renderer, or playback implementation. Legacy example IDs and explicit manifest callers retain their old path.

## Editorial contract

Every public lesson needs one primary learning outcome, a short storyboard, meaningful motion, a representative captured state, and a readable end state. A successful runtime test does not establish editorial quality. Exact Manim source equivalence and public readiness are separate questions.

All showcase scenes have animated introductions and deliberately timed sequences. Use smooth motion and fade/create/write transitions rather than abrupt presentation changes. Linear timing remains intentional where uniform timing or discrete subset thresholds are the concept being taught. Do not distort those semantics solely to apply easing everywhere.

Public scene source contains no regression assertions. The original matching-shapes identity and coordinate checks remain in their original fixtures; they are not stripped, disabled, or replaced by weaker tests. The new lesson teaches matching without an unexplained source re-add. The old static and micro API examples remain reference material, not newly approved showcase lessons.

## Dynamic scene is a featured composition

**A field in motion** remains near the front of the showcase, not hidden as a test utility. Its default 600 shapes and 24 animated text labels participate in six paced sections:

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
| Select a shape | Animated introduction plus source-declared Rust-owned click Indicate | Legacy session-selection overlay behavior or wheel zoom |

The reference `noon-pointer-selection` example remains the legacy session-selection overlay: its manifest declares `pointer-fill-selection`, and the existing playground configures it after authoring completes. The curated `showcase-pointer-selection` lesson instead declares `on_click(..., Indicate(...))` in Python, lowers that typed binding through Rust, and has no manifest interaction policy or host callback. Its poster is captured after a real click, then after the Indicate animation restores the baseline automatically; a background click must leave the image unchanged.

## Preview generation and review

Build the existing web package, install the repository-pinned Playwright/pngjs dependencies, then run:

```sh
node --test web/showcase-gallery.test.mjs scripts/showcase-*.test.mjs
python3 -m unittest discover -s web/python -p 'test_showcase*.py'
node scripts/showcase-capture.mjs
node scripts/showcase-live-review.mjs
```

The capture script uses the existing `manim-raster-host.html` / `SemanticPreviewSession` path for the exact Noon sources; despite the historical host filename, it does not render with Manim. The pointer poster uses the actual playground interaction path. Outputs include source hashes, served build identity, requested and genuinely published times, actual backend, beat PNGs, local poster PNGs and a contact sheet. Quiet-hold timestamps must not be relabeled as newly published frames. Failed sources or missing/empty frames fail qualification. Generated posters must be inspected at card size as well as full size.

The additional live-review runner records a WebM of each exact source through the ordinary gallery Run path, without external time sampling, private scene mutation, or a separate renderer. Every lesson must complete ordinary execution. Deterministic lessons must also admit retained replay, pause at the authored endpoint, and reproduce the same pixels after restart/seek and at intermediate forward/backward samples. The arbitrary Python callback lesson explicitly declares non-replayable host callbacks: it must report the expected `UnsupportedDomain`, disable replay controls, and reproduce its endpoint through a fresh Run. This is reported as rerun qualification, never as a retained-replay pass. Videos and endpoint PNGs are retained under each backend's `live/` directory, including failure diagnostics. Video recording and software rendering perturb wall time; neither that time nor a smoothly sampled video is a peak-performance measurement.

The four feature additions have external syntax-only storyboard checks: explicit play/wait durations must sum to the declared duration and still intervals must correspond to real authored waits. These reject ambiguous timing rather than guessing it. They do not validate runtime rendering or prove perceptual smoothness. The updater lesson also pairs each registered callback with its exact removal.

Generated images are artifacts until reviewed. For publication, retain the reviewed PNGs under `web/thumbnails/showcase/`, verify every image loads on the deployed path, and review full animation playback, introduction, transitions, endpoint, replay/reset, pointer click/clear, desktop/mobile framing, and supported backends. A generated contact sheet alone is not proof that the complete animation is good.

## Runtime qualification boundaries

The original review evidence exposed independent forward-rendering and replay failures. Keep failed captures and recordings with their source/build identity; a later pass does not erase the earlier failure.

- The transform lesson teaches public `copy()` plus ordinary `Transform`. It does not emulate or claim `ReplacementTransform` or `TransformFromCopy`.
- The reactive lesson registers and scene-binds all callback targets before its first play, and removes each callback explicitly. Arbitrary host callbacks remain outside retained replay admission. Their declared capability is surfaced to the reader and checked independently of deterministic replay; unavailable replay in any other lesson remains a failure.
- Family animation revisions and authored scalar timelines use the shared retained execution history. Their replay repairs must preserve current-master graph dependencies and numeric-text support; gallery examples do not introduce a second execution model.
- Subpixel glyph packing must reflect the current frame, including empty/nonempty glyph transitions. The correctness repair preserves unrelated geometry, but its whole-text packing fallback remains C4 locality debt. Gallery qualification does not close that architecture work or establish a device performance budget.
- A live seek targets the actual authored endpoint, which may differ from the decimal storyboard duration by floating-point roundoff. Requested and published values remain recorded unchanged; deterministic sampling retains strict bounds.
- Pointer qualification retains base, indicated and restoration-attempt images before assertions, including on failure. The legacy selection overlay still clears through its session policy. The showcase action must come from an actual host click, restore automatically, settle its wake state, and ignore a background click.

Live review retains an unseeked first-pass image and state before any replay attempt. Endpoint equality, intermediate replay equality, ordinary execution, visual readability, and measured performance remain separate claims.

## Remaining coverage and migration

The fourteen lessons are intentionally not exhaustive. Current master already has paired Rust/Python examples for matrix transforms and display, tables, graphs, moving cameras, implicit and synchronized/gapped plotting, and foreground membership. Reuse those sources and qualifications when developing further editorial lessons; do not duplicate their semantic implementations or describe them as missing Phase B support. Additional authored learning homes include:

- Qualify real replacement/copy animation APIs before adding them; explicit copy-and-Transform is not a substitute claim.
- Pivot/easing distinctions, multi-contour vector paths, and SVG import/morphing.
- dt-driven updater lifecycle, beyond the authored ValueTracker relationship lesson.
- Matrix transforms versus matrix display, tables and graph topology.
- NumberLine transforms, NumberPlane, implicit contours, synchronized/gapped series.
- Vector fields, camera movement with stable world references, and foreground composition.
- Redesign the trigonometry tutorial around a persistent diagram; correct the three-check/four-item inconsistency.
- Keep legacy session-selection presentation distinct from source-declared actions, and separately qualify future wheel-zoom extensions.

Consolidate the public learning experience, not canonical fixtures. The 49-entry audit's source-preservation rule still applies. Its former suggestion to move the dynamic scene out of the showcase is superseded: keep the polished dynamic composition featured, while retaining the unchanged five-second stress workload for regression and profiling comparability.

The default-catalog change must be qualified with actual animation and responsive-layout evidence before merging. A compiling source or generated contact sheet alone is insufficient.

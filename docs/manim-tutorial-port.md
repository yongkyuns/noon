# ManimCE tutorial and example port

Noon's learning path follows the concepts in Manim Community v0.21.0 while adapting the workflow to an interactive browser authoring tool.

The executable source of truth is `web/python/examples/manim_tutorial_manifest.json`. Entries are either:

- `ready`: executable Noon scenes gated by unit/authoring tests;
- `blocked`: part of the intended common-2D learning path but waiting on a named feature issue;
- `deferred`: deliberately outside the current common-2D milestone.

## First runnable path

The initial tranche covers:

1. creating a circle with `Create`;
2. transforming a square into a circle;
3. positioning objects with `VGroup`, `arrange`, and `to_edge`;
4. animating mutating methods with `.animate`;
5. the lifecycle difference between `Transform` and `ReplacementTransform`;
6. `AnimationGroup` and `LaggedStart` composition;
7. `ValueTracker` using Noon's native reactive binding path.

Each example is intentionally short enough for the browser demo loop and is executable through the same `Scene` document path as other playground examples.

## Browser-specific adaptation

Manim tutorials that focus on CLI flags, filesystem movie output, or FFmpeg invocation should not be copied literally. Noon equivalents should teach editor execution, live scene replacement, timeline playback, profiling, and future browser export controls.

## Provenance and licensing

Manim Community is MIT-licensed. The first tutorial tranche uses original Noon code that follows upstream learning goals rather than copying substantial upstream source or prose. The manifest records the upstream reference location and `reuse` mode for every ready example.

If future ports substantially copy an upstream example, retain the required Manim MIT copyright/license notice with that redistributed material. Do not copy protected third-party artwork or assets merely because an upstream example uses them.

## Expansion rule

Feature parity PRs should unlock or add at least one representative manifest entry. Text/math, axes/plots, graph networks, moving camera, and 3D are already represented as blocked/deferred entries so missing tutorial coverage remains visible while those implementations are pending.

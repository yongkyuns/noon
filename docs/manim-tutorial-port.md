# ManimCE tutorial and example gallery

Noon's public learning/demo surface is a Manim Community v0.21.0 compatibility surface. Runnable examples shown to users must therefore be source-equivalent Manim scenes, not Noon-native lookalikes or implementation demonstrations.

The executable source of truth is `web/python/examples/manim_tutorial_manifest.json`.

## Public example policy

A manifest entry may be `ready` only when:

- its output-affecting scene code is source-equivalent to the referenced ManimCE v0.21.0 example;
- `reuse` is `source-equivalent-manim-v0.21`;
- it identifies the canonical parity fixture used by #176/#185;
- it has an explicit parity state of `candidate` or `parity-qualified`;
- its source executes through the browser authoring path;
- it has a static gallery thumbnail/poster asset.

`candidate` means the source-equivalent scene is runnable and is being converged against the canonical Manim oracle. `parity-qualified` means the semantic, pixel, timing, lifecycle, intermediate-frame, seek/playback, and supported-backend gates defined by #176/#185 pass.

Noon-native renderer examples, performance/stress scenes, patch templates, and earlier pedagogical adaptations may remain as internal tests/fixtures where useful, but they are not part of the public example gallery and must not be presented as Manim examples.

## Gallery UX

The demo reads the manifest rather than maintaining an independent example list in `web/main.js`.

The public flow is:

1. browse source-equivalent Manim scenes as thumbnail cards;
2. search/filter by title, capability/category, and parity status;
3. select a stable example ID (also addressable through `?example=<id>`);
4. edit the Python source and view the single live renderer side by side;
5. reset the editable buffer to the canonical source when needed.

Thumbnails are static/lazy assets. The page keeps one live GPU canvas regardless of the number of gallery cards.

## Current runnable set

The initial source-equivalent set covers ManimCE v0.21 quickstart/composition behavior including:

- `CreateCircle`;
- `SquareToCircle`;
- `SquareAndCircle` positioning;
- `AnimatedSquareToCircle`;
- `DifferentRotations`;
- `Add` / `Wait` / `Succession` / `LaggedStartMap` composition.

Additional examples should be added as their compatibility lanes become executable and enter the #176/#185 corpus.

## Browser-specific adaptation boundary

The browser workflow itself may differ from Manim's CLI/file-output workflow: users run/edit scenes in the interactive editor and view them in the live canvas. That workflow difference must not change output-affecting scene semantics in a public example.

## Provenance and licensing

Manim Community is MIT-licensed. Track the upstream version/source for every public example and preserve required Manim copyright/license notices when substantial upstream code is redistributed. Do not copy protected third-party artwork or assets merely because an upstream example uses them.

## Expansion rule

Feature-parity PRs should add or unlock at least one source-equivalent manifest entry when practical. Text/math, axes/plots, graph networks, moving camera, and 3D remain represented as blocked/deferred entries so missing learning coverage stays visible while their implementation work is pending.

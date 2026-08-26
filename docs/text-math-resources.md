# Text and math resource architecture

Noon treats text and mathematical typesetting as retained immutable resources rather than renderer-only bitmaps or Python-owned point arrays.

## Backend split

The semantic core is backend-neutral:

```text
Text / MarkupText  -> native shaping --------------------+
                                                        |
Typst / MathTypst  -> Rust Typst compiler --------------+-> TextResource
                                                        |      |- GlyphRun
Tex / MathTex      -> LaTeX compatibility backend ------+      `- TextVectorItem
                                                               |
                                                               +-> glyph atlas (steady state)
                                                               `-> geometry arena (vector items / lazy outlines)
```

`TextSourceKind` records the authoring language independently from `TextLayoutBackendKind`. This is deliberate: `MathTex` retains LaTeX syntax and layout semantics and must never be silently translated to Typst. Typst is a first-class backend for `Typst` / `MathTypst` and Noon-native math authoring.

## Retained representation

`TextResource` contains:

- shaped `GlyphRun`s with stable source-cluster identity;
- optional intrinsic run colors, while `None` inherits the owning mobject style;
- `TextVectorItem`s for non-glyph backend output such as fraction rules, radical decorations, and arbitrary Typst shapes;
- logical `TextPart`s spanning glyph clusters and vector items for indexing, coloring, matching, and hot reload;
- layout bounds/baseline and a backend/version/artifact fingerprint.

Large vector content is referenced through `GeometryResourceHandle`, reusing the immutable/versioned resource lifetime model from the geometry arena.

## Lazy outlines

Normal steady-state text should use shaped runs and a glyph atlas. Glyph outlines are extracted only when path-level behavior requires them (`Write`, `Create`, morphing, matching) and are cached as geometry resources. This avoids allocating one persistent vector path per glyph for ordinary rendering.

## Typst backend

`noon-typst` is the first concrete math-layout backend. It pins Typst 0.15.1, compiles in-process in Rust, uses Typst's bundled fonts, does not scan system fonts or invoke an external executable, and initially exports a deterministic shrink-wrapped SVG artifact.

The SVG bridge is intentionally a bootstrap boundary rather than the final renderer representation. The next integration step is to walk Typst's finished frame directly (`FrameItem::Text`, `FrameItem::Shape`, nested groups) and normalize it into `GlyphRun` / `TextVectorItem`. That preserves source/glyph identity and avoids an SVG round-trip while leaving the core resource contract unchanged.

## LaTeX parity

`Tex` / `MathTex` remain a separate compatibility path because Typst syntax, metrics, font selection, and line/math layout are not a drop-in replacement for Manim's LaTeX behavior. The LaTeX backend should normalize into the same `TextResource` contract, so rendering, caching, transforms, and matching do not need a second engine.

## Browser policy

The Typst backend is designed to be deterministic and self-contained: no network package resolution, no filesystem resolver, and no system font discovery in the default path. Browser/WASM enablement should preserve those constraints. Optional external packages/assets, if added later, need explicit resource-loading and cache policy rather than implicit host access.

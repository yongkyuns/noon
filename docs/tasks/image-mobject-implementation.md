# ImageMobject implementation slice

Owner: #79 (Phase B2 SVG and image resource support)

This is an implementation checklist, not an architecture document. `docs/architecture.md` remains authoritative.

## Goal

Deliver a bounded, user-facing raster image tranche equivalent to the common ManimCE v0.21 `ImageMobject` path while preserving Noon's single semantic/runtime/renderer authorities.

## Contract

- `SemanticStore` owns image-object identity and references immutable `RasterImageResourceHandle`s.
- Canonical retained content is tightly packed RGBA8 in the existing `RasterImageResourceArena`; repeated identical decoded content deduplicates there.
- Decode occurs only during author/resource preparation. No PNG/JPEG/WebP parsing is allowed on the frame path.
- `noon-compile` lowers image content as ordinary typed compiled content and resolves immutable resource lookup without copying pixels into per-frame state.
- `noon-render-wgpu` owns disposable texture/sampler residency derived from immutable image resources. Transform/opacity-only changes reuse texture residency.
- Native Rust and direct Rust/WASM remain typed in-process paths. Python is a thin adapter and must not own image geometry, scene state, texture state, or animation behavior.
- Construction/publication must be atomic: a failed semantic/compiler/runtime publication cannot retain an unpublished image resource.
- Locality: adding/replacing one image is proportional to that image plus affected semantic/compiler/renderer slots; clean frames and transform-only updates do not decode or re-upload unchanged pixels.

## First supported surface

1. Rust construction from normalized RGBA8 (`width`, `height`, bytes) with Manim-compatible intrinsic aspect ratio and default image height/placement semantics.
2. Python `ImageMobject` from decoded image files and supported array-like pixel input, normalized to RGBA8 before crossing the shared authoring operation.
3. PNG and JPEG file decode in the author/preparation layer. WebP may be promoted only if the same deterministic path and qualification are available; otherwise fail explicitly.
4. Normal `Mobject` transform/layout operations, scene membership, copy, opacity, `FadeIn`/`FadeOut`, and ordinary affine `Transform` where the existing content contract can preserve image identity/resource reuse.
5. WebGPU and WebGL2 retained rendering with alpha compositing and deterministic sampling policy.

## Required implementation order

- [ ] Integrate `RasterImageResourceArena` into `SemanticStore`, including clone re-namespacing and resource lookup.
- [ ] Add semantic image content referencing a resource handle plus image-specific sampling metadata only where it is visual semantic state.
- [ ] Add rollback-safe image-resource admission/publication transaction and focused failure tests.
- [ ] Add Rust `ImageMobject`/scene authoring API from RGBA8 and paired native example.
- [ ] Lower image content through `noon-compile` without per-frame pixel copies.
- [ ] Add retained wgpu texture residency/cache, incremental install/update/removal, and WebGPU/WebGL2 draw encoding.
- [ ] Prove transform/opacity updates reuse texture residency and unchanged images cause no pixel upload.
- [ ] Add WASM authoring handle accepting normalized pixels without JSON/base64 transport inside the engine.
- [ ] Add Python `ImageMobject` facade, file/array normalization, exports, and deterministic unsupported-input errors.
- [ ] Add equivalent Rust/Python examples exercising the same shared semantics.
- [ ] Add source-equivalent ManimCE v0.21 fixture and semantic/raster qualification on WebGPU and WebGL2.
- [ ] Add direct-seek/forward lifecycle checks for supported animations and exact completion.
- [ ] Update compatibility ledger only for behavior actually qualified.
- [ ] Remove this checklist after the implementation is landed and #79 records final evidence.

## Explicit non-goals for the first tranche

- URL/blob/network fetching or a general asset manager.
- Animated GIF/video textures.
- Mutable per-frame pixel buffers or streaming textures.
- SVG `<image>` nodes.
- Renderer-owned semantic image identity.
- A frontend image scene model or image-specific animation scheduler.
- Silent fallback for unsupported codecs/array layouts.

## Acceptance evidence

Before promotion, record exact commands/runs and prove: resource deduplication; rollback atomicity; clone provenance; bounds/aspect behavior; transform/layout; opacity/alpha; texture reuse; no upload on unchanged/transform-only frames; native/direct-WASM/Python shared behavior; WebGPU/WebGL2 output; and canonical ManimCE v0.21 semantic/raster comparison at required checkpoints.
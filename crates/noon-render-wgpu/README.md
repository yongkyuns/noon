# noon-render-wgpu

`noon-render-wgpu` is Noon's reusable retained GPU renderer for WGPU targets.

## Ownership

This crate projects renderer-facing `noon-runtime` frame state, resources, and change sets into retained GPU resources and draw work. It owns WGPU-specific preparation/upload, command encoding, and the native/web/CI feature split needed to build the same renderer for multiple targets.

The renderer derives GPU state from runtime output. It does not read or own mutable Semantic Scene truth, session wake or scheduling policy, or platform lifecycle. The architecture requires renderer input to become coherent effective runtime state at a published `FrameEpoch`, with versioned resources retained until in-flight submissions are safe to retire; this README does not claim those publication and retirement mechanics are already implemented in this crate.

WGPU and its target feature matrix create a real dependency and compilation boundary, so this remains a crate rather than merely mirroring an architecture box.

## Platform boundary

Platform hosts own native window or browser-canvas lifecycle, surface creation/configuration, event-loop/frame scheduling, input translation, presentation, and recovery policy. `noon-native` owns native lifecycle; `noon-web` owns browser/WASM integration. Keeping those concerns outside this crate leaves the renderer reusable by both hosts.

This boundary follows `docs/architecture.md` and the Phase A5 normalization rules in #960.

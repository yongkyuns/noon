# noon-runtime

`noon-runtime` is Noon's deterministic, renderer-independent execution layer.

## Ownership

This crate owns coherent effective state at a published `FrameEpoch`: active timeline, reactive, native-input, and host execution; dirty and spatial state; and renderer-facing change sets. Its effective values are derived from authored Semantic Scene state and execution data; the runtime does not own authored truth.

It also owns deterministic time advancement, execution-slot identity, structural mutation application, retained/family runtime state, and runtime-local diagnostics. Its frame/change-set output is consumed by `noon-render-wgpu` and browser integration without coupling runtime execution to WGPU, browser APIs, or a platform event loop.

## Boundaries

This crate does not own Semantic Scene authoring or language APIs, semantic analysis/lowering, GPU resources or presentation, native/window/browser lifecycle, or serialization as an in-process engine boundary.

`ExecutionSession` composes existing lowering and runtime state; it is not a fifth authority. Host-facing settle and wake orchestration is owned by #1085, outside this crate-local boundary.

The engine path remains `Semantic Scene -> Execution Plan -> Runtime -> Renderer`. Migration-era compiled-scene and patch types at some seams do not create another authored-scene authority.

This boundary follows `docs/architecture.md` and the Phase A5 normalization rules in #960.

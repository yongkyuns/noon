# Noon documentation

Start with [`architecture.md`](architecture.md) for the **normative architecture and roadmap**. It defines the authority boundaries, invariants, publication model, and architectural direction.

For a code-reading view of the current implementation, use [`type-map.md`](type-map.md). It maps important Rust types and crate boundaries, including `Mobject` → `SemanticStore` → `CompiledScene` → `FrameState` → tessellated/packed WGPU geometry, and includes editable D2 diagrams for both type lowering and the internal Cargo dependency graph.

Additional focused documentation:

- [`mobile-web-rendering.md`](mobile-web-rendering.md) — mobile/browser rendering constraints and decisions.

Current implementation and qualification work is tracked in [3D capability #1879](https://github.com/yongkyuns/noon/issues/1879), [camera dispositions #254](https://github.com/yongkyuns/noon/issues/254), and [performance #1653](https://github.com/yongkyuns/noon/issues/1653). Completed [Phase A #953](https://github.com/yongkyuns/noon/issues/953), [Phase B #954](https://github.com/yongkyuns/noon/issues/954), and [Phase C #955](https://github.com/yongkyuns/noon/issues/955) retain their integration records.

Retired phase scheduling notes and the October 2 Phase C evidence ledger remain in [Git history](https://github.com/yongkyuns/noon/tree/fb2ac928336b2b5c9e91d39c9912e3e826e30663/docs).

Reader diagrams in this directory keep the `.d2` source beside the checked-in `.svg` preview. Update the D2 source first when implementation types or crate handoffs move.

# Noon documentation

Start with [`architecture.md`](architecture.md) for the **normative architecture and roadmap**. It defines the authority boundaries, invariants, publication model, and architectural direction.

For a code-reading view of the current implementation, use [`type-map.md`](type-map.md). It maps important Rust types and crate boundaries, including `Mobject` → `SemanticStore` → `CompiledScene` → `FrameState` → tessellated/packed WGPU geometry, and includes editable D2 diagrams for both type lowering and the internal Cargo dependency graph.

Additional focused documentation:

- [`mobile-web-rendering.md`](mobile-web-rendering.md) — mobile/browser rendering constraints and decisions.
- [`phase-a-parallel-work-tracks.md`](phase-a-parallel-work-tracks.md), [`phase-b-parallel-work-tracks.md`](phase-b-parallel-work-tracks.md), [`phase-c-parallel-work-tracks.md`](phase-c-parallel-work-tracks.md), [`phase-d-parallel-work-tracks.md`](phase-d-parallel-work-tracks.md) — scoped development work-track notes. These do not replace `architecture.md` as the architecture authority.

Reader diagrams in this directory keep the `.d2` source beside the checked-in `.svg` preview. Update the D2 source first when implementation types or crate handoffs move.

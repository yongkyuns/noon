# Allocation and capacity performance

This document tracks transient allocation/churn that can affect frame p95/p99. Eventual reclamation and leak invariants remain owned by #114.

## Ranked sources found so far

| Source | Frequency / scale | Evidence | Status |
|---|---|---|---|
| Browser scene JSON parse | Every full authoring result; O(scene bytes) | 2026-08-19 baseline: 220.244 ms to parse the 25.13 MiB / 100k result | Architecture-scale transport ceiling; P6 measures its time-to-visible contribution |
| Scene identity stabilization | Every keyed Run/rerun; formerly O(objects + tracks) Maps/arrays | baseline: 29.724 ms at 100k before the identity fast path | stable-ID reruns now return the original document without remap Maps/arrays (#146) |
| Runtime requested/active-group snapshots | Every active frame; formerly O(active groups) copies | code audit plus active-vs-history scheduler benchmark | active-set and requested-group frame snapshots were removed by #137/#152; timeline relowering is channel-local |
| Python updater callback materialization | Every host callback phase; formerly two deep object graphs per scene object | host protocol necessarily supplies a coherent all-object snapshot; Python then eagerly copied it | #153 changes Python materialization to touched/read objects only and adds native-vs-host callback profiling |
| GPU instance-buffer growth | Only when packed bytes exceed retained capacity | `UploadStats.buffer_reallocations`; existing noop tests prove zero reallocations after warmup | capacity is geometric (next power of two); `buffer_capacity.rs` pins the exact threshold/no-growth/grow-once behavior |
| Steady renderer preparation/upload | Every presented frame but intended to be allocation/locality-free | baseline unchanged 100k prep ~0.000061 ms and 0 uploaded bytes | deterministic dirty/upload tests remain the primary regression gate |

The JSON row is not a frame-loop allocation in steady playback, but it dominates interaction stalls during large Run/rerun operations and can overlap presentation. Browser GC timing remains diagnostic; deterministic operation/capacity counters are preferred where the browser does not expose stable GC telemetry.

## GPU buffer capacity policy

Analytic instance records use an 88-byte stride. GPU buffers retain capacity and grow only when required bytes exceed current capacity. A growth rounds the requested byte size to the next power of two (minimum 256 bytes), so repeated small increases within the retained capacity do not allocate/copy a new GPU buffer.

`crates/noon-render-wgpu/tests/buffer_capacity.rs` derives the maximum object count from the actual retained byte capacity rather than hard-coding a threshold. It verifies:

1. the first non-empty upload allocates once;
2. growing up to the exact retained capacity triggers no reallocation;
3. the first object beyond capacity triggers exactly one reallocation;
4. that crossing doubles retained capacity for the tested boundary;
5. the following unchanged frame performs zero reallocations and zero uploads.

This makes capacity-threshold churn deterministic in CI while real-browser P0/P5/P6 cadence/long-task profiles remain the place to correlate such events with observed jank.

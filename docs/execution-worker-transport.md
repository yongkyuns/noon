# Browser execution worker transport

Noon's browser execution path is split so UI/editor work, deterministic scene evaluation, and GPU presentation do not share one event loop.

## Ownership

The intended topology is:

- **Main thread:** DOM, editor controls, input collection, worker supervision, and user-visible status only.
- **Engine worker:** semantic/runtime state, timeline evaluation, native reactive updates, ordered patches, and host-language coordination.
- **Render worker:** `OffscreenCanvas`, frame mirroring, GPU preparation/upload, and WebGPU/WebGL2 presentation only.

The render worker never evaluates authored Python and never synchronously waits for Pyodide or another host-language callback. If host work stalls, the render worker keeps its own event loop and retains the latest accepted frame.

## Execution delta protocol

The engine-to-render wire channel is `noon.execution`, protocol version 1. Every message carries:

- `session`: identifies one engine/render lifetime;
- `sequence`: a monotonically increasing delta sequence within that session;
- `snapshot`: whether the message is a complete renderer state;
- `time`: deterministic scene time;
- renderer-visible object state keyed by a stable `{slot, generation}` execution identity.

The first message in a session is snapshot sequence 0. A lower sequence is stale and may be dropped. A forward sequence gap is rejected for a partial delta; a complete snapshot may recover from a gap. A worker restart increments the session and therefore requires a new sequence-0 snapshot.

Stable execution slots follow the same tombstone/generation model introduced by the runtime execution-slot work in #58. Removing an object invalidates its old generation; a later object may reuse the slot only with a new generation. Dense renderer order is carried separately and is not persistent identity.

## Snapshots and steady-state deltas

Structural edits and renderer-order changes force a complete snapshot. During ordinary playback, the engine uses runtime `FrameChanges` to send only objects whose renderer-visible state changed. The render worker turns those dirty object indices back into `FrameChanges` and feeds the existing incremental `FramePreparer`, so transport locality and GPU locality line up.

The transport does not intentionally drop a generated partial delta. Backpressure is checked before the engine evaluates another presentation tick. While transport capacity is exhausted, frame requests coalesce to the newest timestamp; once capacity returns, the engine evaluates that newest timestamp and emits the next consecutive delta. This keeps sequence handling deterministic while avoiding an unbounded frame queue.

## Transport modes

### SharedArrayBuffer fast path

When the page is cross-origin isolated, the engine and renderer use a two-slot shared mailbox. Each slot has an atomic state (`free`, `writing`, `ready`) and byte length. A writer claims a free slot, writes one complete UTF-8 delta, publishes it with an atomic state change, and notifies the render worker. If both slots are occupied, the engine stops evaluating presentation ticks until the renderer frees one.

### Transferable fallback

Without cross-origin isolation, deltas are UTF-8 `ArrayBuffer`s transferred over the engine/render `MessagePort`. At most two buffers may be in flight. The renderer acknowledges each consumed buffer; the engine coalesces presentation ticks while the in-flight limit is reached.

Both modes therefore expose the same sequencing and backpressure semantics. Shared memory is an optimization, not a different correctness model.

## Failure and restart behavior

Worker control envelopes are independently versioned from the execution-delta wire format. Worker errors reject the matching main-thread request and surface through the supervisor error callback. Restart creates a fresh engine/render worker pair, replaces the transferred DOM canvas with a new canvas element, increments the execution session, and starts from a complete scene snapshot.

Renderer-side transient surface loss is recoverable because semantic/runtime state remains owned by the engine. A renderer restart never requires rerunning frontend authoring code to reconstruct the current scene.

## Validation

The transport has native Rust tests for snapshot/partial round trips, stable slots, stale-message handling, sequence gaps, and session restart rules. JavaScript tests exercise SharedArrayBuffer and transferable backpressure. The browser worker smoke runs a cross-origin-isolated page, executes both transport modes with an `OffscreenCanvas`, applies ordered patches, verifies error containment, and restarts the worker pair.

## Default browser path and host callbacks

The playground uses `ExecutionWorkerClient` by default. `NoonCanvasPlayer` remains as a compatibility/profiling path, but the normal UI thread no longer evaluates scene state or submits GPU work. It authors scenes, forwards explicit edits, collects DOM input, and polls worker metrics.

Arbitrary Python updater closures remain in the lazy Pyodide authoring worker. When a scene registers callbacks, the UI transfers a dedicated `MessagePort` between that Python worker and the engine worker. Callback snapshots and patch results then travel directly between those workers; the main thread is not in the per-frame callback loop. The engine launches at most one callback phase at a time, records missed presentation deadlines, drops results that became stale while native time advanced, and commits on-time callback batches through a sequence domain separate from interactive user patches. The render worker never waits for that callback.

Callback scenes currently retain authoring-local object IDs so Python closures and their coherent snapshot table refer to the same identities. Stable callback-aware hot-reload identity reconciliation belongs to #64; ordinary callback-free scenes continue using the playground stable identity adapter.

# Browser memory profiling

This guide complements the deterministic resource-count gates in #114. Browser heap measurements are useful diagnostics, but they are not suitable as required CI assertions because JavaScript garbage collection, browser process accounting, GPU-driver allocation, and memory-pressure behavior vary across runs and platforms.

## What required CI should gate

Prefer deterministic engine-owned counters for required tests:

- live semantic and execution slots;
- retained/reusable slot capacity and generations;
- geometry, text, font, raster, outline, and atlas residency where exposed;
- renderer cache entries and retained buffer capacity;
- worker/callback registrations;
- renderer/device generations.

Long-running churn tests should assert that live counts return to baseline and retained capacities reach a documented plateau. A browser memory profile is evidence for leaks that escape those counters, not a replacement for them.

## Reproducible profile setup

Use a production-style web build and a fresh browser process. Record the exact Noon commit, browser version, OS, backend, viewport, DPR, and transport mode with every profile.

For Chromium, use one profile per scenario and keep DevTools closed until the workload is ready to inspect. Prefer a software-rendered backend for repeatable comparisons; repeat suspected GPU-only leaks on the affected real GPU separately.

Keep the tab focused unless the scenario explicitly tests page visibility or teardown. Browser background throttling changes allocation and collection behavior.

## Standard scenarios

Profile the same workloads as the deterministic #114 gates:

1. Repeated scene replacement: alternate unrelated scenes for at least 1,000 authoring reruns.
2. Bounded working-set churn: repeatedly remove/create objects while keeping the live object count fixed.
3. Local edit churn: apply transform, style, and geometry edits without changing unrelated objects.
4. Example switching: alternate between two representative gallery examples for at least 1,000 switches.
5. Renderer teardown/recreation: repeatedly terminate and restart the execution/render path.
6. Failure recovery: inject a failed load/patch, then recover with a valid scene, repeated many times.
7. Large-to-small contraction: load a deliberately large temporary scene, then replace it with a small scene and continue steady-state edits.

Use a short warmup before recording. The first iterations legitimately allocate compiler, WASM, pipeline, font, shader, and cache state.

## Measurements

Capture three different classes of evidence rather than treating one number as total memory.

### JavaScript heap

Use Chromium's Memory panel or `performance.measureUserAgentSpecificMemory()` where supported. Heap snapshots are best for identifying retained object graphs; sampling is best for allocation hot spots.

Take snapshots at:

- baseline after startup/warmup;
- after the first workload plateau;
- after the full churn run;
- after teardown and a quiescent period.

Compare retained dominators, not just total bytes. Persistent growth in Worker, MessagePort, ArrayBuffer, typed-array, listener, or application-object dominators is actionable.

### WASM memory

Track the backing `WebAssembly.Memory` byte length when it is available through the debugging/runtime surface. Linear memory may grow in pages and normally does not shrink, so the useful invariant is a bounded plateau for a bounded workload rather than a return to the initial byte length.

A profile should distinguish:

- live logical resources returning to baseline;
- allocator/linear-memory high-water mark remaining bounded;
- monotonic growth on every repeated edit.

Only the last case is a leak signal for a bounded workload.

### Browser/GPU process memory

Use the browser task manager or OS process tooling for renderer/GPU-process resident memory. Treat these values as diagnostic only. Driver caches, shader compilation, command buffers, and process-level allocators can retain memory after logical resources are released.

A convincing GPU leak report should pair process growth with an engine-owned counter or a repeated renderer-generation scenario that demonstrates unbounded growth.

## Interpreting plateaus

Do not require memory to return to the cold-start baseline. Expected one-time/high-water allocations include:

- WASM linear-memory growth;
- compiled shader/pipeline caches;
- bounded glyph/raster/outline caches;
- renderer buffers sized to the largest recently supported working set;
- browser-internal decoder and graphics caches.

Document the intended retention policy when a subsystem keeps a high-water allocation. A bounded cache needs an explicit capacity/budget; an unbounded cache is not a valid explanation for monotonic growth.

## Leak triage order

When growth is observed, narrow it in this order:

1. Check deterministic Noon resource counters for a monotonic class.
2. Check worker, callback, observer, listener, and MessagePort lifetimes.
3. Check retained ArrayBuffer/typed-array/WASM wrapper graphs.
4. Check WASM resource arenas and allocator high-water behavior.
5. Check renderer/device generations and GPU cache residency.
6. Only then attribute unexplained residual growth to browser/driver behavior, with a minimal reproduction.

This order keeps product-owned leaks actionable and avoids tuning logic around garbage-collector timing.

## Recording a useful result

For every manual or named-runner profile, record:

- commit SHA;
- scenario and iteration count;
- browser/version and launch flags;
- OS and GPU/backend;
- viewport and DPR;
- transport mode;
- deterministic Noon resource snapshots before/after;
- JS heap/WASM/process measurements before/after;
- whether values plateaued;
- retained-object or process evidence when they did not.

Attach heap snapshots or trace files only when they materially help diagnosis; they can be large and browser-version-specific.

## CI policy

Required CI should continue to gate deterministic live-resource counts and bounded capacities. Browser heap/process measurements belong in manual, scheduled, or named-runner diagnostics until a specific metric is proven stable enough to ratchet without GC- or driver-induced flakes.

When a browser-only leak is fixed, add the cheapest deterministic regression that proves the underlying lifetime invariant. Keep the memory profile as supporting evidence rather than making GC timing the test oracle.

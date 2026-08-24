# Host callback execution semantics

Arbitrary Python/JavaScript callbacks are a compatibility path, not part of
Noon's native frame-critical execution model. They may inspect opaque host state,
perform untraceable control flow, or take longer than a presentation deadline.
The engine therefore makes their semantics explicit instead of pretending that
all three of the following can be guaranteed simultaneously:

1. unrestricted arbitrary host code;
2. exact same-frame blocking behavior;
3. guaranteed realtime presentation deadlines.

## Ordered frame phases

Observable evaluation is equivalent to:

```text
timeline
  -> native dynamic/reactive
  -> host callbacks
  -> derived state (bounds/culling/hit-test inputs)
  -> render/present
```

Backends may fuse internal work, but they must preserve this ordering. If more
than one dynamic phase writes the same property, **later phase wins** and sees
the value produced by the earlier phase. A timeline animation plus a native
updater is therefore not intrinsically invalid; nor is a host callback layered
on top of both. This replaces a global "one driver per property" restriction
with deterministic phase composition.

The language-neutral enums live in
`noon_core::{FrameExecutionPhase, DynamicDriverPhase}`.

## Realtime versus deterministic/offline playback

`HostPlaybackMode::Realtime` is presentation-deadline driven. The presenter is
never required to synchronously wait for arbitrary host code. If a callback
misses its budget, the render side presents the **latest committed state** and a
later completed host transaction may become visible on a subsequent frame.
Missed deadlines are observable profiler events, not silent stalls.

`HostPlaybackMode::DeterministicOffline` prioritizes exact evaluated output and
waits for the host callback phase to commit before the corresponding frame is
accepted. This is the mode for deterministic export/reference validation when a
scene contains arbitrary host callbacks.

The browser worker split in #60 must implement this contract; it must not make
the renderer synchronously dependent on Pyodide in realtime mode.

## Callback reads

A declared snapshot is an optimization, not a complete semantic model for
arbitrary Python. A Python closure may synchronously read a different mobject,
a tracker, or other engine-owned state that was not declared when the callback
was registered.

Two read models are therefore explicit:

- `DeclaredSnapshot`: sufficient for constrained/traced callbacks whose reads
  are known ahead of time;
- `EngineLocalSemanticView`: required for unrestricted callbacks. The host
  runtime and authoritative engine state must be colocated closely enough that
  getters do not become per-property cross-worker round trips.

The existing deduplicated callback snapshot remains useful as a fast transport
for declared state; it is not treated as proof that arbitrary callbacks are
snapshot-complete.

## Seek/replay classes

Every arbitrary callback belongs to one of three semantic classes:

| Replay class | Exact seek requirement |
| --- | --- |
| `Pure` | direct evaluation at the destination |
| `StatefulDeterministic` | restore a compatible checkpoint and replay forward |
| `Opaque` | replay from initialization unless the host provides a stronger contract |

`Opaque` is the safe default. Closure state, randomness, wall time, external I/O,
or other state not owned by Noon cannot be reconstructed from a scene-state
snapshot alone.

A whole scene's exact seek mode is the least-seekable active callback. The
runtime `HostCallbackProfiler` exposes that result so editor/profiler UI can tell
the user whether a seek is direct, checkpoint replay, or initialization replay.

## Deadline and cost instrumentation

`noon_runtime::HostCallbackProfiler` records:

- per-callback call count, latest/average/max duration;
- callback-phase latest/max duration;
- accumulated missed presentation deadlines;
- realtime/offline frame disposition;
- exact scene seek mode implied by registered callback replay classes.

The host records measured callback and phase durations; the profiler is transport
independent so the browser, native hosts, and tests share the same contract.

## Commit semantics

Host writes remain an atomic callback-phase mutation transaction. Property-only
writes may use the current incremental commit path. Structural/timeline edits
that require re-lowering remain explicit higher-impact operations and must not
silently leave native reactive bindings or execution indices stale.

Wave 2's stable execution slots and local transaction journal (#58) will remove
the current full-runtime staging fallback. That optimization does not change the
phase or deadline semantics defined here.

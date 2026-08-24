# Native input signals

Noon models common browser interaction as language-neutral reactive inputs rather than per-frame Python callbacks. Pointer, keyboard, viewport, wheel/gesture, and named UI-control sources are declared in the semantic scene document and routed directly into the native reactive VM.

## State versus events

`NativeStateSource` represents sampled state. Pointer position, pointer-button state, key state, viewport size, wheel/gesture deltas, and named scalar controls update typed reactive input signals. Identical state samples are coalesced before VM execution.

`NativeEventSource` represents discrete occurrences. Pointer down/up, key press/release, wheel, gesture, and control-commit events drive scalar event-sequence signals. Every event advances the sequence signal, so two identical clicks or key presses remain two observable reactive changes.

A reactive signal has one external driver. A signal cannot be both native-input-driven and signal-timeline-driven. Timeline-owned signals continue to reject external writes with the stable `timeline-driven` diagnostic used by existing callers and browser regression tests.

## Browser ordering

For one DOM event that has both state and event semantics, Noon applies the sampled state first and then emits the discrete event. For example, a pointer-down updates the declared pointer-button state before advancing the matching pointer-down event sequence. Key, wheel, and gesture dispatch follow the same rule.

Input dispatch performs no rendering and no Python execution. It only updates the declared reactive dependency closure and dense frame targets. The normal render loop presents the changed frame on its next iteration.

Pointer positions are converted from normalized canvas coordinates to scene/world coordinates in the WASM canvas player using the current camera. Viewport-size and wheel/gesture deltas use canvas-pixel units at the current browser boundary.

## Browser collection

`web/native-inputs.js` contains a thin DOM collector:

- `attachNativeInputs(player, canvas)` forwards pointer, keyboard, and wheel samples/events to `ReactiveCanvasPlayer`.
- `bindNativeControl(player, element, name)` forwards numeric input/range samples and commit events by semantic control name.

The collector knows nothing about signal IDs or scene objects. That separation is intentional: the same source messages can cross the engine-worker transport planned by #60 without changing authored scene documents.

## Performance and instrumentation

`NativeInputRouter` indexes declared sources once when the scene is loaded. Dispatch does not scan scene objects or the full reactive graph. Runtime counters expose received/coalesced/dropped samples, discrete events, reactive updates, derived-signal evaluations, and invalidated bindings. The browser exposes these counters through `nativeInputStatsJson()`.

This path is intended for common high-frequency interaction where a host-language round trip would be unnecessary. Arbitrary user callbacks remain part of the host-callback path, and object hit testing remains a separate concern tracked by #66.

## Current boundaries

This slice deliberately does not implement object-level pointer targeting, gesture recognition, or worker transport. It establishes the semantic source schema, frontend authoring hooks, native routing, browser collection, deterministic event ordering, and CI coverage needed for those later layers.

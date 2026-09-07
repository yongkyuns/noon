# Mobile web render hosting

## Problem

Noon's browser renderer historically always ran in a dedicated Worker. The production `HTMLCanvasElement` was transferred to an `OffscreenCanvas`, sent to the render worker, and initialized by the Rust/`wgpu` renderer there.

The cross-browser matrix reproduced a mobile WebKit environment where the page, gallery, Workers, and canvas APIs were available but WebGL2 could not be created on the transferred canvas in the worker. The old test treated that condition as an unsupported-runtime skip, so the WebKit job could remain green without executing a scene.

The failure is a **render-host** limitation. It is separate from scene semantics and from `wgpu`'s WebGPU-versus-WebGL backend choice.

## Implementation

Noon now has two render hosts behind the existing execution protocol:

```text
execution / semantic owner
          |
          v
   render protocol
      /       \
 worker     main thread
      \       /
   shared render controller
          |
   Rust / wgpu renderer
```

`web/authoring-render-controller.js` provides an instance-owned render controller from the state machine that previously lived directly in `authoring-render-worker.js`. Each render endpoint creates its own controller; the worker file is a thin adapter. `MainThreadRenderWorker` implements the same endpoint shape with `EventTarget`/`postMessage`, but dispatches the protocol to that shared controller on the browser main thread.

There is still one authoritative `ExecutionWorkerClient`. Its engine, semantic execution, session, transport, reconnect, and recovery logic are unchanged. Only creation of the render endpoint is host-selectable.

### Canvas contract

Both hosts currently use the existing Rust binding contract: an `OffscreenCanvas` produced by `HTMLCanvasElement.transferControlToOffscreen()`.

The difference is where that `OffscreenCanvas` is consumed:

- `worker`: transferred into the dedicated render Worker
- `main-thread`: retained on the main thread and passed to the same WASM/`wgpu` renderer there

This PR does **not** add direct `HTMLCanvasElement` support to the Rust renderer and does not introduce a second renderer.

### Backend ownership

Noon chooses only the render host. The Rust renderer/`wgpu` stack continues to choose and expose the usable GPU backend (`WebGPU` or `WebGL2`). Product code does not add a Safari-specific WebGL renderer or duplicate `wgpu` backend selection.

## Host selection

`web/render-host-selection.js` prefers the worker host.

Before the production canvas is transferred, it probes disposable surfaces:

1. try a transferred worker surface for WebGPU or WebGL2
2. if that fails, try a main-thread transferred `OffscreenCanvas` for WebGPU or WebGL2
3. fail if neither surface configuration is available; a later attempt may probe again

Probe worker construction failures also reach the main-thread check, and disposable probe contexts are released.

This is intentionally a lightweight browser-surface preflight, not a second implementation of `wgpu` adapter selection. The definitive renderer check still occurs when the real Rust/`wgpu` renderer initializes; browser CI exercises that path end-to-end.

For deterministic testing, the host can be forced with:

```text
?renderHost=worker
?renderHost=main-thread
```

or `globalThis.__NOON_RENDER_HOST__` before execution startup.

The selected host is exposed separately from the renderer backend through execution diagnostics/metrics:

```text
renderHost: worker | main-thread
renderer backend: WebGPU | WebGL2
```

## Lifecycle invariants

- Host selection completes before the production canvas is transferred.
- Worker rendering remains the preferred path when its disposable surface probe succeeds.
- Both hosts use the same render protocol and shared controller.
- Engine and Python execution remain off the main thread as before.
- Full renderer restart preserves the selected host for that execution client.
- Normal termination replaces a transferred DOM canvas using the existing recovery path.
- Each main-thread endpoint owns its controller; concurrent clients and terminated clients cannot change another endpoint’s renderer.
- Startup is reserved before asynchronous host probing; cancellation prevents a late probe or renderer bootstrap from reviving a terminated owner.

## Browser regression coverage

The cross-browser matrix keeps presentation coverage on Chromium, Firefox, and mobile WebKit. It calls the production host selector from the served page before starting execution, so runtime support and actual startup use the same secure browser realm and capability decision. Runtime execution is required whenever either supported host exposes a usable GPU surface. An environment with neither host remains an explicit unsupported-runtime result rather than a false product failure; the capability result is saved with the job diagnostics.

This distinction matters for the current headless Firefox runner, which exposes the surrounding canvas/Worker APIs but no usable WebGL2 surface in either supported host. Mobile WebKit, by contrast, exposes the main-thread surface and therefore must execute rather than skip.

The normal runtime smoke verifies deferred startup, scene execution, example selection, edit/rerun, resize, and absence of page/console errors on every environment with a usable host.

A dedicated mobile WebKit test additionally forces `renderHost=main-thread` and verifies:

- geometry execution
- Text execution
- edit + rerun
- mobile viewport changes
- successful frame presentation
- `renderHost === "main-thread"`

## Scope and follow-ups

This change intentionally does not:

- add browser-name or Safari-specific runtime branches
- move Python or semantic execution to the main thread
- duplicate the scene model or animation scheduler
- add a JavaScript WebGL renderer
- generalize the Rust surface binding to `HTMLCanvasElement`
- redesign updater/callback lifecycle semantics

The existing `RotationUpdater` live-lifecycle gap remains owned by the updater/gallery roadmap (#252); it is not coupled to render-host selection.

Playwright WebKit provides required regression coverage for the host fallback, but real-device iOS Safari remains useful additional qualification. If future browsers make worker-hosted rendering usable, the capability selection will naturally prefer the worker path again without a user-agent rule.

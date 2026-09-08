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

1. try a transferred worker surface with a usable WebGPU adapter or WebGL2
2. if that fails, try a main-thread transferred `OffscreenCanvas` with the same adapter/context checks
3. fail if neither surface configuration is available; a later attempt may probe again

Probes use DOM-connected disposable canvases, a module worker, and the same antialias-disabled WebGL2 context settings as wgpu. Probe worker construction failures also reach the main-thread check. Disposable contexts are released and canvases removed after probing. Renderer initialization errors retain browser surface-creation diagnostics.

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

Browser API presence alone does not establish runtime support. If the production selector finds a surface, the matrix requires the real renderer to initialize and present; a later renderer failure is a test failure, not an unsupported-runtime skip.

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


## Python continuation portability

A usable render host does not establish that an interpreter can suspend. Issue
#1207 reproduced a fatal Pyodide JSPI `SuspendError` on mobile WebKit even with the
main-thread renderer forced. The minimal `run_sync(Promise.resolve(...))` control
also failed without Noon. Browsers without JSPI reject that synchronous path.

The browser source loader now compiles eligible ordinary `construct` methods with
direct, statement-position `self.play(...)` / `self.wait(...)` calls to the existing
async continuation contract. It executes the original module once, then binds the
selected portable function code to the original globals, defaults and closure.
Module, class, decorator and default-value effects are not replayed. `setup()` runs
before selecting the current method. Reference examples and editor source are not
rewritten, and explicit document exports still execute the original function.

This is Python host control flow, not animation lowering: the same Rust operations,
segment admission, authored-time clock, callback protocol, completion receipt and
renderer own every frame. There is no endpoint-only or legacy-document fallback.
The source coroutine yields only at its actual canonical barrier. An unexpected
indirect barrier rejects before that operation can mutate the shared scene.

The portable compilation is intentionally bounded. Aliased/returned barriers,
scene escapes to helpers, custom scene-method calls, decorators, generators and
nested functions retain original synchronous execution. Those patterns still need
working JSPI, or explicitly async code that awaits its barriers. Arbitrary Python
blocking I/O and synchronous callback reads are not made portable by this repair.
Unhandled interpreter-fatal rejections now use the existing fatal authoring channel,
reject pending work and close the failed worker instead of leaving Run pending.

The main-thread mobile regression uses a real mobile/touch browser profile, checks
forced hosting and automatic hosting with JSPI deliberately absent, requires a
visible intermediate SquareToCircle frame and the final FadeOut removal at the
actual authored time, and retains Text, edit/rerun and resize coverage. Browser
emulation does not establish physical-device iOS qualification.


### Review hardening and qualification

Portable execution admits only the ordinary `play`, `wait`, `add`, `remove` and
`clear` methods, checked on the instance after `setup()` without invoking authored
properties. Instance/class overrides, custom attribute lookup, method replacement
and explicit `__dict__` access retain the original synchronous path. In particular,
an overridden membership method must not hide an uncompiled suspension barrier.

Static, explicitly async and export-only input bypasses Python AST preparation.
Eligible synchronous constructs still pay a one-time source preparation cost; this
is not a claim of literally identical startup cost for every source size. No
source analysis or new Python work is added to the renderer's per-frame loop.

The mobile test exercises the real initial autoplay. It briefly holds the worker
module's network response to capture an empty canvas, then releases the unchanged
worker and observes intermediate rendering. This avoids mistaking a replacement's
blocked metrics request (old-context retirement waits behind the new construct)
for missing intermediate frames. Pixel comparisons use the complete empty canvas,
including fractional clipping edges, rather than assuming its top-left pixel is
the renderer's background color. Existing pixel and authored-time thresholds stay
unchanged. Test artifacts retain the empty, intermediate and final captures.

The same browser test also runs setup-installed overrides, property and dynamic
lookup, and explicit async source through the public editor and Run path. The
source compiler's unit tests cover ordering, cancellation, globals/defaults/closure
preservation, unsupported source rejection and the no-AST fast paths.


### Full-gallery continuation follow-up (#1207)

The portable host compiler also admits synchronous lambda and nested callback
bodies which do not reference the outer scene or hide a play/wait barrier. It
leaves those bodies unchanged and preserves their ordinary Python closure and
callable identity. Direct module-level statement-position play/wait calls use
Python top-level await in the original module namespace. Definitions and module
effects execute once; arbitrary non-Scene methods and returned awaitables are
not treated as canonical barriers. Export execution keeps ordinary compilation.
This remains bounded host-language portability, not a general synchronous
Python compiler or a second animation scheduler.

Updater removal/replacement after a completed segment is a shared Rust semantic
transaction. The compiler prepares a revised callback index from semantic
preflight before the existing session atomically publishes it. Runtime identity,
current time, and the last effective frame survive the change; removal freezes
that effective value instead of restoring the authored value. Pending callback
phases, retroactive edits, mixed structural/registration transactions, and first
registration on a target absent from the initial callback index are rejected.
The latter two remain explicitly unsupported; they are not silently replayed.

This first bounded registration publication rebuilds the callback-only index in
O(R log R) time and O(R) temporary storage, where R is retained callback occurrence
history. It does not traverse or relower unrelated scene geometry, reset the
runtime, or add new per-frame work. It is not an O(1) registration edit. More
incremental callback-index editing remains under the shared live-session work
owned by #969; no temporary frontend schedule or compatibility authority is added.

The native `ordinary_live_updater_lifecycle` example and the direct Rust/WASM
pixel probe execute the same sequential remove/reverse/remove program as the
unchanged Python RotationUpdater gallery example. The full-gallery browser gate
executes every ready source and explicitly disables JSPI for the four cases
identified by the public audit. This complements, rather than replaces, the
canonical raster/timeline qualification and performance gates.

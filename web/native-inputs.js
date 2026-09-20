import { attachBrowserPointerInput } from "./browser-pointer-input.js";

// Lifetimes belong to the host, not a listener attachment. Retired DOM callbacks
// cannot reuse a source ID when the same direct renderer is attached again.
const pointerLifetimes = new WeakMap();
const MAX_DIRECT_POINTER_SAMPLES = 64;

function wheelLinePixels(canvas) {
  const view = canvas.ownerDocument?.defaultView;
  if (typeof view?.getComputedStyle !== "function") {
    throw new TypeError("canvas must provide computed style for line-mode wheel input");
  }
  const style = view.getComputedStyle(canvas);
  const lineHeight = Number.parseFloat(style.lineHeight);
  if (Number.isFinite(lineHeight) && lineHeight > 0) return lineHeight;
  const fontSize = Number.parseFloat(style.fontSize);
  if (Number.isFinite(fontSize) && fontSize > 0) return fontSize;
  throw new TypeError("canvas must have a positive CSS line-height or font-size");
}

function wheelDeltaCssPixels(canvas, event, resolveLinePixels) {
  switch (event.deltaMode) {
    case 0:
      return { x: event.deltaX, y: event.deltaY };
    case 1: {
      const linePixels = resolveLinePixels();
      return { x: event.deltaX * linePixels, y: event.deltaY * linePixels };
    }
    case 2: {
      const rect = canvas.getBoundingClientRect();
      return { x: event.deltaX * rect.width, y: event.deltaY * rect.height };
    }
    default:
      throw new RangeError(`unsupported WheelEvent.deltaMode ${event.deltaMode}`);
  }
}

/**
 * Attach browser pointer/keyboard/wheel input to one canonical execution host.
 *
 * The collector owns DOM normalization and listener lifetime only. Source
 * routing, semantic identity, reactive evaluation, and rendering remain in the
 * canonical Rust session behind the host methods.
 */
export function attachNativeInputs(
  host,
  canvas,
  {
    keyboardTarget = window,
    preventWheelDefault = false,
    onError = defaultInputError,
    onInput = () => {},
    pointer = true,
  } = {},
) {
  validateHost(host, pointer ? ["nativePointerInput", "nativeKey", "nativeWheel"] : ["nativeKey", "nativeWheel"]);
  if (!canvas) throw new TypeError("host and canvas are required");
  if (typeof onError !== "function") throw new TypeError("onError must be a function");
  if (typeof onInput !== "function") throw new TypeError("onInput must be a function");
  let attached = true;
  const controller = new AbortController();
  const fail = error => {
    if (!attached) return;
    attached = false;
    controller.abort();
    onError(error);
  };
  const notify = value => {
    if (!attached) return;
    try { onInput(value); } catch (error) { fail(error); }
  };
  const invoke = (method, ...args) => {
    if (!attached) return;
    const result = host[method](...args);
    if (result && typeof result.then === "function") result.then(notify, fail);
    else notify(result);
  };
  const guarded = (method, ...args) => {
    try { invoke(method, ...args); } catch (error) { fail(error); }
  };
  const lifetime = pointerLifetimes.get(host) ?? { source: 0, view: 0 };
  pointerLifetimes.set(host, lifetime);
  const increment = key => {
    if (lifetime[key] >= Number.MAX_SAFE_INTEGER) throw new RangeError("pointer lifetime exhausted");
    return ++lifetime[key];
  };
  const collector = pointer ? attachBrowserPointerInput(canvas, {
    signal: controller.signal, isCurrent: () => attached,
    allocateSource: () => increment("source"), viewRevision: () => lifetime.view,
    advanceView: () => increment("view"), maxSamples: MAX_DIRECT_POINTER_SAMPLES,
    windowTarget: canvas.ownerDocument?.defaultView ?? keyboardTarget,
    onError: fail,
    send: sample => invoke("nativePointerInput", sample.kind, sample.source_id,
      sample.pointer_id, sample.view_revision, sample.surface_x ?? undefined,
      sample.surface_y ?? undefined, sample.viewport_width ?? undefined,
      sample.viewport_height ?? undefined, sample.button ?? undefined,
      sample.shift === true, sample.control === true, sample.alt === true, sample.meta === true),
  }) : null;
  const keyDown = (event) => guarded("nativeKey", event.code, true);
  const keyUp = (event) => guarded("nativeKey", event.code, false);
  let cachedLinePixels = null;
  const resolveLinePixels = () => {
    cachedLinePixels ??= wheelLinePixels(canvas);
    return cachedLinePixels;
  };
  const wheel = (event) => {
    if (preventWheelDefault) event.preventDefault();
    const delta = wheelDeltaCssPixels(canvas, event, resolveLinePixels);
    guarded("nativeWheel", delta.x, delta.y);
  };

  canvas.addEventListener("wheel", wheel, { passive: !preventWheelDefault, signal: controller.signal });
  keyboardTarget.addEventListener("keydown", keyDown, { signal: controller.signal });
  keyboardTarget.addEventListener("keyup", keyUp, { signal: controller.signal });

  return () => {
    if (attached) {
      try { collector?.invalidateView(); } catch (error) { fail(error); }
    }
    attached = false;
    controller.abort();
    canvas.removeEventListener("wheel", wheel);
    keyboardTarget.removeEventListener("keydown", keyDown);
    keyboardTarget.removeEventListener("keyup", keyUp);
  };
}

/** Bind a numeric input/range element to a named native scalar control. */
export function bindNativeControl(
  player,
  element,
  name,
  { onError = defaultInputError } = {},
) {
  validateHost(player, ["nativeControl", "nativeControlCommit"]);
  if (!element) throw new TypeError("host and element are required");
  if (typeof onError !== "function") throw new TypeError("onError must be a function");
  if (typeof name !== "string" || name.trim().length === 0) {
    throw new TypeError("control name must be a non-empty string");
  }

  const sample = () => {
    const value = Number(element.value);
    if (!Number.isFinite(value)) throw new TypeError("control value must be finite");
    invokeHost(player, "nativeControl", [name, value], onError);
  };
  const commit = () => {
    sample();
    invokeHost(player, "nativeControlCommit", [name], onError);
  };

  element.addEventListener("input", sample);
  element.addEventListener("change", commit);
  sample();

  return () => {
    element.removeEventListener("input", sample);
    element.removeEventListener("change", commit);
  };
}

/**
 * Adapt the genuine semantic execution-worker boundary to the DOM host shape.
 *
 * Pointer records use the same occurrence-local route as the ordinary authoring
 * client. If that client already owns the canvas pointer collector, attach this
 * host with `pointer: false`. JavaScript performs no scene-coordinate conversion
 * or split pointer state/event writes. The execution client's existing bound remains authoritative.
 */
export function createExecutionWorkerNativeInputHost(
  client,
  { maxInFlight = 64 } = {},
) {
  if (
    typeof client?.setNativeStateInput !== "function" ||
    typeof client?.emitNativeEvent !== "function"
  ) {
    throw new TypeError("worker native input requires a canonical execution client");
  }
  if (!Number.isSafeInteger(maxInFlight) || maxInFlight <= 0) {
    throw new TypeError("maxInFlight must be a positive safe integer");
  }
  let inFlight = 0;
  const submit = (operations) => {
    if (inFlight + operations.length > maxInFlight) {
      return Promise.reject(new Error(
        "native input admission is full; wait for pending commands before retrying",
      ));
    }
    inFlight += operations.length;
    const results = operations.map((operation) => {
      try {
        return Promise.resolve(operation());
      } catch (error) {
        return Promise.reject(error);
      }
    }).map((result) => result.finally(() => {
      inFlight -= 1;
    }));
    return results.length === 1 ? results[0] : Promise.all(results);
  };
  const state = (source, value) => () => client.setNativeStateInput(source, value);
  const event = (source) => () => client.emitNativeEvent(source);

  const host = {
    nativeKey(code, pressed) {
      return submit([
        state({ kind: "key", code }, { kind: "bool", value: pressed }),
        event({ kind: pressed ? "key_press" : "key_release", code }),
      ]);
    },
    nativeWheel(x, y) {
      return submit([
        state({ kind: "wheel_delta" }, { kind: "vec2", x, y }),
        event({ kind: "wheel" }),
      ]);
    },
    nativeControl(name, value) {
      return submit([state({ kind: "control", name }, { kind: "scalar", value })]);
    },
    nativeControlCommit(name) {
      return submit([event({ kind: "control_commit", name })]);
    },
  };
  // AuthoringExecutionClient already owns its canvas pointer collector. Expose
  // this entry only for a client that actually supports explicit pointer input;
  // nonpointer/control adapters must not create a bypass around that ownership.
  if (typeof client.submitBrowserPointerInput === "function") {
    host.nativePointerInput = (kind, source_id, pointer_id, view_revision,
      surface_x, surface_y, viewport_width, viewport_height, button, shift, control, alt, meta) =>
      submit([() => client.submitBrowserPointerInput({
        kind, source_id, pointer_id, view_revision, surface_x, surface_y,
        viewport_width, viewport_height, button, shift, control, alt, meta,
      })]);
  }
  return Object.freeze(host);
}

function validateHost(host, methods) {
  if (!host) throw new TypeError("canonical native input host is required");
  for (const method of methods) {
    if (typeof host[method] !== "function") {
      throw new TypeError(`canonical native input host requires ${method}`);
    }
  }
}

function invokeHost(host, method, args, onError) {
  try {
    const result = host[method](...args);
    if (result && typeof result.then === "function") {
      result.catch(onError);
    }
  } catch (error) {
    onError(error);
  }
}

function defaultInputError(error) {
  queueMicrotask(() => {
    throw error;
  });
}

import { MAX_PENDING_SEMANTIC_CONTROLS } from "./semantic-engine-endpoint.js";

function normalizedPointer(canvas, event) {
  const rect = canvas.getBoundingClientRect();
  if (!(rect.width > 0) || !(rect.height > 0)) return null;
  return {
    x: (event.clientX - rect.left) / rect.width,
    y: (event.clientY - rect.top) / rect.height,
  };
}

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
  } = {},
) {
  validateHost(host);
  if (!canvas) throw new TypeError("host and canvas are required");
  if (typeof onError !== "function") throw new TypeError("onError must be a function");
  const invoke = (method, ...args) => invokeHost(host, method, args, onError);

  const pointerMove = (event) => {
    const point = normalizedPointer(canvas, event);
    if (point !== null) invoke("nativePointerPosition", point.x, point.y);
  };
  const pointerDown = (event) => {
    pointerMove(event);
    invoke("nativePointerButton", event.button, true);
  };
  const pointerUp = (event) => {
    pointerMove(event);
    invoke("nativePointerButton", event.button, false);
  };
  const keyDown = (event) => invoke("nativeKey", event.code, true);
  const keyUp = (event) => invoke("nativeKey", event.code, false);
  let cachedLinePixels = null;
  const resolveLinePixels = () => {
    cachedLinePixels ??= wheelLinePixels(canvas);
    return cachedLinePixels;
  };
  const wheel = (event) => {
    if (preventWheelDefault) event.preventDefault();
    const delta = wheelDeltaCssPixels(canvas, event, resolveLinePixels);
    invoke("nativeWheel", delta.x, delta.y);
  };

  canvas.addEventListener("pointermove", pointerMove);
  canvas.addEventListener("pointerdown", pointerDown);
  canvas.addEventListener("pointerup", pointerUp);
  canvas.addEventListener("wheel", wheel, { passive: !preventWheelDefault });
  keyboardTarget.addEventListener("keydown", keyDown);
  keyboardTarget.addEventListener("keyup", keyUp);

  return () => {
    canvas.removeEventListener("pointermove", pointerMove);
    canvas.removeEventListener("pointerdown", pointerDown);
    canvas.removeEventListener("pointerup", pointerUp);
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
  validateHost(player);
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
 * Pointer conversion is supplied by the platform integration because canonical
 * pointer signals carry scene coordinates. Calls are admitted synchronously and
 * then forwarded immediately; the endpoint's bounded control queue remains the
 * only ordered queue.
 */
export function createExecutionWorkerNativeInputHost(
  client,
  { pointerToScene, maxInFlight = MAX_PENDING_SEMANTIC_CONTROLS } = {},
) {
  if (
    typeof client?.setNativeStateInput !== "function" ||
    typeof client?.emitNativeEvent !== "function"
  ) {
    throw new TypeError("worker native input requires a canonical execution client");
  }
  if (typeof pointerToScene !== "function") {
    throw new TypeError("worker native input requires pointerToScene");
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

  return Object.freeze({
    nativePointerPosition(normalizedX, normalizedY) {
      const point = pointerToScene(normalizedX, normalizedY);
      if (!Number.isFinite(point?.x) || !Number.isFinite(point?.y)) {
        throw new TypeError("pointerToScene must return finite x and y coordinates");
      }
      return submit([state(
        { kind: "pointer_position" },
        { kind: "vec2", x: point.x, y: point.y },
      )]);
    },
    nativePointerButton(button, pressed) {
      return submit([
        state(
          { kind: "pointer_button", button },
          { kind: "bool", value: pressed },
        ),
        event({ kind: pressed ? "pointer_down" : "pointer_up", button }),
      ]);
    },
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
  });
}

function validateHost(host) {
  if (!host) throw new TypeError("canonical native input host is required");
  for (const method of [
    "nativePointerPosition",
    "nativePointerButton",
    "nativeKey",
    "nativeWheel",
    "nativeControl",
    "nativeControlCommit",
  ]) {
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

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
 * Attach browser pointer/keyboard/wheel input to a ReactiveCanvasPlayer.
 *
 * The collector only forwards semantic source samples/events. It never calls
 * Python and never renders synchronously; the existing render loop presents the
 * resulting native reactive state on its next frame.
 */
export function attachNativeInputs(
  player,
  canvas,
  { keyboardTarget = window, preventWheelDefault = false } = {},
) {
  if (!player || !canvas) throw new TypeError("player and canvas are required");

  const pointerMove = (event) => {
    const point = normalizedPointer(canvas, event);
    if (point !== null) player.dispatchPointerPosition(point.x, point.y);
  };
  const pointerDown = (event) => {
    pointerMove(event);
    player.dispatchPointerButton(event.button, true);
  };
  const pointerUp = (event) => {
    pointerMove(event);
    player.dispatchPointerButton(event.button, false);
  };
  const keyDown = (event) => player.dispatchKey(event.code, true);
  const keyUp = (event) => player.dispatchKey(event.code, false);
  let cachedLinePixels = null;
  const resolveLinePixels = () => {
    cachedLinePixels ??= wheelLinePixels(canvas);
    return cachedLinePixels;
  };
  const wheel = (event) => {
    if (preventWheelDefault) event.preventDefault();
    const delta = wheelDeltaCssPixels(canvas, event, resolveLinePixels);
    player.dispatchWheel(delta.x, delta.y);
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
export function bindNativeControl(player, element, name) {
  if (!player || !element) throw new TypeError("player and element are required");
  if (typeof name !== "string" || name.trim().length === 0) {
    throw new TypeError("control name must be a non-empty string");
  }

  const sample = () => {
    const value = Number(element.value);
    if (!Number.isFinite(value)) throw new TypeError("control value must be finite");
    player.dispatchControl(name, value);
  };
  const commit = () => {
    sample();
    player.dispatchControlCommit(name);
  };

  element.addEventListener("input", sample);
  element.addEventListener("change", commit);
  sample();

  return () => {
    element.removeEventListener("input", sample);
    element.removeEventListener("change", commit);
  };
}

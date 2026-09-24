export function wheelLinePixels(canvas) {
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

export function wheelDeltaCssPixels(canvas, event, resolveLinePixels, rect = null) {
  if (!Number.isFinite(event.deltaX) || !Number.isFinite(event.deltaY)) {
    throw new TypeError("wheel deltas must be finite");
  }
  switch (event.deltaMode) {
    case 0:
      return checked(event.deltaX, event.deltaY);
    case 1: {
      const linePixels = resolveLinePixels();
      return checked(event.deltaX * linePixels, event.deltaY * linePixels);
    }
    case 2: {
      rect ??= canvas.getBoundingClientRect();
      return checked(event.deltaX * rect.width, event.deltaY * rect.height);
    }
    default:
      throw new RangeError(`unsupported WheelEvent.deltaMode ${event.deltaMode}`);
  }
}

function checked(x, y) {
  if (!Number.isFinite(x) || !Number.isFinite(y)) throw new RangeError("normalized wheel delta is not finite");
  return { x, y };
}

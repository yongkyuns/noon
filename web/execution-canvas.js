export function replaceExecutionCanvas(canvas) {
  if (!(canvas instanceof HTMLCanvasElement)) {
    throw new TypeError("replaceExecutionCanvas requires an HTMLCanvasElement");
  }
  const replacement = canvas.cloneNode(false);
  replacement.width = canvas.width;
  replacement.height = canvas.height;
  canvas.replaceWith(replacement);
  return replacement;
}

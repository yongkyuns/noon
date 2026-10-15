// Project disposable Rust/WASM option values for Pyodide. No option policy here.
export function resolveRenderOptionsPlain(resolve, ...args) {
  const value = resolve(...args);
  try {
    return {
      pixelWidth: value.pixelWidth,
      pixelHeight: value.pixelHeight,
      frameRateNumerator: value.frameRateNumerator,
      frameRateDenominator: value.frameRateDenominator,
      format: value.format,
    };
  } finally {
    value.free();
  }
}

// Copy the typed Rust result before releasing its WASM owner.
export function resolveAnimationOptionsPlain(resolveAnimationOptions, ...args) {
  const result = resolveAnimationOptions(...args);
  try {
    return {
      runTime: result.runTime,
      rateFunc: result.rateFunc,
      lagRatio: result.lagRatio,
      pathArc: result.pathArc,
      reverseRateFunction: result.reverseRateFunction,
    };
  } finally {
    result.free();
  }
}


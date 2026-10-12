// Shared by the Node harness and browser observer; no Node-only imports.
export const animatedPlottingCases = {
  coordinates: {
    factory: "createAnimatedCoordinatePlottingRenderer", source: "coordinate_plotting",
    output: "animated-coordinates", count: 17, duration: 5.1,
    boundaries: [0, 0.5, 1.3, 3.3, 4.3, 5.1], waits: [[4.3, 5.1]],
    checkpoints: [0, 0.25, 0.5, 0.9, 1.3, 1.8, 2.3, 3.3, 3.8, 4.3, 4.7, 5.1],
  },
  "number-line": {
    factory: "createAnimatedNumberLineRenderer", source: "animated_number_line",
    output: "animated-number-line", count: 22, duration: 8.8,
    boundaries: [0, 0.5, 1.5, 2, 3.5, 4, 5.2, 5.7, 6.5, 8, 8.8],
    waits: [[3.5, 4], [5.2, 5.7], [8, 8.8]],
    checkpoints: [0, 0.25, 0.5, 1, 1.5, 1.75, 2, 2.75, 3.5, 3.75, 4, 4.6,
      5.2, 5.45, 5.7, 6.1, 6.5, 7.25, 8, 8.4, 8.8],
  },
};

export function animatedPresentedTime(time, mode) {
  const example = animatedPlottingCases[mode];
  if (!example || !Number.isFinite(time) || time < 0 || time > example.duration) {
    throw new RangeError("invalid animated plotting sample");
  }
  const interval = example.waits.find(([start, end]) => time > start && (time < end || (time === end && end === example.duration)));
  return interval ? interval[0] : time;
}

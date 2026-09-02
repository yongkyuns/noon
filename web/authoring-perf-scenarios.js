import { summarizeSamples } from "./frame-metrics.js";

export function stableCameraSweepTargets(sampleCount, cameraHeight) {
  if (!Number.isSafeInteger(sampleCount) || sampleCount <= 0) {
    throw new Error("sampleCount must be a positive integer");
  }
  if (!Number.isFinite(cameraHeight) || cameraHeight <= 0) {
    throw new Error("cameraHeight must be positive and finite");
  }

  // Keep the motion deliberately small so the benchmark changes only camera
  // uniforms while preserving the same visible candidate/painter topology.
  const xAmplitude = cameraHeight * 0.01;
  const yAmplitude = cameraHeight * 0.006;
  return Array.from({ length: sampleCount }, (_, index) => {
    const phase = (index / sampleCount) * Math.PI * 2;
    return {
      x: Math.cos(phase) * xAmplitude,
      y: Math.sin(phase) * yAmplitude,
    };
  });
}

export function summarizeStableCameraProfile(samples) {
  if (!Array.isArray(samples) || samples.length === 0) {
    throw new Error("camera profile requires at least one sample");
  }

  const drawCalls = uniqueFiniteMetric(samples, "drawCalls");
  const instances = uniqueFiniteMetric(samples, "instances");
  if (drawCalls.size !== 1 || instances.size !== 1) {
    throw new Error(
      "camera profile changed visible draw topology; increase camera margin or inspect culling",
    );
  }

  return {
    samples: samples.length,
    stableDrawCalls: [...drawCalls][0],
    stableInstances: [...instances][0],
    timeToVisibleMs: summarizeSamples(samples.map(({ timeToVisibleMs }) => timeToVisibleMs)),
    runtimeMs: summarizeSamples(samples.map(({ frame }) => frame.runtimeMs)),
    prepareMs: summarizeSamples(samples.map(({ frame }) => frame.prepareMs)),
    uploadMs: summarizeSamples(samples.map(({ frame }) => frame.uploadMs)),
    encodeSubmitMs: summarizeSamples(samples.map(({ frame }) => frame.encodeSubmitMs)),
    uploadBytes: summarizeSamples(samples.map(({ frame }) => frame.uploadBytes)),
  };
}

function uniqueFiniteMetric(samples, name) {
  const values = new Set();
  for (const { frame } of samples) {
    const value = frame?.[name];
    if (!Number.isFinite(value) || value < 0) {
      throw new Error(`camera profile sample has invalid ${name}`);
    }
    values.add(value);
  }
  return values;
}

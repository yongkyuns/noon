import { isDeepStrictEqual } from "node:util";

// Select actual reference frames, never fabricated timestamps. Explicit times
// are for contract boundaries; fraction-based sampling remains the default.
export function sampleRasterFrames(frameTimes, sampleFractions, sampleTimes) {
  if (!Array.isArray(frameTimes) || frameTimes.length === 0) {
    throw new Error("expected a non-empty reference frame timeline");
  }
  let previous = -Infinity;
  for (const [index, time] of frameTimes.entries()) {
    if (!Number.isFinite(time) || time < 0 || time + 1e-12 < previous) {
      throw new Error(`invalid logical time for reference frame ${index}`);
    }
    previous = time;
  }

  let indices;
  if (sampleTimes !== undefined) {
    if (!Array.isArray(sampleTimes) || sampleTimes.length === 0) {
      throw new Error("sample_times must be a non-empty array");
    }
    let previousRequested = -Infinity;
    indices = sampleTimes.map((time) => {
      if (!Number.isFinite(time) || time < 0 || time <= previousRequested) {
        throw new Error("sample_times must be finite, non-negative and strictly increasing");
      }
      previousRequested = time;
      // This tolerance accounts only for binary clock arithmetic. It must not
      // silently substitute a nearby video frame for a requested boundary.
      const index = frameTimes.findIndex((frameTime) => Math.abs(frameTime - time) <= 1e-9);
      if (index === -1) {
        throw new Error(`no reference frame at requested logical time ${time}`);
      }
      return index;
    });
  } else {
    if (!Array.isArray(sampleFractions) || sampleFractions.length === 0) {
      throw new Error("expected non-empty sample fractions");
    }
    indices = sampleFractions.map((fraction) => {
      const value = Number(fraction);
      if (!Number.isFinite(value) || value < 0 || value > 1) {
        throw new Error("sample fractions must be finite and in [0, 1]");
      }
      return Math.round((frameTimes.length - 1) * value);
    });
  }
  return [...new Set(indices)].map((frameIndex) => ({
    frameIndex,
    time: frameTimes[frameIndex],
    label: `frame-${String(frameIndex).padStart(4, "0")}`,
  }));
}

function visualState(state) {
  if (!state || typeof state !== "object" || Array.isArray(state)) return state;
  return Object.fromEntries(Object.entries(state)
    .filter(([key]) => !["frame_index", "time", "animation_time"].includes(key)));
}

// Resolve requested logical samples to existing Cairo PNGs. An authored
// endpoint may reuse the final PNG only when the terminal visual state is
// exactly the state of that materialized frame. An empty semantic timeline is
// accepted only for a zero-duration scene with exactly one independently
// observed Cairo PNG; its terminal state describes that sole static output.
export function resolveRasterReferenceSamples(frameTimes, sampleTimes, {
  logicalDuration, terminalState, pngFrameCount, semanticFrames, sampleFractions,
}) {
  if (!Array.isArray(frameTimes) || !Number.isFinite(logicalDuration) || logicalDuration < 0
      || !Number.isInteger(pngFrameCount) || pngFrameCount < 1) {
    throw new Error("invalid logical duration or independently observed PNG count");
  }
  if (frameTimes.length > pngFrameCount) {
    throw new Error("semantic and independently observed PNG frame counts differ");
  }
  let previousFrameTime = -Infinity;
  for (const [index, time] of frameTimes.entries()) {
    if (!Number.isFinite(time) || time < 0 || time + 1e-12 < previousFrameTime
        || time > logicalDuration + 1e-9) {
      throw new Error(`invalid or out-of-range logical time for reference frame ${index}`);
    }
    previousFrameTime = time;
  }
  if (sampleTimes !== undefined && (!Array.isArray(sampleTimes) || sampleTimes.length === 0
      || sampleTimes.some((time, index) => !Number.isFinite(time) || time < 0
        || (index > 0 && time <= sampleTimes[index - 1])))) {
    throw new Error("sample_times must be finite, non-negative and strictly increasing");
  }
  if (frameTimes.length === 0) {
    if (pngFrameCount !== 1 || logicalDuration !== 0 || !terminalState
        || terminalState.time !== 0) {
      throw new Error("empty semantic timeline requires one PNG and a zero-duration terminal state");
    }
    const selected = sampleTimes === undefined
      ? sampleRasterFrames([0], sampleFractions)
      : sampleRasterFrames([0], [], sampleTimes);
    return selected.map(sample => ({ ...sample, requestedTime: sample.time,
      materializedTime: 0, terminalState: true }));
  }
  if (frameTimes.length !== pngFrameCount) {
    throw new Error("semantic and independently observed PNG frame counts differ");
  }
  if (sampleTimes === undefined) {
    return sampleRasterFrames(frameTimes, sampleFractions).map(sample => ({ ...sample,
      requestedTime: sample.time, materializedTime: sample.time, terminalState: false }));
  }
  return sampleTimes.map(requestedTime => {
    const exact = frameTimes.findIndex(time => Math.abs(time - requestedTime) <= 1e-9);
    if (exact >= 0) return { frameIndex: exact, time: requestedTime, requestedTime,
      materializedTime: frameTimes[exact], terminalState: false,
      label: `frame-${String(exact).padStart(4, "0")}` };
    if (Math.abs(requestedTime - logicalDuration) <= 1e-9
        && terminalState && typeof terminalState.time === "number"
        && Math.abs(terminalState.time - logicalDuration) <= 1e-9) {
      const lastIndex = frameTimes.length - 1;
      // The terminal comparison uses observed scene state, not time alone.
      if (Array.isArray(semanticFrames) && semanticFrames[lastIndex]
          && isDeepStrictEqual(visualState(semanticFrames[lastIndex]), visualState(terminalState))) {
        return { frameIndex: lastIndex, time: requestedTime, requestedTime,
          materializedTime: frameTimes[lastIndex], terminalState: true,
          label: `frame-${String(lastIndex).padStart(4, "0")}-terminal` };
      }
    }
    throw new Error(`no reference frame at requested logical time ${requestedTime}`);
  });
}

export function rasterFixtureSource(source, scene, { requires_latex = false } = {}) {
  const adapted = source.replace("from manim import *", "from noon import *");
  const prepared = requires_latex
    ? `from noon import prepare_latex\nawait prepare_latex()\n${adapted}`
    : adapted;
  // Selection is host bootstrap. The normal source runner owns construct and
  // continuation; authored semantics and callbacks remain unchanged.
  return `${prepared}\nfor _name, _cls in tuple(globals().items()):\n    if isinstance(_cls, type) and issubclass(_cls, Scene) and _cls is not ${scene}:\n        _cls.__module__ = "raster_fixture_library"\ndel _cls\n`;
}

// Automatic browser selection becomes a strict expectation after the first
// render; paired hosts must still use exactly the same supported backend.
export function resolveQualifiedBackend(requestedBackend, actualBackend) {
  const supportedBackends = ["WebGPU", "WebGL2"];
  if (requestedBackend === "automatic") {
    if (!supportedBackends.includes(actualBackend)) {
      throw new Error(`automatic renderer selected unsupported backend ${actualBackend}`);
    }
    return actualBackend;
  }
  if (!supportedBackends.includes(requestedBackend)) {
    throw new Error(`unsupported expected renderer backend ${requestedBackend}`);
  }
  if (actualBackend !== requestedBackend) {
    throw new Error(`renderer selected ${actualBackend}; expected ${requestedBackend}`);
  }
  return requestedBackend;
}

export function browserArgs(backend, { gpuMode = "software" } = {}) {
  if (!new Set(["software", "hardware"]).has(gpuMode)) {
    throw new Error(`unsupported GPU mode: ${gpuMode}`);
  }
  if (backend === "webgpu") {
    if (gpuMode === "hardware") {
      return [
        "--enable-unsafe-webgpu",
        "--use-gpu-in-tests",
        "--ignore-gpu-blocklist",
        "--enable-accelerated-2d-canvas",
        "--use-webgpu-power-preference=default-high-performance",
        "--force-high-performance-gpu",
        "--disable-gpu-sandbox",
        "--disable-dev-shm-usage",
      ];
    }
    return [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  if (gpuMode === "hardware") {
    return [
      "--disable-features=WebGPU",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--force-high-performance-gpu",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ];
  }
  return [
    "--disable-features=WebGPU",
    "--enable-unsafe-swiftshader",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=swiftshader",
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

// Infer the canvas background from the modal exact RGBA value across the image.
// A border-only sample is unreliable when a grid or perimeter path covers it.
export function dominantImageRgba({ data, width, height }) {
  if (!Number.isInteger(width) || width < 1 || !Number.isInteger(height) || height < 1
      || !data || data.length !== width * height * 4) {
    throw new Error("expected RGBA image data with positive integer dimensions");
  }
  const counts = new Map();
  let selected = 0;
  let maximumCount = 0;
  for (let offset = 0; offset < data.length; offset += 4) {
    const key = ((data[offset] << 24) | (data[offset + 1] << 16)
      | (data[offset + 2] << 8) | data[offset + 3]) >>> 0;
    const count = (counts.get(key) ?? 0) + 1;
    counts.set(key, count);
    if (count > maximumCount) {
      selected = key;
      maximumCount = count;
    }
  }
  return [selected >>> 24, (selected >>> 16) & 255, (selected >>> 8) & 255, selected & 255];
}

export function isIdentifiedGpuAdapter(info) {
  if (!info || typeof info !== "object") return false;
  return [info.vendor, info.architecture, info.device, info.description]
    .some((value) => typeof value === "string" && value.trim() !== "")
    || typeof info.isFallbackAdapter === "boolean";
}

export function isSoftwareGpuAdapter(info) {
  if (!info || typeof info !== "object") return false;
  if (info.isFallbackAdapter === true) return true;
  const description = [info.vendor, info.architecture, info.device, info.description]
    .filter((value) => typeof value === "string")
    .join(" ")
    .toLowerCase();
  return ["swiftshader", "llvmpipe", "software rasterizer", "software adapter"]
    .some((needle) => description.includes(needle));
}

// Runtime geometry uses f32. Absolute shared-property callbacks can round by a
// fraction of a micro-unit as sample cadence changes. Identities, shape, order
// and scalar values beyond this fixed numerical bound must still agree.
export const MAX_EFFECTIVE_ABSOLUTE_ERROR = 1e-6;

export function compareEffectiveFrames(actual, expected) {
  let maximumAbsoluteError = 0;
  function compare(left, right, key) {
    if (Object.is(left, right)) return;
    if (typeof left === "number" && typeof right === "number") {
      const error = Math.abs(left - right);
      if (!Number.isFinite(error) || error > MAX_EFFECTIVE_ABSOLUTE_ERROR) {
        throw new Error(`${key}: effective value differs (${left} vs ${right})`);
      }
      maximumAbsoluteError = Math.max(maximumAbsoluteError, error);
      return;
    }
    if (left === null || right === null || typeof left !== "object" || typeof right !== "object"
        || Array.isArray(left) !== Array.isArray(right)) {
      throw new Error(`${key}: effective shape or value differs`);
    }
    const keys = Object.keys(left);
    if (keys.length !== Object.keys(right).length || keys.some((k) => !Object.hasOwn(right, k))) {
      throw new Error(`${key}: effective fields differ`);
    }
    for (const field of keys) compare(left[field], right[field], `${key}.${field}`);
  }
  compare(actual, expected, "frame");
  return maximumAbsoluteError;
}

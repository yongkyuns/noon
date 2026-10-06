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

// Resolve logical samples against observed Cairo images. Interior static holds
// require an explicit frozen interval; changed endpoints require a separately
// captured terminal PNG. Reuse of a final sequence PNG requires exact terminal
// visual equality. Zero-duration scenes retain an empty semantic timeline.
export function resolveRasterReferenceSamples(frameTimes, sampleTimes, {
  logicalDuration, terminalState, terminalPng, frozenIntervals = [], pngFrameCount,
  semanticFrames, sampleFractions,
}) {
  if (!Array.isArray(frameTimes) || !Number.isFinite(logicalDuration) || logicalDuration < 0
      || !Number.isInteger(pngFrameCount) || pngFrameCount < 0) {
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
    const emptyTerminalOnly = pngFrameCount === 0 && Boolean(terminalPng);
    if ((!emptyTerminalOnly && pngFrameCount !== 1) || logicalDuration !== 0 || !terminalState
        || terminalState.time !== 0) {
      throw new Error("empty semantic timeline requires a zero-duration terminal and observed Cairo image");
    }
    const selected = sampleTimes === undefined
      ? sampleRasterFrames([0], sampleFractions)
      : sampleRasterFrames([0], [], sampleTimes);
    return selected.map((sample) => terminalPng && (sampleTimes !== undefined || pngFrameCount === 0)
      ? { ...sample, frameIndex: null, requestedTime: sample.time, materializedTime: 0,
        terminalState: true, referenceKind: "terminal", label: "frame-0000-terminal" }
      : { ...sample, requestedTime: sample.time, materializedTime: 0,
        terminalState: false, referenceKind: "sequence" });
  }
  if (frameTimes.length !== pngFrameCount) {
    throw new Error("semantic and independently observed PNG frame counts differ");
  }
  if (sampleTimes === undefined) {
    return sampleRasterFrames(frameTimes, sampleFractions).map(sample => ({ ...sample,
      requestedTime: sample.time, materializedTime: sample.time, terminalState: false,
      referenceKind: "sequence" }));
  }
  const intervals = frozenIntervals.map((interval, intervalIndex) => {
    const frameIndex = interval?.frame_index;
    const start = interval?.start_time;
    const end = interval?.end_time;
    if (!Number.isSafeInteger(frameIndex) || frameIndex < 0 || frameIndex >= frameTimes.length
        || !Number.isFinite(start) || !Number.isFinite(end) || start < 0
        || end < start || end > logicalDuration + 1e-9
        || Math.abs(frameTimes[frameIndex] - start) > 1e-9) {
      throw new Error(`invalid frozen interval ${intervalIndex}`);
    }
    return { frameIndex, start, end };
  });
  return sampleTimes.map(requestedTime => {
    if (Math.abs(requestedTime - logicalDuration) <= 1e-9 && terminalPng) {
      if (typeof terminalState?.time !== "number" || !Number.isFinite(terminalState.time)
          || Math.abs(terminalState.time - logicalDuration) > 1e-9) {
        throw new Error("terminal PNG requires a terminal semantic state at the authored endpoint");
      }
      const lastIndex = frameTimes.length - 1;
      return { frameIndex: null, time: requestedTime, requestedTime,
        materializedTime: logicalDuration, terminalState: true, referenceKind: "terminal",
        label: `frame-${String(lastIndex).padStart(4, "0")}-terminal` };
    }
    const exact = frameTimes.findIndex(time => Math.abs(time - requestedTime) <= 1e-9);
    if (exact >= 0) return { frameIndex: exact, time: requestedTime, requestedTime,
      materializedTime: frameTimes[exact], terminalState: false, referenceKind: "sequence",
      label: `frame-${String(exact).padStart(4, "0")}` };
    const covering = intervals.filter(interval => requestedTime >= interval.start
      && requestedTime < interval.end);
    if (covering.length === 1) {
      const { frameIndex } = covering[0];
      return { frameIndex, time: requestedTime, requestedTime,
        materializedTime: frameTimes[frameIndex], terminalState: false,
        referenceKind: "frozen-hold",
        label: `frame-${String(frameIndex).padStart(4, "0")}-hold-${String(requestedTime).replace(/[^0-9a-z]/gi, "_")}` };
    }
    if (covering.length > 1) throw new Error(`overlapping frozen holds at logical time ${requestedTime}`);
    if (Math.abs(requestedTime - logicalDuration) <= 1e-9
        && terminalState && typeof terminalState.time === "number"
        && Number.isFinite(terminalState.time)
        && Math.abs(terminalState.time - logicalDuration) <= 1e-9) {
      const lastIndex = frameTimes.length - 1;
      // The terminal comparison uses observed scene state, not time alone.
      if (Array.isArray(semanticFrames) && semanticFrames[lastIndex]
          && isDeepStrictEqual(visualState(semanticFrames[lastIndex]), visualState(terminalState))) {
        return { frameIndex: lastIndex, time: requestedTime, requestedTime,
          materializedTime: frameTimes[lastIndex], terminalState: true, referenceKind: "sequence",
          label: `frame-${String(lastIndex).padStart(4, "0")}-terminal` };
      }
    }
    throw new Error(`no reference frame at requested logical time ${requestedTime}`);
  });
}

// A clean retained canvas needs no new draw. An idle snapshot may be sampled by
// seek only when the independent reference proves a frozen hold at that time.
// Continuous animation and unexplained stalls must remain qualification failures.
export function directStaticObservation(metrics, requestedTime, referenceKind) {
  if (metrics.presented || !Number.isFinite(metrics.time) || !Number.isFinite(requestedTime)
      || requestedTime < 0 || metrics.time < 0 || metrics.wake?.presentNow !== false) return null;
  const { cadence, delayMs } = metrics.wake;
  const timer = cadence === "timer" && Number.isFinite(delayMs) && delayMs > 0;
  if (!timer && cadence !== "idle") return null;
  if (Math.abs(metrics.time - requestedTime) <= 1e-9) return "retained";
  if (metrics.time < requestedTime && (timer || referenceKind === "frozen-hold")) return "seek";
  return null;
}

export function selectDirectReplayCapture(captures, duration) {
  if (!Array.isArray(captures) || !Number.isFinite(duration) || duration <= 0) return null;
  const interior = captures.filter(capture => Number.isFinite(capture.time)
    && capture.time > 0 && capture.time < duration);
  const nearest = candidates => candidates.sort((left, right) =>
    Math.abs(left.time - duration / 2) - Math.abs(right.time - duration / 2)
    || left.time - right.time)[0] ?? null;
  const forward = nearest(interior.filter(capture => capture.observationMode === "forward"));
  if (forward) return { capture: forward, replayMode: "direct-seek-replay" };
  if (interior.length > 0 && interior.every(capture => capture.observationMode === "static-hold-seek")) {
    return { capture: nearest(interior), replayMode: "static-hold-observation-repeat" };
  }
  return null;
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

export function classifyBrowserGpuDiagnostics(diagnostics) {
  if (diagnostics?.api === "webgpu") {
    const adapter = diagnostics.adapter;
    if (!adapter || !isIdentifiedGpuAdapter(adapter)) return "unknown";
    return isSoftwareGpuAdapter(adapter) ? "software" : "hardware-like-unverified";
  }
  const rendererInfo = {
    vendor: String(diagnostics?.vendor ?? ""),
    device: String(diagnostics?.renderer ?? ""),
  };
  if (![rendererInfo.vendor, rendererInfo.device].some((value) => value.trim() !== "")) return "unknown";
  return isSoftwareGpuAdapter(rendererInfo) ? "software" : "hardware-like-unverified";
}

// Classify only identity read from the renderer's actual WGPU device. The
// descriptor can distinguish known software paths from an identified adapter,
// but it cannot certify a physical display or prove hardware provenance.
export function classifyRendererGpuIdentity(info, expectedBackend) {
  if (!info || typeof info !== "object" || !["webgpu", "webgl"].includes(expectedBackend)) {
    return "unknown";
  }
  const expected = expectedBackend === "webgpu" ? "BrowserWebGpu" : "Gl";
  if (info.backend !== expected) return "unknown";
  const descriptor = {
    vendor: Number(info.vendor) > 0 ? String(info.vendor) : "",
    device: Number(info.device) > 0 ? String(info.device) : "",
    description: [info.name, info.driver, info.driverInfo]
      .filter((value) => typeof value === "string").join(" "),
    architecture: String(info.deviceType ?? ""),
    driver: String(info.driver ?? ""),
    driverInfo: String(info.driverInfo ?? ""),
  };
  const hasIdentity = [descriptor.vendor, descriptor.device, descriptor.description]
    .some((value) => value.trim() !== "");
  if (String(info.deviceType ?? "").toLowerCase() === "cpu") return "software";
  if (!hasIdentity) return "unknown";
  if (isSoftwareGpuAdapter(descriptor)) {
    return "software";
  }
  return "hardware-like-unverified";
}

export function validateRendererGpuMode(mode, identity, expectedBackend) {
  if (mode === null || mode === undefined) return null;
  if (!["software", "hardware"].includes(mode)) {
    throw new Error(`NOON_CORPUS_GPU_MODE must be hardware or software`);
  }
  const classification = classifyRendererGpuIdentity(identity, expectedBackend);
  if (classification === "unknown") {
    throw new Error(`renderer GPU identity is unknown for explicit ${mode} qualification`);
  }
  if (mode === "hardware" && classification !== "hardware-like-unverified") {
    throw new Error("renderer selected a known software adapter for hardware qualification");
  }
  if (mode === "software" && classification !== "software") {
    throw new Error("renderer did not select an identified software adapter for software qualification");
  }
  return classification;
}

export function rendererGpuQualification(mode, identity, expectedBackend) {
  const classification = classifyRendererGpuIdentity(identity, expectedBackend);
  try {
    validateRendererGpuMode(mode, identity, expectedBackend);
    return { mode, passed: true, classification, error: null };
  } catch (error) {
    return { mode, passed: false, classification, error: String(error?.message ?? error) };
  }
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

export function rasterFixtureSource(source, scene) {
  const adapted = source.replace("from manim import *", "from noon import *");
  // Selection is host bootstrap. The normal source runner owns construct and
  // continuation; authored semantics and callbacks remain unchanged.
  return `${adapted}\nfor _name, _cls in tuple(globals().items()):\n    if isinstance(_cls, type) and issubclass(_cls, Scene) and _cls is not ${scene}:\n        _cls.__module__ = "raster_fixture_library"\ndel _cls\n`;
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

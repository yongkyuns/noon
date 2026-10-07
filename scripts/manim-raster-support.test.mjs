import assert from "node:assert/strict";
import test from "node:test";
import {
  compareEffectiveFrames,
  compareEffective3DCamera,
  classifyBrowserGpuDiagnostics,
  classifyRendererGpuIdentity,
  dominantImageRgba,
  rasterFixtureSource,
  resolveQualifiedBackend,
  rendererGpuQualification,
  validateRendererGpuMode,
} from "./manim-raster-support.mjs";

const frame = { objects: [{ id: 7, present: true, transform: { y: 3.9542133808135986 }, opacity: 0.5 }] };

test("automatic paired qualification records either actual supported backend", () => {
  assert.equal(resolveQualifiedBackend("automatic", "WebGPU"), "WebGPU");
  assert.equal(resolveQualifiedBackend("automatic", "WebGL2"), "WebGL2");
});

test("explicit paired qualification retains strict backend matching", () => {
  assert.equal(resolveQualifiedBackend("WebGL2", "WebGL2"), "WebGL2");
  assert.throws(() => resolveQualifiedBackend("WebGL2", "WebGPU"), /expected WebGL2/);
  assert.throws(() => resolveQualifiedBackend("automatic", "Other"), /unsupported backend/);
  assert.throws(() => resolveQualifiedBackend("Other", "WebGPU"), /unsupported expected/);
});

test("renderer GPU qualification rejects unknown, mismatched, and software identity", () => {
  const hardware = { backend: "BrowserWebGpu", vendor: 1452, device: 1,
    name: "Apple M4", deviceType: "IntegratedGpu", driver: "Metal", driverInfo: "" };
  const software = { ...hardware, name: "SwiftShader Device (Subzero)" };
  const unknown = { backend: "BrowserWebGpu", vendor: 0, device: 0,
    name: "", deviceType: "Unknown", driver: "", driverInfo: "" };
  const emptyOtherType = { ...unknown, deviceType: "Other" };
  assert.equal(classifyRendererGpuIdentity(hardware, "webgpu"), "hardware-like-unverified");
  assert.equal(validateRendererGpuMode("hardware", hardware, "webgpu"), "hardware-like-unverified");
  assert.equal(classifyRendererGpuIdentity(software, "webgpu"), "software");
  assert.equal(validateRendererGpuMode("software", software, "webgpu"), "software");
  assert.throws(() => validateRendererGpuMode("hardware", software, "webgpu"), /known software/);
  assert.throws(() => validateRendererGpuMode("hardware", unknown, "webgpu"), /identity is unknown/);
  assert.equal(classifyRendererGpuIdentity(emptyOtherType, "webgpu"), "unknown");
  assert.throws(() => validateRendererGpuMode("hardware", emptyOtherType, "webgpu"), /identity is unknown/);
  assert.throws(() => validateRendererGpuMode("software", hardware, "webgpu"), /did not select/);
  assert.throws(() => validateRendererGpuMode("hardware", hardware, "webgl"), /identity is unknown/);
  assert.deepEqual(rendererGpuQualification("hardware", software, "webgpu"), {
    mode: "hardware", passed: false, classification: "software",
    error: "renderer selected a known software adapter for hardware qualification",
  });
  assert.deepEqual(rendererGpuQualification("hardware", unknown, "webgpu"), {
    mode: "hardware", passed: false, classification: "unknown",
    error: "renderer GPU identity is unknown for explicit hardware qualification",
  });
});

test("default corpus browser diagnostics retain their standalone classifier path", () => {
  assert.equal(classifyBrowserGpuDiagnostics({ api: "webgpu", adapter: {
    vendor: "Apple", architecture: "Apple GPU", device: "M4", description: "Apple M4",
  } }), "hardware-like-unverified");
  assert.equal(classifyBrowserGpuDiagnostics({ api: "webgpu", adapter: {
    vendor: "Google", architecture: "SwiftShader", device: "", description: "SwiftShader Device",
  } }), "software");
  assert.equal(classifyBrowserGpuDiagnostics({ api: "webgl2", vendor: null, renderer: null }), "unknown");
});

test("full-image background estimate ignores a colored perimeter", () => {
  const width = 7;
  const height = 7;
  const background = [12, 34, 56, 255];
  const perimeter = [180, 40, 220, 255];
  const data = Buffer.alloc(width * height * 4);
  for (let offset = 0; offset < data.length; offset += 4) {
    data.set(background, offset);
  }
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (x === 0 || y === 0 || x === width - 1 || y === height - 1) {
        data.set(perimeter, (y * width + x) * 4);
      }
    }
  }
  assert.deepEqual(dominantImageRgba({ data, width, height }), background);
});

test("raster fixture adaptation leaves default sources without LaTeX preparation", () => {
  const source = "from manim import *\nclass Example(Scene):\n    pass\n";
  const adapted = rasterFixtureSource(source, "Example");
  assert.equal(adapted.match(/prepare_latex/g), null);
  assert.match(adapted, /^from noon import \*/);
});

test("raster fixture adaptation prepares LaTeX exactly once when requested", () => {
  const source = "from manim import *\nclass Example(Scene):\n    pass\n";
  const adapted = rasterFixtureSource(source, "Example", { requires_latex: true });
  assert.equal(adapted.match(/from noon import prepare_latex/g)?.length, 1);
  assert.equal(adapted.match(/await prepare_latex\(\)/g)?.length, 1);
  assert.match(adapted, /^from noon import prepare_latex\nawait prepare_latex\(\)\nfrom noon import \*/);
});

test("effective comparison accepts f32 rounding and reports the actual error", () => {
  const actual = structuredClone(frame);
  actual.objects[0].transform.y = 3.9542136192321777;
  assert.equal(compareEffectiveFrames(actual, frame), 2.384185791015625e-7);
  assert.equal(compareEffectiveFrames(frame, structuredClone(frame)), 0);
});

test("effective comparison rejects changed properties and non-finite values", () => {
  for (const value of [0.501, NaN, Infinity]) {
    const actual = structuredClone(frame);
    actual.objects[0].opacity = value;
    assert.throws(() => compareEffectiveFrames(actual, frame), /effective value differs/);
  }
});

test("effective comparison preserves identity, presence and object structure", () => {
  for (const mutate of [
    (f) => { f.objects[0].id += 1; },
    (f) => { f.objects[0].present = false; },
    (f) => { delete f.objects[0].opacity; },
    (f) => { f.objects.push(structuredClone(f.objects[0])); },
    (f) => { f.objects = { 0: f.objects[0] }; },
  ]) {
    const actual = structuredClone(frame);
    mutate(actual);
    assert.throws(() => compareEffectiveFrames(actual, frame));
  }
});

test("effective 3D camera compares every profile field and observes point light", () => {
  const expected = {
    phi: 0.8, theta: -0.1, gamma: 0.2, focal_distance: 5, zoom: 1.1,
    frame_height: 8, frame_center: [0.3, 0, 0], light_source: [-6, -8, 9],
  };
  const profile = { ...expected, near: 0.1, far: 100 };
  delete profile.light_source;
  const debug = { objects: [
    { present: true, spatial: { camera_profile: profile } },
    { present: true, spatial: { point_light: true, translation: [-6, -8, 9] } },
  ] };
  assert.deepEqual(compareEffective3DCamera(debug, expected), {
    maximumAbsoluteError: 0, lightObserved: true,
  });
  const withinTolerance = structuredClone(debug);
  withinTolerance.objects[0].spatial.camera_profile.theta += 5e-7;
  assert.ok(Math.abs(compareEffective3DCamera(withinTolerance, expected).maximumAbsoluteError - 5e-7) < 1e-12);
  for (const mutate of [
    frame => { frame.objects[0].spatial.camera_profile.zoom = 1.10001; },
    frame => { frame.objects[0].spatial.camera_profile.phi = NaN; },
    frame => { frame.objects[0].spatial.camera_profile.frame_center.pop(); },
    frame => { frame.objects[0].spatial.camera_profile.near = Infinity; },
    frame => { frame.objects[0].spatial.camera_profile.far = 0.05; },
    frame => { frame.objects[1].spatial.translation[0] = -5; },
  ]) {
    const bad = structuredClone(debug);
    mutate(bad);
    assert.throws(() => compareEffective3DCamera(bad, expected));
  }
});

test("effective 3D camera permits an absent light row only at Manim's pinned default", () => {
  const expected = {
    phi: 0.6, theta: -1.2, gamma: 0, focal_distance: 5, zoom: 1,
    frame_height: 8, frame_center: [0, 0, 0], light_source: [-7, -9, 10],
  };
  const debug = { objects: [{ present: true, spatial: {
    camera_profile: { ...expected, near: 0.1, far: 100 },
  } }] };
  delete debug.objects[0].spatial.camera_profile.light_source;
  assert.deepEqual(compareEffective3DCamera(debug, expected), {
    maximumAbsoluteError: 0, lightObserved: false,
  });
  assert.throws(() => compareEffective3DCamera(debug, {
    ...expected, light_source: [-6, -8, 9],
  }), /effective value differs/);
  for (const invalid of [
    { ...expected, frame_height: Infinity },
    { ...expected, frame_center: [0, NaN, 0] },
    { ...expected, light_source: [0, 0] },
  ]) assert.throws(() => compareEffective3DCamera(debug, invalid));
  assert.throws(() => compareEffective3DCamera({ objects: [] }, expected), /expected one present/);
});

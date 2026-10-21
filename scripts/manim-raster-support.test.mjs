import assert from "node:assert/strict";
import test from "node:test";
import {
  authoredCameraReference,
  browserArgs,
  compareEffectiveFrames,
  compareEffective3DCamera,
  compareEffectiveInsetCameras,
  classifyBrowserGpuDiagnostics,
  classifyRendererGpuIdentity,
  dominantImageRgba,
  rasterFixtureSource,
  rasterReferenceEnvironment,
  resolveQualifiedBackend,
  rendererGpuQualification,
  validateRendererGpuMode,
} from "./manim-raster-support.mjs";

const frame = { objects: [{ id: 7, present: true, transform: { y: 3.9542133808135986 }, opacity: 0.5 }] };

test("reference default font comes only from its recorded manifest", () => {
  const environment = { RETAINED: "value", NOON_MANIM_REFERENCE_TEXT_FONT: "Ambient font" };
  assert.deepEqual(rasterReferenceEnvironment({}, environment), { RETAINED: "value" });
  assert.deepEqual(rasterReferenceEnvironment({ text_font: "DejaVu Sans Mono" }, environment), {
    RETAINED: "value", NOON_MANIM_REFERENCE_TEXT_FONT: "DejaVu Sans Mono",
  });
  assert.equal(environment.NOON_MANIM_REFERENCE_TEXT_FONT, "Ambient font");
});

test("invalid reference font configuration fails before starting an oracle", () => {
  for (const text_font of [null, 1, "", " ", " DejaVu Sans Mono", "DejaVu Sans Mono "]) {
    assert.throws(() => rasterReferenceEnvironment({ text_font }, {}), /explicit font family/);
  }
});

test("explicit Pango backend overrides ambient backend without changing original observations", () => {
  const environment = { PANGOCAIRO_BACKEND: "coretext" };
  assert.equal(rasterReferenceEnvironment({}, environment).PANGOCAIRO_BACKEND, "coretext");
  assert.equal(rasterReferenceEnvironment({ pango_backend: "fc" }, environment).PANGOCAIRO_BACKEND, "fc");
  for (const pango_backend of [null, "", "auto", " fc", 1]) {
    assert.throws(() => rasterReferenceEnvironment({ pango_backend }, environment), /pango_backend/);
  }
});

function authoredCameraInputs() {
  const camera = { phi: 0.7, theta: -0.2, gamma: 0, zoom: 1, frame_center: [0, 0, 0] };
  const fixture = { sample_times: [1], camera_profile_observation: true,
    authored_camera_checkpoints: [{ time: 1, phi: 0.7, theta: -0.1 }] };
  const sample = { requestedTime: 1, materializedTime: 1, camera3dOracle: camera };
  const semantic = { authored_camera_samples: [{ time: 1, sampled_time: 1,
    camera_3d: { ...camera, theta: -0.1 }, sampled_manim_camera_3d: camera,
    path: "checkpoint.png", sha256: "a".repeat(64) }] };
  return { fixture, sample, semantic };
}

test("authored camera target preserves original state and changes only explicit angles", () => {
  const { fixture, sample, semantic } = authoredCameraInputs();
  const before = structuredClone({ fixture, sample, semantic });
  assert.equal(authoredCameraReference(fixture, sample, semantic), semantic.authored_camera_samples[0]);
  assert.deepEqual({ fixture, sample, semantic }, before);
  assert.equal(authoredCameraReference({}, sample, semantic), null);
});

test("authored camera reference rejects missing coverage, wrong time, or unrelated state changes", () => {
  for (const mutate of [
    input => { input.semantic.authored_camera_samples = []; },
    input => { input.fixture.sample_times.push(2); },
    input => { input.sample.requestedTime = 2; },
    input => { input.semantic.authored_camera_samples[0].sampled_time = 0.99; },
    input => { input.semantic.authored_camera_samples[0].camera_3d.zoom = 2; },
    input => { input.semantic.authored_camera_samples[0].sampled_manim_camera_3d = {}; },
    input => { input.semantic.authored_camera_samples[0].sha256 = "unverified"; },
  ]) {
    const input = authoredCameraInputs();
    mutate(input);
    assert.throws(() => authoredCameraReference(input.fixture, input.sample, input.semantic),
      /authored camera reference/);
  }
});

test("hardware WebGL disables Chromium WebGPU services without forcing software rendering", () => {
  const args = browserArgs("webgl", { gpuMode: "hardware" });
  const disabled = new Set(args.filter(arg => arg.startsWith("--disable-features="))
    .flatMap(arg => arg.slice("--disable-features=".length).split(",")));
  for (const feature of ["WebGPU", "WebGPUService", "WebGPUBlobCache"]) {
    assert.ok(disabled.has(feature), `${feature} must be disabled to select WebGL`);
  }
  assert.ok(args.every(arg => !arg.includes("swiftshader")));
});

test("automatic paired qualification records either actual supported backend", () => {
  assert.equal(resolveQualifiedBackend("automatic", "WebGPU"), "WebGPU");
  assert.equal(resolveQualifiedBackend("automatic", "WebGL2"), "WebGL2");
});

test("same-device browser identity can identify a redacted WebGPU description", () => {
  const redacted = { backend: "BrowserWebGpu", vendor: 0, device: 0,
    name: "", deviceType: "Other", driver: "", driverInfo: "" };
  const info = { ...redacted, browserDeviceInfo: {
    vendor: "amd", architecture: "rdna-1", device: "", description: "",
    isFallbackAdapter: false,
  } };
  assert.equal(validateRendererGpuMode("hardware", info, "webgpu"), "hardware-like-unverified");
  assert.equal(classifyRendererGpuIdentity(info, "webgl"), "unknown");
  assert.equal(classifyRendererGpuIdentity({ ...info, backend: "Gl" }, "webgl"), "unknown");
  const empty = { ...info, browserDeviceInfo: { vendor: "", architecture: "",
    device: "", description: "", isFallbackAdapter: false } };
  assert.equal(classifyRendererGpuIdentity(empty, "webgpu"), "unknown");
  assert.equal(classifyRendererGpuIdentity({ ...empty, browserDeviceInfo: {
    ...empty.browserDeviceInfo, vendor: "Unknown", device: "n/a", description: "Other",
  } }, "webgpu"), "unknown");
  assert.equal(classifyRendererGpuIdentity({ ...redacted, browserDeviceInfo: {
    vendor: null, device: 0, description: false,
  } }, "webgpu"), "unknown");
});

test("browser fallback and software architecture retain software classification", () => {
  const info = { backend: "BrowserWebGpu", vendor: 0, device: 0,
    name: "", deviceType: "Other", driver: "", driverInfo: "",
    browserDeviceInfo: { vendor: "google", architecture: "swiftshader",
      device: "", description: "", isFallbackAdapter: false } };
  assert.equal(classifyRendererGpuIdentity(info, "webgpu"), "software");
  assert.throws(() => validateRendererGpuMode("hardware", info, "webgpu"), /known software/);
  const fallback = { ...info, browserDeviceInfo: { ...info.browserDeviceInfo,
    vendor: "amd", architecture: "rdna-1", isFallbackAdapter: true } };
  assert.equal(classifyRendererGpuIdentity(fallback, "webgpu"), "software");
  assert.equal(validateRendererGpuMode("software", fallback, "webgpu"), "software");
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

test("effective 3D camera reports all numeric field mismatches in one diagnostic", () => {
  const expected = {
    phi: 1.1090003497949128, theta: 0.5252506208689597, gamma: 0.2,
    focal_distance: 5, zoom: 1, frame_height: 8,
    frame_center: [0.000003, -0.000004, 0], light_source: [-6, -8, 10],
  };
  const debug = { objects: [
    { present: true, spatial: { camera_profile: {
      phi: 1.108996938995747, theta: 0.5235987755982988, gamma: 0.2 + 5e-7,
      focal_distance: 5, zoom: 1, frame_height: 8, frame_center: [0, 0, 0],
      near: 0.1, far: 100,
    } } },
    { present: true, spatial: { point_light: true, translation: [-7, -9, 10 + 5e-7] } },
  ] };

  assert.throws(() => compareEffective3DCamera(debug, expected), error => {
    assert.match(error.message, /(?:^|; )frame\.phi: effective value differs/);
    assert.match(error.message, /(?:^|; )frame\.theta: effective value differs/);
    assert.match(error.message, /(?:^|; )frame\.frame_center\[0\]: effective value differs/);
    assert.match(error.message, /(?:^|; )frame\.frame_center\[1\]: effective value differs/);
    assert.match(error.message, /(?:^|; )frame\.light_source\[0\]: effective value differs/);
    assert.match(error.message, /(?:^|; )frame\.light_source\[1\]: effective value differs/);
    assert.doesNotMatch(error.message, /gamma|frame_center\[2\]|light_source\[2\]/);
    return true;
  });
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

const insetView = () => ({ camera_frame: 4, display: 5,
  camera: { center: { x: 0.5, y: -0.25 }, height: 0.45 },
  display_center: { x: 3, y: 2 }, display_size: { x: 3, y: 1.5 },
  capture_own_display: false });
const insetObservation = () => ({ camera: { center: [0, 0], height: 8 },
  inset_2d_views: [insetView()], objects: [{ id: 4, present: false }, { id: 5, present: true }] });

test("inset comparison requires active camera/display state and preserves the fixed numeric limit", () => {
  const actual = insetObservation();
  const expected = structuredClone(actual);
  delete expected.objects;
  delete expected.inset_2d_views[0].camera_frame;
  delete expected.inset_2d_views[0].display;
  assert.deepEqual(compareEffectiveInsetCameras(actual, expected), {
    maximumAbsoluteError: 0, insetCount: 1, mainCameraObserved: true,
  });
  actual.inset_2d_views[0].camera.center.x += 5e-7;
  assert.ok(compareEffectiveInsetCameras(actual, expected).maximumAbsoluteError <= 1e-6);
  actual.inset_2d_views[0].camera.center.x += 1e-5;
  assert.throws(() => compareEffectiveInsetCameras(actual, expected), /effective value differs/);
  const empty = { camera: expected.camera, inset_2d_views: [], objects: [] };
  assert.equal(compareEffectiveInsetCameras(empty, empty).insetCount, 0);
  assert.throws(() => compareEffectiveInsetCameras(empty, expected));
});

test("inset comparison fails closed on missing views, bad links, invalid geometry and wrong lifecycle", () => {
  const expected = insetObservation();
  for (const mutate of [
    f => { delete f.inset_2d_views; },
    f => { f.inset_2d_views = { error: "invalid display" }; },
    f => { f.inset_2d_views.push(insetView()); },
    f => { f.inset_2d_views[0].camera_frame = 6; },
    f => { f.inset_2d_views[0].camera_frame = 5; },
    f => { f.objects[1].present = false; },
    f => { f.inset_2d_views[0].camera.center.y = NaN; },
    f => { f.inset_2d_views[0].camera.height = Infinity; },
    f => { f.inset_2d_views[0].display_size.x = 0; },
    f => { f.inset_2d_views[0].display_center.y += 0.01; },
    f => { f.inset_2d_views[0].capture_own_display = true; },
    f => { f.camera.height = 0; },
    f => { f.camera.center = [0, 0, 0]; },
  ]) {
    const bad = structuredClone(expected);
    mutate(bad);
    assert.throws(() => compareEffectiveInsetCameras(bad, expected));
  }
  const invalidReference = insetObservation();
  invalidReference.inset_2d_views[0].camera.height = NaN;
  assert.throws(() => compareEffectiveInsetCameras(insetObservation(), invalidReference));
});

// Capability proofs over native Rust builders and real Python workers.
// Surface lighting and text fonts use explicit Noon profiles; these proofs do
// not assert exact Cairo pixels for the untouched upstream reference cases.
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import playwright from "playwright";

const spatial = [
  ["fixed_in_frame_mobject_test", "fixed-frame", 1, [1]],
  ["three_d_camera_rotation", "ambient", 3, [0.5, 2.5, 3]],
  ["three_d_camera_illusion_rotation", "illusion", Math.PI / 2, [0.5, Math.PI / 2]],
  ["three_d_light_source_position", "light", 0, [0]],
  ["three_d_surface_plot", "surface", 0, [0]],
];
const cases = spatial.flatMap(([name, profile, duration, samples]) => samples.map(sampleTime => ({
  id: `${profile}-${sampleTime}`,
  file: `manim_example_${name}.py`,
  factory: "createDirectSpecialCameraSettingsRenderer", factoryArgs: [profile],
  duration, sampleTime, playback: duration > 0 ? "live" : undefined, boundaries: [1, 2],
  // The native host sleeps until the wait deadline after ambient motion stops.
  // Its held frame stays at 2s; Python's explicit sample may evaluate 2.5s.
  directHeldSampleTime: profile === "ambient" && sampleTime === 2.5 ? 2 : undefined,
})));
for (const [name, factory, duration, samples] of [
  ["following_graph_camera", "createDirectFollowingGraphCameraRenderer", 3, [0.5, 1.5, 2.5, 3]],
  ["moving_zoomed_scene_around", "createDirectMovingZoomedSceneAroundRenderer", 12, [0.5, 1.5, 3.5, 9.5, 12]],
]) {
  for (const sampleTime of samples) cases.push({
    id: `${name}-${sampleTime}`, file: `manim_example_${name}.py`,
    factory, duration, sampleTime, playback: "live",
    boundaries: Array.from({length: duration}, (_, index) => index + 1),
  });
}
const browserName = process.env.NOON_CAMERA_BROWSER ?? "chromium";
const noJspi = process.env.NOON_CAMERA_NO_JSPI === "1" || browserName === "webkit";
const contextOptions = browserName === "webkit" ? { ...playwright.devices["iPhone 13"] } : undefined;
const output = process.env.NOON_CAMERA_ARTIFACTS ?? "browser-smoke-artifacts/special-camera";
await qualifyPairedAuthoring({
  cases, prepareContext: noJspi ? disableAuthoringJspi : undefined,
  browserName, contextOptions,
  artifactDirectory: noJspi ? `${output}/no-jspi` : output,
});

import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_ZOOM_ARTIFACTS ?? "zoom-artifacts",
  cases: [0, 0.5, 1].map(sampleTime => ({
    id: `zoomed-scene-${sampleTime}`, file: "zoomed_scene.py",
    factory: "createZoomedSceneRenderer", objectCount: 4, duration: 1, sampleTime,
  })),
});

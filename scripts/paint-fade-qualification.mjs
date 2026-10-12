import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_PAINT_FADE_ARTIFACTS ?? "ci-artifacts/paint-fade",
  cases: [{
    id: "paint-fade",
    sourcePath: "parity/manim-v0.21/core-examples/paint_fade.py",
    scene: "PaintFade",
    factory: "createDirectFamilyPaintSmokeRenderer",
    objectCount: 2,
    duration: 0.2,
    sampleTime: 0.2,
  }],
});

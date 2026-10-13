import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_CUBIC_BEZIER_ARTIFACTS ?? "cubic-bezier-artifacts",
  cases: [{ id: "cubic-bezier", sourcePath: "parity/manim-v0.21/core-examples/cubic_bezier.py",
    scene: "CubicBezierConstructor", factory: "createDirectCubicBezierSmokeRenderer",
    objectCount: 1, duration: 0.2, sampleTime: 0.2, playback: "live" }],
});

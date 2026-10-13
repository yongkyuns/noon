import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_POLYGON_ARTIFACTS ?? "polygon-artifacts",
  cases: [{ id: "polygon-constructors", sourcePath: "parity/manim-v0.21/core-examples/polygon_constructors.py",
    scene: "PolygonConstructors", factory: "createPolygonConstructorsRenderer", objectCount: 6,
    duration: 0.2, sampleTime: 0.2, playback: "live" },
    { id: "angle-constructors", sourcePath: "parity/manim-v0.21/core-examples/angle_constructors.py",
      scene: "AngleConstructors", factory: "createAngleConstructorsRenderer", objectCount: 18,
      duration: 0.2, sampleTime: 0.2, playback: "live" },
    { id: "triangle-options", sourcePath: "parity/manim-v0.21/core-examples/triangle_options.py",
      scene: "TriangleOptions", factory: "createTriangleOptionsRenderer", objectCount: 3,
      duration: 0.2, sampleTime: 0.2, playback: "live" }],
});

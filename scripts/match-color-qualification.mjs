import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_MATCH_COLOR_ARTIFACTS ?? "match-color-artifacts",
  cases: [{
    id: "match-color",
    sourcePath: "web/python/examples/ordinary_paint_queries_gradients.py",
    scene: "PaintQueriesGradients",
    factory: "createDirectPaintQueriesGradientsSmokeRenderer",
    objectCount: 5,
    duration: 0.2,
    sampleTime: 0.2,
    playback: "live",
  }],
});

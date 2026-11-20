// Exact-pixel paired Graph qualification: native direct-WASM and Python use
// the same retained graph leaves, with circular/explicit layouts only.
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_GRAPH_ARTIFACTS ?? "graph-artifacts",
  cases: [0.25, 0.75, 1].map(sampleTime => ({
    id: `graph-${sampleTime}`,
    file: "ordinary_graph.py",
    scene: "OrdinaryGraph",
    factory: "createDirectGraphSmokeRenderer",
    objectCount: 20,
    duration: 1,
    playback: "live",
    sampleTime,
    stopAtSourceCompletion: sampleTime === 1,
  })),
});

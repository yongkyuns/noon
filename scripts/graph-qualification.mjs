// Exact-pixel paired Graph qualification: native direct-WASM and Python use
// the same retained graph leaves, with circular/explicit layouts only.
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_GRAPH_ARTIFACTS ?? "graph-artifacts",
  cases: [{
    id: "graph",
    file: "ordinary_graph.py",
    scene: "OrdinaryGraph",
    factory: "createDirectGraphSmokeRenderer",
    objectCount: 20,
    duration: 0.21,
    sampleTime: 0.21,
  }],
});

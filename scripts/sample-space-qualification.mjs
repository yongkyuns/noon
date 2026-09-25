import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_SAMPLE_SPACE_ARTIFACTS ?? "sample-space-artifacts",
  cases: [{
    id: "sample-space-horizontal-vertical",
    file: "sample_space.py",
    factory: "createSampleSpaceRenderer",
    objectCount: 8,
  }],
});

import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_CONSTRUCTOR_DEFAULTS_ARTIFACTS ?? "constructor-defaults-artifacts",
  cases: [{
    id: "constructor-defaults",
    sourcePath: "web/python/examples/ordinary_live_primitive_construction.py",
    scene: "OrdinaryLivePrimitiveConstruction",
    factory: "createDirectOrdinaryLivePrimitiveConstructionSmokeRenderer",
    objectCount: 3,
    duration: 1,
    sampleTime: 1,
    playback: "live",
  }],
});

// Reuse the pinned source and shared paired harness; no duplicate Python oracle.
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

const variants = [
  ["ordinary", "MatchingOrdinaryForeground", false, 0],
  ["source", "MatchingForegroundSource", true, 2],
  ["layer", "MatchingForegroundOnlyLayer", false, 2],
];
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_FOREGROUND_PAIRED_ARTIFACTS ?? "foreground-paired-artifacts",
  cases: variants.flatMap(([id, scene, foreground, layer]) =>
    [0, 0.5, 1, 1.5, 2, 2.2].map(sampleTime => ({
      id: `${id}-${sampleTime}`, scene,
      sourcePath: "parity/manim-v0.21/core-examples/foreground_matching.py",
      factory: "createDirectForegroundMatchingRenderer", factoryArgs: [foreground, layer],
      playback: "live", boundaries: [2, 2.2], duration: 2 + 0.2 + 0.2, sampleTime,
      objectCount: sampleTime < 2 ? 4 : sampleTime < 2.2 ? 6 : 7,
    }))),
});

import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_POLYNOMIAL_RATE_ARTIFACTS ?? "polynomial-rate-artifacts",
  cases: [{ id: "polynomial-rates", sourcePath: "parity/manim-v0.21/core-examples/polynomial_rates.py",
    scene: "PolynomialRates", factory: "createDirectPolynomialRatesRenderer", objectCount: 3,
    duration: 1, sampleTime: 0.25, playback: "live" }],
});

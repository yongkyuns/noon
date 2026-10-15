import { pathToFileURL } from "node:url";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

export const polynomialRateCases = [{
  id: "polynomial-rates", sourcePath: "parity/manim-v0.21/core-examples/polynomial_rates.py",
  scene: "PolynomialRates", factory: "createDirectPolynomialRatesRenderer", objectCount: 5,
  duration: 1, sampleTime: 0.25, playback: "live",
}];

// The existing constructor entrypoint reuses these fixtures without a second run.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await qualifyPairedAuthoring({
    artifactDirectory: process.env.NOON_POLYNOMIAL_RATE_ARTIFACTS ?? "polynomial-rate-artifacts",
    cases: polynomialRateCases,
  });
}

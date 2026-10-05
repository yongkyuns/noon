import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile(new URL("../scripts/playground-product-e2e.mjs", import.meta.url), "utf8");

assert.match(source, /const measurement = productMeasurement\(exampleId\);/);
assert.match(source, /gallery\?\.executionMode !== null/);
assert.match(source, /void gallery\.executionMetrics\(\)\.catch\(\(\) => \{/);
assert.match(source, /class ObservedWorker extends NativeWorker/);
assert.match(source, /message\?\.channel !== "noon\.render"/);
assert.match(source, /message\?\.type !== "metrics"/);
assert.match(source, /run_time=\$\{durationSeconds\}/);
assert.match(source, /async function synchronizeFinalFrame\(page, seconds\)/);
assert.match(source, /async function waitForRenderedEndpoint\(page, seconds\)/);
assert.match(source, /Math\.abs\(rendered\.time - seconds\) <= 0\.001/);
assert.match(source, /rendered\?\.ready === true/);
assert.match(source, /rendered\.needsPresent === false/);
assert.match(source, /rendered\.bufferedDeltas === 0/);
assert.match(source, /await synchronizeFinalFrame\(page, measurement\.sourceEndSeconds\)/);
assert.match(source, /const screenshotName = "frame-final\.png"/);
assert.match(source, /authoredEndpointSeconds: measurement\.sourceEndSeconds/);
assert.doesNotMatch(source, /frame-0\.5\.png/);
// Counter/clock reset behavior is exercised by playground-product-fps.test.mjs;
// this contract only checks that the real browser harness uses that scorer.
assert.match(source, /sampleRendererFps\(warm\.frameSamples, measurement\.windowEndSeconds/);
assert.match(source, /rendererAt: metrics\?\.sampledAtMs/);
assert.match(source, /clockOriginMs: metrics\?\.performanceTimeOriginMs/);
assert.match(source, /warmupSeconds: measurement\.windowStartSeconds/);
assert.match(source, /samplePresentationGaps\(warm\.presentationSamples, fps\)/);
assert.match(source, /profilePublicationStages: true/);
assert.match(source, /probe\.stageKeys\.size >= 2_000/);
assert.match(source, /locator\("\.canvas-frame"\)\.scrollIntoViewIfNeeded\(\)/);
assert.match(source, /minMeasurementMs: MIN_PRODUCT_MEASUREMENT_MS/);
assert.doesNotMatch(source, /editor\.dispatchEvent/);

console.log("✓ product gate scores a warm renderer window and compares the same authored endpoint");

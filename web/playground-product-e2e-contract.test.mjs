import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile(new URL("../scripts/playground-product-e2e.mjs", import.meta.url), "utf8");

assert.match(source, /const PRODUCT_FIRST_PASS_SECONDS = 3;/);
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
assert.match(source, /void gallery\.executionMetrics\(\)\.catch\(\(\) => \{/);
assert.match(source, /await synchronizeFinalFrame\(page, PRODUCT_FIRST_PASS_SECONDS\)/);
assert.match(source, /const screenshotName = "frame-final\.png"/);
assert.match(source, /authoredEndpointSeconds: PRODUCT_FIRST_PASS_SECONDS/);
assert.doesNotMatch(source, /frame-0\.5\.png/);
// Counter/clock reset behavior is exercised by playground-product-fps.test.mjs;
// this contract only checks that the real browser harness uses that scorer.
assert.match(source, /sampleRendererFps\(cold\.frameSamples, PRODUCT_FIRST_PASS_SECONDS/);
assert.match(source, /minMeasurementMs: MIN_PRODUCT_MEASUREMENT_MS/);
assert.doesNotMatch(source, /editor\.dispatchEvent/);

console.log("✓ product gate observes the same rendered authored endpoint after one long first-pass fixture");

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("../workflows/manim-plotting-qualification.yml", import.meta.url), "utf8");
const section = (job) => source.split(`  ${job}:\n`)[1]?.split(/\n  [a-z]+:\n/)[0];

test("all original plotting cases retain their commands and modes", () => {
  const actual = [...source.matchAll(/- case: ([\w-]+)\n\s+script: ([\w/.-]+)\n\s+mode: '([^']*)'/g)]
    .map((match) => match.slice(1));
  assert.deepEqual(actual, [
    ["plotting", "scripts/plotting-qualification.mjs", ""],
    ["polygons", "scripts/polygon-qualification.mjs", ""],
    ["constructor-defaults", "scripts/constructor-defaults-qualification.mjs", ""],
    ["paint-fade", "scripts/paint-fade-qualification.mjs", ""],
    ["match-color", "scripts/match-color-qualification.mjs", ""],
    ["coordinates", "scripts/time-series-plotting-qualification.mjs", "coordinates"],
    ["number-line", "scripts/time-series-plotting-qualification.mjs", "number-line"],
    ["single", "scripts/time-series-plotting-qualification.mjs", ""],
    ["synchronized", "scripts/time-series-plotting-qualification.mjs", "synchronized"],
    ["gapped", "scripts/time-series-plotting-qualification.mjs", "gapped"],
    ["live", "scripts/time-series-plotting-qualification.mjs", "live"],
  ]);
});

test("every case consumes the authenticated full-validation package", () => {
  const build = section("build"), qualify = section("qualify");
  assert.match(build, /bash scripts\/check\.sh full/);
  assert.ok(build.indexOf("wasm-build.mjs stamp") > build.indexOf("bash scripts/check.sh full"));
  assert.match(build, /wasm-build\.mjs verify/);
  assert.match(qualify, /needs: build/);
  assert.match(qualify, /actions\/download-artifact@v8/);
  assert.match(qualify, /wasm-build\.mjs verify/);
  assert.ok(qualify.indexOf("wasm-build.mjs verify") < qualify.indexOf("npm install"));
  assert.doesNotMatch(qualify, /dev-wasm-build|check\.sh full|continue-on-error/);
  assert.match(source, /fail-fast: false/);
  assert.equal((source.match(/timeout-minutes: 30/g) ?? []).length, 3);
});

test("the original plotting gate requires all cases and retains cancellation evidence", () => {
  const gate = section("plotting"), qualify = section("qualify");
  assert.match(gate, /name: plotting/);
  assert.match(gate, /needs: \[build, qualify\]/);
  assert.match(gate, /if: always\(\)/);
  assert.match(gate, /test "\$BUILD_RESULT" = success/);
  assert.match(gate, /test "\$CASES_RESULT" = success/);
  assert.match(qualify, /Retain every executed case, including cancellation evidence\n\s+if: always\(\)/);
  assert.match(qualify, /plotting-\$\{\{ matrix\.case \}\}-\$\{\{ github\.run_id \}\}-\$\{\{ github\.run_attempt \}\}/);
  assert.match(qualify, /node "\$NOON_CASE_SCRIPT" "\$NOON_CASE_MODE"/);
});

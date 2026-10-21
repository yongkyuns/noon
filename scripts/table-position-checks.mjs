// Real worker position protocols, compared with an independently pinned fixture.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";
import { rasterFixtureSource } from "./manim-raster-support.mjs";

const sha256 = value => createHash("sha256").update(value).digest("hex");

function compare(expected, actual, path = "") {
  if (typeof expected === "number" && typeof actual === "number") {
    const geometric = /\/result\/(center\/\d+|width|height)$/.test(path);
    if (Number.isFinite(actual) && (geometric ? Math.abs(expected - actual) <= 1e-6 : expected === actual)) return [];
  } else if (expected === actual) return [];
  if (Array.isArray(expected) && Array.isArray(actual)) {
    if (expected.length !== actual.length) return [{ path, expectedLength: expected.length, actualLength: actual.length }];
    return expected.flatMap((item, index) => compare(item, actual[index], `${path}/${index}`));
  }
  if (expected && actual && typeof expected === "object" && typeof actual === "object") {
    if (JSON.stringify(Object.keys(expected).sort()) !== JSON.stringify(Object.keys(actual).sort())) {
      return [{ path, expectedKeys: Object.keys(expected), actualKeys: Object.keys(actual) }];
    }
    return Object.keys(expected).flatMap(key => compare(expected[key], actual[key], `${path}/${key}`));
  }
  return [{ path, expected, actual }];
}

export async function qualifyTablePositionProtocols(context, baseUrl, expectedBackend) {
  const generatorUrl = new URL("./manim-table-positions-reference.py", import.meta.url);
  const fixtureUrl = new URL("../fixtures/manim-v0.21/table-positions.json", import.meta.url);
  const generator = await readFile(generatorUrl, "utf8");
  const fixtureBytes = await readFile(fixtureUrl);
  const fixture = JSON.parse(fixtureBytes);
  assert.equal(fixture.manimVersion, "0.21.0");
  assert.equal(sha256(generator), fixture.generatorSha256);
  const start = generator.indexOf("def observe(");
  const end = generator.indexOf("\ndef main():", start);
  assert.ok(start >= 0 && end > start);
  let observe = generator.slice(start, end);
  // Only the oracle's native-test argument serialization is removed. Huge
  // Python operands must cancel before str(), under the product's normal
  // integer-string limit. Query calls, arithmetic, callbacks and observations
  // remain byte-for-byte the pinned probe's implementation.
  const serializationStart = observe.indexOf("    if type(position) in");
  const serializationEnd = observe.indexOf("    python_type = None", serializationStart);
  assert.ok(serializationStart >= 0 && serializationEnd > serializationStart);
  observe = observe.slice(0, serializationStart) + observe.slice(serializationEnd);

  const report = {
    scope: "Table integer/sequence/operator/error/default position protocols",
    fixtureSha256: sha256(fixtureBytes), generatorSha256: fixture.generatorSha256,
    boundsBudget: 1e-6, caseCount: 0, sceneCaseCount: 0, moduleCaseCount: 0, positionDifferences: [], wrapperTypeDifferences: [], groups: [],
  };
  for (const labels of [false, true]) {
    const cases = fixture.cases.filter(row => row.labels === labels);
    const specs = cases.map(({ method, expression }) => ({ method, expression }));
    const moduleCases = cases.filter(row => row.expression === "(1, 1)");
    assert.equal(moduleCases.length, 4);
    const moduleSpecs = moduleCases.map(({ method, expression }) => ({ method, expression }));
    const rawSource = `from noon import *
import noon as m
import json

${observe}

module_specs = json.loads(${JSON.stringify(JSON.stringify(moduleSpecs))})
module_observations = [observe(spec["method"], ${labels ? "True" : "False"}, spec["expression"]) for spec in module_specs]

class TablePositionAudit(Scene):
    async def construct(self):
        specs = json.loads(${JSON.stringify(JSON.stringify(specs))})
        observations = [observe(spec["method"], ${labels ? "True" : "False"}, spec["expression"]) for spec in specs]
        print("NOON_TABLE_POSITIONS=" + json.dumps({"module": module_observations, "scene": observations}))
        self.add(Dot(radius=0.02))
        await self.wait(0.2)
`;
    const source = rasterFixtureSource(rawSource, "TablePositionAudit");
    let observed;
    const watch = page => page.on("console", message => {
      const value = message.text(), marker = "NOON_TABLE_POSITIONS=";
      if (value.includes(marker)) observed = JSON.parse(value.slice(value.indexOf(marker) + marker.length));
    });
    context.on("page", watch);
    let frame;
    try {
      frame = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.2]);
    } finally {
      context.off("page", watch);
    }
    assert.equal(frame.rendererBackend, expectedBackend);
    assert.equal(frame.presented, true);
    assert.equal(frame.objectCount, 1);
    for (const [scope, expectedCases] of [["module", moduleCases], ["scene", cases]]) {
      assert.ok(Array.isArray(observed?.[scope]), "actual worker must emit every position observation");
      assert.equal(observed[scope].length, expectedCases.length);
      for (let index = 0; index < expectedCases.length; index += 1) {
        const expected = expectedCases[index], actual = observed[scope][index];
        const fields = ["method", "labels", "expression", "result", "events"];
        report.positionDifferences.push(...compare(
          Object.fromEntries(fields.map(key => [key, expected[key]])),
          Object.fromEntries(fields.map(key => [key, actual[key]])),
          `/${scope}/${labels ? "labels" : "plain"}/${index}`,
        ));
        // Keep the broader #2014 wrapper requirement visible rather than
        // presenting a position-only pass as complete cell compatibility.
        if (expected.python_type !== actual.python_type) {
          report.wrapperTypeDifferences.push({ scope, method: expected.method, expression: expected.expression,
            labels, expected: expected.python_type, actual: actual.python_type });
        }
      }
    }
    report.sceneCaseCount += cases.length;
    report.moduleCaseCount += moduleCases.length;
    report.caseCount += cases.length + moduleCases.length;
    report.groups.push({ labels, sceneCaseCount: cases.length, moduleCaseCount: moduleCases.length, sourceSha256: sha256(source), frame });
  }
  assert.equal(report.sceneCaseCount, fixture.cases.length);
  assert.equal(report.moduleCaseCount, 8);
  assert.deepEqual(report.positionDifferences, [], "all pinned position observations must match");
  return report;
}

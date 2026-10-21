import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";
import { readFile } from "node:fs/promises";
import { qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";
import { rasterFixtureSource } from "./manim-raster-support.mjs";

// Return every observation for before/after audits; the qualification entrypoint
// below requires exact equivalence and is the one enrolled in the browser gate.
export async function observeTableMutationPositions(context, baseUrl, expectedBackend) {
  const generator = await readFile(new URL("./manim-table-mutation-reference.py", import.meta.url), "utf8");
  const bytes = await readFile(new URL("../fixtures/manim-v0.21/table-mutation-positions.json", import.meta.url));
  const fixture = JSON.parse(bytes);
  const sha256 = value => createHash("sha256").update(value).digest("hex");
  assert.equal(fixture.manimVersion, "0.21.0");
  assert.equal(sha256(generator), fixture.generatorSha256);
  const start = generator.indexOf("def observe_mutation(");
  const end = generator.indexOf("\ndef main():", start);
  assert.ok(start >= 0 && end > start);
  const constructorEnd = generator.indexOf("    class Operand:", start);
  assert.ok(constructorEnd > start && constructorEnd < end);
  // Reuse only reference construction/mutation code for the direct typed read.
  // No Python position arithmetic or integer-index protocol selects its member.
  const directSource = generator.slice(start, constructorEnd).replace("def observe_mutation(spec):", "def observe_direct_member(spec):") + `
    from _noon_errors import engine_call
    from _manim_semantic_handles import _family_member_handle, _shared_handle_key, _is_missing_handle
    keys = {_shared_handle_key(_family_member_handle(value)[1]): names[id(value)]
            for value in [*cells[0], *cells[1], *rows, *columns, extra]}
    selection = engine_call(table._table_handle.entrySelection, method == "get_entries_without_labels")
    length = engine_call(selection.length) if spec["access"] == "table" else group._semantic_family_handle.memberCount
    assert length == spec["capturedLength"]
    emit("index", spec["rawIndex"])
    getter = selection.selectNormalizedIndex if spec["access"] == "table" else group._semantic_family_handle.memberAt
    handle = engine_call(getter, spec["normalizedIndex"])
    return None if _is_missing_handle(handle) else keys[_shared_handle_key(handle)]
`;
  const report = { fixtureSha256: sha256(bytes), generatorSha256: fixture.generatorSha256,
    caseCount: 0, staticCaseCount: 0, liveCaseCount: 0, moduleCaseCount: 0, directCaseCount: 0, differences: [], groups: [] };
  const batches = [false, true].flatMap(labels => {
    const cases = fixture.cases.filter(row => row.spec.labels === labels);
    return Array.from({ length: Math.ceil(cases.length / 144) }, (_, chunk) => ({
      labels, offset: chunk * 144, expected: cases.slice(chunk * 144, (chunk + 1) * 144),
    }));
  });
  for (const { labels, offset, expected } of batches) {
    const direct = expected.filter(row => row.spec.phase === "index").flatMap(row => {
      const length = BigInt(row.normalization.length), raw = BigInt(row.normalization.index);
      const normalized = raw < 0 ? raw + length : raw;
      if (normalized < 0 || normalized >= length) return [];
      if (typeof row.result !== "string") assert.deepEqual(row.result, { exception: "IndexError", message: "list index out of range" });
      return [{ spec: { ...row.spec, capturedLength: Number(length), rawIndex: Number(raw), normalizedIndex: Number(normalized) },
        result: typeof row.result === "string" ? row.result : null }];
    });
    const live = expected.filter(row => row.spec.phase === "index" || row.spec.action === "append_extra");
    const moduleCases = expected.filter(row =>
      (row.spec.access === "view" && row.spec.action === "remove_first" && row.spec.position[0] === 1 && row.spec.position[1] === 1) ||
      (row.spec.access === "table" && row.spec.phase === "add" && row.spec.action === "append_extra" && row.spec.position[0] > 2));
    const source = rasterFixtureSource(`from noon import *
import noon as m
import json

${generator.slice(start, end)}
${directSource}

module_specs = json.loads(${JSON.stringify(JSON.stringify(moduleCases.map(row => row.spec)))})
module_observations = [observe_mutation(spec) for spec in module_specs]

class TableMutationAudit(Scene):
    async def construct(self):
        specs = json.loads(${JSON.stringify(JSON.stringify(expected.map(row => row.spec)))})
        observations = [observe_mutation(spec) for spec in specs]
        direct_specs = json.loads(${JSON.stringify(JSON.stringify(direct.map(row => row.spec)))})
        direct_observations = [observe_direct_member(spec) for spec in direct_specs]
        self.add(Dot(radius=0.02))
        await self.wait(0.0625)
        live_specs = json.loads(${JSON.stringify(JSON.stringify(live.map(row => row.spec)))})
        live_observations = [observe_mutation(spec) for spec in live_specs]
        print("NOON_TABLE_MUTATIONS=" + json.dumps({"static": observations, "live": live_observations, "module": module_observations, "direct": direct_observations}))
        await self.wait(0.1375)
`, "TableMutationAudit");
    let observed;
    const watch = page => page.on("console", message => {
      const value = message.text(), marker = "NOON_TABLE_MUTATIONS=";
      if (value.includes(marker)) observed = JSON.parse(value.slice(value.indexOf(marker) + marker.length));
    });
    context.on("page", watch);
    let frame;
    try {
      frame = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.0625, 0.2]);
    } finally {
      context.off("page", watch);
    }
    assert.equal(frame.rendererBackend, expectedBackend);
    assert.equal(frame.presented, true);
    assert.equal(frame.objectCount, 1);
    for (const [scope, cases] of [["static", expected], ["live", live], ["module", moduleCases]]) {
      assert.equal(observed?.[scope]?.length, cases.length);
      observed[scope].forEach((actual, index) => {
        if (!isDeepStrictEqual(actual, cases[index])) report.differences.push({ scope, expected: cases[index], actual });
      });
      report[`${scope}CaseCount`] += cases.length;
      report.caseCount += cases.length;
    }
    assert.equal(observed?.direct?.length, direct.length);
    observed.direct.forEach((actual, index) => {
      if (!isDeepStrictEqual(actual, direct[index].result)) report.differences.push({ scope: "direct", spec: direct[index].spec, expected: direct[index].result, actual });
    });
    report.directCaseCount += direct.length;
    report.groups.push({ labels, offset, caseCount: expected.length, sourceSha256: sha256(source), observed, frame });
  }
  assert.equal(report.staticCaseCount, fixture.cases.length);
  assert.equal(report.liveCaseCount, 288);
  assert.equal(report.moduleCaseCount, 8);
  assert.equal(report.directCaseCount, 144);
  return report;
}

export async function qualifyTableMutationPositions(context, baseUrl, expectedBackend) {
  const report = await observeTableMutationPositions(context, baseUrl, expectedBackend);
  assert.deepEqual(report.differences, [], "all pinned mutation/index observations must match");
  return report;
}

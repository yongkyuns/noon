// Execute the same independent pinned observations through the actual worker.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";
import { rasterFixtureSource } from "./manim-raster-support.mjs";

export async function qualifyTableQueryGroups(context, baseUrl, expectedBackend) {
  const generator = await readFile(new URL("./manim-table-groups-reference.py", import.meta.url), "utf8");
  const bytes = await readFile(new URL("../fixtures/manim-v0.21/table-groups.json", import.meta.url));
  const fixture = JSON.parse(bytes);
  const sha256 = value => createHash("sha256").update(value).digest("hex");
  assert.equal(fixture.manimVersion, "0.21.0");
  assert.equal(sha256(generator), fixture.generatorSha256);
  const start = generator.indexOf("def observe_groups(");
  const end = generator.indexOf("\ndef main():", start);
  assert.ok(start >= 0 && end > start);
  const source = rasterFixtureSource(`from noon import *
import noon as m
import json

${generator.slice(start, end)}

module_groups = [observe_groups(labels) for labels in (False, True)]

class TableQueryGroupAudit(Scene):
    async def construct(self):
        scene_groups = [observe_groups(labels) for labels in (False, True)]
        print("NOON_TABLE_GROUPS=" + json.dumps({"module": module_groups, "scene": scene_groups}))
        cells = [Square(0.2), Square(0.3), Square(0.4), Square(0.5)]
        table = MobjectTable([cells[:2], cells[2:]])
        displayed, body = table.get_entries(), table.get_entries_without_labels()
        self.add(table)
        await self.wait(0.0625)
        displayed.remove(cells[0])
        body.remove(cells[1])
        assert table.get_entries() is displayed
        assert table.get_entries_without_labels() is body
        assert list(displayed) == cells[1:]
        assert list(body) == [cells[0], cells[2], cells[3]]
        await self.wait(0.0625)
        assert table.get_entries((1, 1)) is cells[1]
        assert table.get_entries_without_labels((1, 1)) is cells[0]
        displayed.add(cells[0])
        body.add(cells[1])
        assert list(table.get_entries()) == [*cells[1:], cells[0]]
        assert list(table.get_entries_without_labels()) == [cells[0], cells[2], cells[3], cells[1]]
        await self.wait(0.0625)
        self.remove(table)
        self.add(Dot(radius=0.02))
        await self.wait(0.0625)
`, "TableQueryGroupAudit");
  let observed;
  const watch = page => page.on("console", message => {
    const text = message.text(), marker = "NOON_TABLE_GROUPS=";
    if (text.includes(marker)) observed = JSON.parse(text.slice(text.indexOf(marker) + marker.length));
  });
  context.on("page", watch);
  let frame;
  try {
    frame = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.0625, 0.125, 0.1875, 0.25]);
  } finally {
    context.off("page", watch);
  }
  assert.equal(frame.rendererBackend, expectedBackend);
  assert.equal(frame.presented, true);
  assert.equal(frame.objectCount, 1);
  assert.deepEqual(observed?.module, fixture.cases, "module-level canonical group observations");
  assert.deepEqual(observed?.scene, fixture.cases, "scene canonical group observations");
  return { fixtureSha256: sha256(bytes), generatorSha256: fixture.generatorSha256,
    sourceSha256: sha256(source), moduleCases: 2, sceneCases: 2, liveMembershipPassed: true, frame };
}

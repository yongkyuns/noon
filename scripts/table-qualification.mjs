import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_TABLE_ARTIFACTS ?? "table-artifacts",
  cases: [{
    id: "table", file: "table.py", scene: "RetainedTable",
    factory: "createTableRenderer", objectCount: 28,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class TableLifecycle(Scene):
    async def construct(self):
        table = Table([["a", "b"], ["c", "d"]], h_buff=0.9, v_buff=0.4,
                      include_outer_lines=True)
        self.add(table)
        await self.wait(0.05)
        table.shift(RIGHT * 1.5).scale(0.8)
        center = table.get_entries((1, 1)).get_center()
        cell = table.get_cell((1, 1))
        assert abs(cell.width - table.get_columns()[0].width - 0.9 * 0.8) < 1e-6
        assert abs(cell.height - table.get_rows()[0].height - 0.4 * 0.8) < 1e-6
        clone = table.copy()
        clone.shift(LEFT * 3)
        assert (table.get_entries((1, 1)).get_center() - center).length() < 1e-6
        assert abs(clone.get_cell((1, 1)).width - cell.width) < 1e-6
        detached = clone.get_highlighted_cell((2, 2))
        assert len(clone.submobjects[0]) == 0
        assert clone.add_highlighted_cell((1, 1)) is clone
        assert len(clone.submobjects[0]) == 1
        self.add(clone, detached)
        await prepare_latex()
        for table_type in (IntegerTable, DecimalTable):
            numeric = table_type([[1, -2], [3, 4]])
            assert len(numeric.get_entries()) == 4
            self.add(numeric)
            self.remove(numeric)
        await self.wait(0.05)
`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.05, 0.1]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.objectCount, 22);
    assert.ok(Math.abs(result.authoredDuration - 0.1) < 1e-6);
    return result;
  },
});

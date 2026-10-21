import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";
import { qualifyTablePositionProtocols } from "./table-position-checks.mjs";
import { qualifyTableQueryGroups } from "./table-group-checks.mjs";
import { qualifyTableMutationPositions } from "./table-mutation-checks.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_TABLE_ARTIFACTS ?? "table-artifacts",
  cases: [{
    id: "table", file: "table.py", scene: "RetainedTable",
    factory: "createTableRenderer", objectCount: 31,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }, {
    id: "decimal-table-format", file: "decimal_table_format.py", scene: "DecimalTableFormatExample",
    factory: "createDecimalTableFormatRenderer", duration: 0.2,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }, {
    id: "table-labels", file: "table_labels.py", scene: "TableLabelsExample",
    factory: "createTableLabelsRenderer", duration: 0.2,
  }, {
    id: "mobject-table-family-labels", file: "mobject_table_family_labels.py", scene: "MobjectTableFamilyLabelsExample",
    factory: "createMobjectTableFamilyLabelsRenderer", duration: 0.2,
  }, {
    id: "table-line-config", file: "table_line_config.py", scene: "TableLineConfigExample",
    factory: "createTableLineConfigRenderer", duration: 0.2,
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class TableLifecycle(Scene):
    async def construct(self):
        table = Table([["a", "b"], ["c", "d"]], h_buff=0.9, v_buff=0.4,
                      include_outer_lines=True)
        self.add(table)
        horizontal_lines = table.get_horizontal_lines()
        vertical_lines = table.get_vertical_lines()
        assert len(horizontal_lines) == 3
        assert len(vertical_lines) == 3
        assert table.get_horizontal_lines() is horizontal_lines
        assert table.get_vertical_lines() is vertical_lines
        await self.wait(0.0625)
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
        converted = []
        def as_text(value, *, font_size):
            result = Text(str(value), font="DejaVu Sans Mono", font_size=font_size)
            converted.append(result)
            return result
        for table_type, values in (
            (Table, [["plain"]]),
            (MathTable, [["math"]]),
            (IntegerTable, [[7]]),
            (DecimalTable, [[1.25]]),
        ):
            converted_table = table_type(
                values,
                element_to_mobject=as_text,
                element_to_mobject_config={"font_size": 18},
            )
            assert converted_table.get_entries((1, 1)) is converted[-1]
        converted_entry = Circle(radius=0.2)
        def scale_entry(value, *, factor):
            value.scale(factor)
            return value
        converted_mobject_table = MobjectTable(
            [[converted_entry]],
            element_to_mobject=scale_entry,
            element_to_mobject_config={"factor": 0.75},
        )
        assert converted_mobject_table.get_entries((1, 1)) is converted_entry
        await self.wait(0.0625)
        entry = Circle(radius=0.18)
        row_label = Square(side_length=0.14, color=YELLOW)
        column_labels = [Circle(radius=0.1, color=GREEN), Square(side_length=0.12, color=BLUE)]
        top_left_entry = Circle(radius=0.16, color=WHITE)
        labeled = MobjectTable([[entry, Circle(radius=0.12)]], row_labels=[row_label],
                               col_labels=column_labels, top_left_entry=top_left_entry,
                               h_buff=0.7, v_buff=0.45, include_outer_lines=True)
        assert labeled.get_row_labels()[0] is row_label
        assert labeled.get_col_labels()[0] is column_labels[0]
        assert labeled.get_col_labels()[1] is column_labels[1]
        assert labeled.get_labels()[0] is top_left_entry
        assert labeled.row_labels[0] is row_label
        assert labeled.col_labels[0] is column_labels[0]
        assert labeled.top_left_entry is top_left_entry
        assert labeled.get_entries((1, 1)) is top_left_entry
        assert labeled.get_entries((2, 2)) is entry
        assert labeled.get_entries_without_labels((1, 1)) is entry
        assert len(labeled.get_rows()) == 2
        assert len(labeled.get_columns()) == 3
        labeled_copy = labeled.copy()
        assert labeled_copy.row_labels[0] is not row_label
        assert labeled_copy.col_labels[0] is not column_labels[0]
        assert labeled_copy.top_left_entry is not top_left_entry
        assert labeled_copy.get_entries((1, 1)) is not top_left_entry
        assert labeled_copy.get_entries_without_labels((1, 1)) is not entry
        self.add(labeled)
        await self.wait(0.0625)
`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.0625, 0.125, 0.1875]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.objectCount, 35);
    assert.ok(Math.abs(result.authoredDuration - 0.1875) < 1e-6,
      `expected 0.1875 authored seconds, received ${result.authoredDuration}`);
    const positions = await qualifyTablePositionProtocols(context, baseUrl, expectedBackend);
    assert.equal(positions.sceneCaseCount, 228);
    assert.equal(positions.moduleCaseCount, 8);
    const groups = await qualifyTableQueryGroups(context, baseUrl, expectedBackend);
    const mutations = await qualifyTableMutationPositions(context, baseUrl, expectedBackend);
    return { ...result, positions, groups, mutations };
  },
});

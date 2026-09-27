import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_NUMERIC_ARTIFACTS ?? "numeric-artifacts",
  cases: [{
    id: "numeric-decimal",
    file: "numeric_decimal_number.py",
    factory: "createNumericDecimalRenderer",
    objectCount: 1,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }, {
    id: "variable",
    file: "variable.py",
    factory: "createVariableRenderer",
    objectCount: 3,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class VariableLifecycle(Scene):
    async def construct(self):
        await prepare_latex()
        await self.wait(0.05)
        variable = Variable(1.25, "x")
        assert variable.label[0].tex_string == "x"
        assert variable.equals.tex_string == "="
        assert variable.value.get_value() == 1.25
        assert variable.tracker.get_value() == 1.25
        integer = Variable(3, "n", var_type=Integer, num_decimal_places=4)
        assert integer.value.get_value() == 3
        integer.shift(UP)
        self.add(variable, integer)
        integer.tracker.set_value(4.5)
        await self.wait(0.05)
        assert integer.value.get_value() == 4
        self.remove(integer)
        variable.tracker.set_value(7.5)
        assert variable.tracker.get_value() == 7.5
        await self.wait(0.05)
        assert variable.value.get_value() == 7.5
`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.05, 0.1, 0.15]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.objectCount, 3);
    assert.ok(Math.abs(result.authoredDuration - 0.15) < 1e-6);
    return result;
  },
});

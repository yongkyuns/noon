import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_MATRIX_ARTIFACTS ?? "matrix-artifacts",
  cases: [{
    id: "matrix", file: "ordinary_matrix.py", scene: "OrdinaryMatrix",
    factory: "createMatrixRenderer", objectCount: 6,
    preparation: { module: "/web/latex/backend.js", export: "prepareLatexBackend", wrapper: "WasmLatexCompiler" },
  }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class MatrixLifecycle(Scene):
    async def construct(self):
        await prepare_latex()
        ordinary = Matrix([["1", "2"], ["x", "y"]])
        assert len(ordinary.get_entries()) == 4
        assert len(ordinary.get_rows()) == 2
        assert len(ordinary.get_columns()) == 2
        assert len(ordinary.get_brackets()) == 2
        self.add(ordinary)
        self.wait(0.05)
        self.remove(ordinary)
        for matrix_type in (IntegerMatrix, DecimalMatrix):
            numeric = matrix_type([[1, -2], [3, 4]])
            assert len(numeric.get_entries()) == 4
            assert len(numeric.get_columns()[0]) == 2
            self.add(numeric)
            self.remove(numeric)
        first = VGroup(Circle(radius=0.2), Square(side_length=0.3))
        second = MathTex("x", "y")
        supplied = MobjectMatrix([[first, second]])
        assert supplied.get_entries()[0] is first
        assert supplied.get_entries()[1] is second
        assert supplied.get_columns()[1][0] is second
        clone = supplied.copy()
        assert len(clone.get_entries()) == 2
        assert len(clone.get_entries()[0]) == 2
        assert clone.get_entries()[0] is not first
        center = first.get_center()
        clone.shift(UP)
        assert (first.get_center() - center).length() < 1e-6
        try:
            MobjectMatrix([[first, first]])
        except Exception:
            pass
        else:
            raise AssertionError("duplicate Matrix roots must be rejected")
        self.add(supplied, clone)
        self.wait(0.05)

`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.05, 0.1]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.objectCount, 12);
    assert.ok(Math.abs(result.authoredDuration - 0.1) < 1e-6);
    return result;
  },
});

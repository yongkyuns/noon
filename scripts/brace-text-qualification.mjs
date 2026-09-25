import assert from "node:assert/strict";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_BRACE_ARTIFACTS ?? "brace-artifacts",
  cases: [{ id: "brace-text", file: "ordinary_brace_text.py", scene: "BraceTextExample",
    factory: "createBraceTextRenderer", objectCount: 6, duration: 0.2, sampleTime: 0.2 }],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = `from noon import *

class LiveBrace(Scene):
    def construct(self):
        square = Square()
        self.add(square)
        self.play(square.animate.shift(RIGHT * 2), run_time=0.1)
        brace = Brace(square, DOWN)
        assert abs(brace.get_center().x - square.get_center().x) < 1e-6
        assert brace.get_top().y < square.get_bottom().y
        self.add(brace)
        self.wait(0.1)
`;
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.1, 0.2]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.equal(result.objectCount, 2);
    assert.ok(Math.abs(result.authoredDuration - 0.2) < 1e-6);
    return result;
  },
});

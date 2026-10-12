import assert from "node:assert/strict";
import test from "node:test";
import { SemanticPreviewSession } from "./semantic-preview-session.js";

async function fixture(endpoint) {
  const calls = [];
  let finish;
  const sourceResult = new Promise(resolve => { finish = resolve; });
  const session = new SemanticPreviewSession({
    createAuthoringClient: () => ({
      async ready() {},
      run(_source, _context, { onSemanticContinuation }) {
        void onSemanticContinuation({ semanticExecution: {} });
        return sourceResult;
      },
      terminate() {},
    }),
    createExecutionClient: () => ({
      async prepare() {},
      async startSemanticExecution() {},
      async sampleToAuthoredTime(time, options) {
        calls.push({ time, options });
        const complete = options?.stopAtSourceCompletion === true && time >= endpoint;
        if (complete) finish({ duration: endpoint });
        return { time: complete ? endpoint : time, sourceCompleted: complete };
      },
      async metrics() { return { metrics: { backend: "WebGL2", objectCount: 1 } }; },
      terminate() {},
    }),
  });
  await session.open("source");
  return { session, calls };
}

test("completion probes retain the actual source endpoint and requested bound separately", async () => {
  for (const endpoint of [7.999999999999999, 8.000000000000002]) {
    const { session, calls } = await fixture(endpoint);
    try {
      const bound = 8 + 1e-9;
      const result = await session.sample(bound, { stopAtSourceCompletion: true });
      assert.equal(result.frame.requestedTime, bound);
      assert.equal(result.frame.publishedTime, endpoint);
      assert.equal(result.frame.sourceCompleted, true);
      assert.equal(result.authoredDuration, endpoint);
      assert.equal(result.sourceState, "completed");
      assert.deepEqual(calls.at(-1), { time: bound, options: { stopAtSourceCompletion: true } });
    } finally { session.close(); }
  }
});

test("ordinary samples stay strict and an incomplete bounded sample does not claim completion", async () => {
  const { session, calls } = await fixture(8);
  try {
    const strict = await session.sample(2);
    assert.deepEqual(calls.at(-1), { time: 2, options: undefined });
    assert.equal(Object.hasOwn(strict.frame, "sourceCompleted"), false);
    const partial = await session.sample(3, { stopAtSourceCompletion: true });
    assert.equal(partial.frame.sourceCompleted, false);
    assert.equal(partial.sourceState, "running");
    assert.equal(partial.authoredDuration, null);
    await assert.rejects(session.sample(2, { stopAtSourceCompletion: true }), /backwards/);
  } finally { session.close(); }
});

test("completion options are validated before forwarding and do not bypass sampling limits", async () => {
  const { session, calls } = await fixture(8);
  try {
    const before = calls.length;
    for (const value of [null, 1, "true"]) {
      await assert.rejects(session.sample(1, { stopAtSourceCompletion: value }), /boolean/);
    }
    await assert.rejects(session.sample(601, { stopAtSourceCompletion: true }), /limit/);
    assert.equal(calls.length, before);
  } finally { session.close(); }
});

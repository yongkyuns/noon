import assert from "node:assert/strict";
import { test } from "node:test";
import { SemanticPreviewSession } from "./semantic-preview-session.js";

test("preview session preserves the caller authoring context", async (t) => {
  const context = { moduleName: "preview_context", globals: { fixture: true } };
  let observedContext;
  let resolveSource;
  const sourceDone = new Promise((resolve) => { resolveSource = resolve; });
  const authoring = {
    ready: async () => {},
    run: (_source, received, options) => {
      observedContext = received;
      queueMicrotask(() => options.onSemanticContinuation({
        semanticExecution: { contextId: 1, continuationGeneration: 1 },
      }));
      return sourceDone;
    },
    terminate() {},
  };
  const execution = {
    prepare: async () => {},
    startSemanticExecution: async () => {},
    sampleToAuthoredTime: async (time) => ({ time }),
    metrics: async () => ({ metrics: { backend: "WebGL2", objectCount: 0, drawCalls: 0 } }),
    terminate() {},
  };
  const session = new SemanticPreviewSession({
    createAuthoringClient: () => authoring,
    createExecutionClient: () => execution,
  });
  t.after(() => session.close());

  await session.open("scene", { loopDurationSeconds: 4, context });
  assert.equal(observedContext, context);
  resolveSource({ duration: 0 });
});

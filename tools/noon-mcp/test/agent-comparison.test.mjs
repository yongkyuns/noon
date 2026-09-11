import assert from "node:assert/strict";
import { test } from "node:test";

import {
  EVALUATION_MODES,
  aggregateComparison,
  comparisonMarkdown,
  loadEvaluationContext,
  validateComparisonSet,
} from "../eval/agent-comparison.mjs";

function syntheticSet(context, { repetitions = 2 } = {}) {
  const runs = [];
  for (const [modeIndex, mode] of EVALUATION_MODES.entries()) {
    for (const [taskIndex, taskId] of context.taskIds.entries()) {
      for (let repetition = 1; repetition <= repetitions; repetition += 1) {
        const capability = context.taskKinds[taskId] === "capability";
        runs.push({
          mode,
          taskId,
          repetition,
          status: "completed",
          semanticCorrect: modeIndex > 0 || taskIndex % 2 === 0,
          visualCorrect: capability ? null : modeIndex > 0,
          unsupportedApiSuggestionCorrect: capability ? modeIndex > 0 : null,
          repairIterations: Math.max(0, 2 - modeIndex),
          firstUsefulFrameMs: capability ? null : 900 - modeIndex * 200 + taskIndex,
          usage: {
            inputTokens: 1000 + modeIndex * 100,
            outputTokens: 400 + taskIndex,
            cachedInputTokens: 0,
            toolCalls: modeIndex === 0 ? 0 : 2 + modeIndex,
            toolInputTokens: modeIndex === 0 ? 0 : 120,
            toolOutputTokens: modeIndex === 0 ? 0 : 180,
          },
          evidence: [`fixture://${mode}/${taskId}/${repetition}`],
          notes: "synthetic pipeline fixture",
        });
      }
    }
  }
  return {
    schema: 1,
    kind: "noon-agent-stochastic-comparison",
    evaluationId: "synthetic-report-contract",
    fixtureOnly: true,
    repositoryRevision: "612a76dc48ef8f8c0ea6a63e4ce45df541562e7d",
    corpusSha256: context.corpusSha256,
    promptPackSha256: context.promptPackSha256,
    model: {
      provider: "fixture",
      name: "deterministic-test-double",
      version: "1",
      temperature: 0.7,
      topP: 1,
      maxOutputTokens: 4096,
      seedPolicy: "same external policy across all three modes",
      systemPromptSha256: "a".repeat(64),
    },
    taskOrder: [...context.taskIds],
    repetitions,
    runs,
  };
}

test("comparison context binds the prompt pack exactly to the deterministic corpus", async () => {
  const context = await loadEvaluationContext();
  assert.equal(context.taskIds.length, 9);
  assert.equal(context.corpusSha256.length, 64);
  assert.equal(context.promptPackSha256.length, 64);
  assert.deepEqual(Object.keys(context.prompts.modes), EVALUATION_MODES);
});

test("complete three-mode synthetic matrix aggregates the required comparison metrics", async () => {
  const context = await loadEvaluationContext();
  const set = syntheticSet(context);
  const report = aggregateComparison(set, context);
  assert.equal(report.fixtureOnly, true);
  assert.equal(report.taskCount, context.taskIds.length);
  assert.deepEqual(report.taskOrder, context.taskIds);
  assert.deepEqual(Object.keys(report.modes), EVALUATION_MODES);
  assert.equal(report.model.systemPromptSha256, "a".repeat(64));
  assert.equal(report.modes["docs-only"].meanToolCalls, 0);
  assert.equal(report.modes["skill+runner"].meanToolCalls, 3);
  assert.equal(report.modes["skill+MCP"].meanToolCalls, 4);
  assert.equal(report.modes["skill+MCP"].semanticCorrectRate, 1);
  assert.equal(report.modes["skill+MCP"].visualCorrectRate, 1);
  assert.equal(report.modes["skill+MCP"].unsupportedApiSuggestionCorrectRate, 1);
  assert.ok(report.modes["docs-only"].meanFirstUsefulFrameMs > report.modes["skill+MCP"].meanFirstUsefulFrameMs);
  assert.match(comparisonMarkdown(report), /SYNTHETIC FIXTURE — NOT STOCHASTIC AGENT RESULTS/);
  assert.match(comparisonMarkdown(report), /First useful frame \(ms\)/);
  assert.match(comparisonMarkdown(report), /System prompt: `a{64}`/);
});

test("comparison rejects stale corpus, prompt identities, or task order", async () => {
  const context = await loadEvaluationContext();
  const staleCorpus = syntheticSet(context);
  staleCorpus.corpusSha256 = "0".repeat(64);
  assert.throws(() => validateComparisonSet(staleCorpus, context), /corpus hash/);

  const stalePrompts = syntheticSet(context);
  stalePrompts.promptPackSha256 = "1".repeat(64);
  assert.throws(() => validateComparisonSet(stalePrompts, context), /prompt-pack hash/);

  const reordered = syntheticSet(context);
  [reordered.taskOrder[0], reordered.taskOrder[1]] = [reordered.taskOrder[1], reordered.taskOrder[0]];
  assert.throws(() => validateComparisonSet(reordered, context), /taskOrder/);
});

test("comparison rejects incomplete, duplicate, or mode-specific result shapes", async () => {
  const context = await loadEvaluationContext();

  const missing = syntheticSet(context);
  missing.runs.pop();
  assert.throws(() => validateComparisonSet(missing, context), /exactly/);

  const duplicate = syntheticSet(context);
  duplicate.runs[1] = { ...duplicate.runs[0] };
  assert.throws(() => validateComparisonSet(duplicate, context), /duplicate or unexpected/);

  const capability = syntheticSet(context);
  const capabilityRun = capability.runs.find((run) => context.taskKinds[run.taskId] === "capability");
  capabilityRun.visualCorrect = true;
  assert.throws(() => validateComparisonSet(capability, context), /capability runs require null visual/);

  const executable = syntheticSet(context);
  const executableRun = executable.runs.find((run) => context.taskKinds[run.taskId] !== "capability");
  executableRun.firstUsefulFrameMs = null;
  assert.throws(() => validateComparisonSet(executable, context), /executable runs require visual\/latency/);
});

test("comparison schema is strict so mode-specific model/settings cannot silently differ", async () => {
  const context = await loadEvaluationContext();
  const set = syntheticSet(context);
  set.runs[0].model = { name: "different-model" };
  assert.throws(() => validateComparisonSet(set, context), /unknown field model/);

  const missingSystemPrompt = syntheticSet(context);
  delete missingSystemPrompt.model.systemPromptSha256;
  assert.throws(() => validateComparisonSet(missingSystemPrompt, context), /systemPromptSha256/);

  const unknownUsage = syntheticSet(context);
  unknownUsage.runs[0].usage.costUsd = 1;
  assert.throws(() => validateComparisonSet(unknownUsage, context), /unknown field costUsd/);
});

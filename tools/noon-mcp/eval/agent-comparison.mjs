import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_CORPUS_PATH = path.join(here, "corpus.json");
export const DEFAULT_PROMPT_PACK_PATH = path.join(here, "agent-prompts.json");
export const EVALUATION_MODES = Object.freeze(["docs-only", "skill+runner", "skill+MCP"]);

const ID = /^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$/;
const SHA256 = /^[0-9a-f]{64}$/;
const REVISION = /^[0-9a-f]{40}$/;
const RUN_KEYS = new Set([
  "mode", "taskId", "repetition", "status", "semanticCorrect", "visualCorrect",
  "unsupportedApiSuggestionCorrect", "repairIterations", "firstUsefulFrameMs", "usage", "evidence", "notes",
]);
const USAGE_KEYS = new Set([
  "inputTokens", "outputTokens", "cachedInputTokens", "toolCalls", "toolInputTokens", "toolOutputTokens",
]);
const SET_KEYS = new Set([
  "schema", "kind", "evaluationId", "fixtureOnly", "repositoryRevision", "corpusSha256", "promptPackSha256",
  "model", "repetitions", "runs",
]);
const MODEL_KEYS = new Set([
  "provider", "name", "version", "temperature", "topP", "maxOutputTokens", "seedPolicy",
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function object(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new TypeError(`${label} must be an object`);
  return value;
}

function onlyKeys(value, allowed, label) {
  for (const key of Object.keys(value)) if (!allowed.has(key)) throw new Error(`${label} contains unknown field ${key}`);
}

function boundedInt(value, label, { min = 0, max = 10_000_000 } = {}) {
  if (!Number.isSafeInteger(value) || value < min || value > max) throw new RangeError(`${label} is outside its bound`);
  return value;
}

function boundedNumber(value, label, { min = 0, max = 1_000_000_000 } = {}) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) {
    throw new RangeError(`${label} is outside its bound`);
  }
  return value;
}

function boundedText(value, label, { max = 4096, pattern } = {}) {
  if (typeof value !== "string" || value.length === 0 || value.length > max || (pattern && !pattern.test(value))) {
    throw new TypeError(`${label} must be bounded text`);
  }
  return value;
}

function boolOrNull(value, label) {
  if (value !== null && typeof value !== "boolean") throw new TypeError(`${label} must be boolean or null`);
  return value;
}

function mean(values) {
  if (!values.length) return null;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function rate(values) {
  if (!values.length) return null;
  return values.filter(Boolean).length / values.length;
}

function stableModel(model) {
  const row = object(model, "model");
  onlyKeys(row, MODEL_KEYS, "model");
  boundedText(row.provider, "model.provider", { max: 128, pattern: ID });
  boundedText(row.name, "model.name", { max: 256 });
  if (row.version !== undefined) boundedText(row.version, "model.version", { max: 256 });
  boundedNumber(row.temperature, "model.temperature", { max: 2 });
  boundedNumber(row.topP, "model.topP", { max: 1 });
  boundedInt(row.maxOutputTokens, "model.maxOutputTokens", { min: 1, max: 1_000_000 });
  boundedText(row.seedPolicy, "model.seedPolicy", { max: 256 });
  return Object.freeze({
    provider: row.provider,
    name: row.name,
    ...(row.version === undefined ? {} : { version: row.version }),
    temperature: row.temperature,
    topP: row.topP,
    maxOutputTokens: row.maxOutputTokens,
    seedPolicy: row.seedPolicy,
  });
}

function validateUsage(value, label) {
  const usage = object(value, label);
  onlyKeys(usage, USAGE_KEYS, label);
  for (const key of USAGE_KEYS) boundedInt(usage[key], `${label}.${key}`, { max: 1_000_000_000 });
  return usage;
}

function validateEvidence(value, label) {
  if (!Array.isArray(value) || value.length === 0 || value.length > 32) throw new TypeError(`${label} must contain 1..32 entries`);
  for (const [index, item] of value.entries()) boundedText(item, `${label}[${index}]`, { max: 2048 });
  return value;
}

export async function loadEvaluationContext({ corpusPath = DEFAULT_CORPUS_PATH, promptPackPath = DEFAULT_PROMPT_PACK_PATH } = {}) {
  const [corpusBytes, promptBytes] = await Promise.all([readFile(corpusPath), readFile(promptPackPath)]);
  const corpus = JSON.parse(corpusBytes.toString("utf8"));
  const prompts = JSON.parse(promptBytes.toString("utf8"));
  if (corpus?.schema !== 1 || corpus.kind !== "noon-deterministic-agent-evaluation-corpus" || !Array.isArray(corpus.tasks)) {
    throw new Error("deterministic corpus contract is incompatible");
  }
  if (prompts?.schema !== 1 || prompts.kind !== "noon-agent-stochastic-task-prompts" || !Array.isArray(prompts.tasks)) {
    throw new Error("agent prompt pack contract is incompatible");
  }
  const corpusIds = corpus.tasks.map((task) => task.id);
  const promptIds = prompts.tasks.map((task) => task.id);
  if (new Set(corpusIds).size !== corpusIds.length || new Set(promptIds).size !== promptIds.length ||
      corpusIds.length !== promptIds.length || corpusIds.some((id, index) => id !== promptIds[index])) {
    throw new Error("agent prompt pack must cover the deterministic corpus in the same order");
  }
  for (const mode of EVALUATION_MODES) boundedText(prompts.modes?.[mode], `prompt mode ${mode}`, { max: 2048 });
  return Object.freeze({
    corpus,
    prompts,
    taskIds: Object.freeze([...corpusIds]),
    taskKinds: Object.freeze(Object.fromEntries(corpus.tasks.map((task) => [task.id, task.kind]))),
    corpusSha256: sha256(corpusBytes),
    promptPackSha256: sha256(promptBytes),
  });
}

export function validateComparisonSet(input, context) {
  const set = object(input, "comparison set");
  onlyKeys(set, SET_KEYS, "comparison set");
  if (set.schema !== 1 || set.kind !== "noon-agent-stochastic-comparison") throw new Error("comparison set schema/kind mismatch");
  boundedText(set.evaluationId, "evaluationId", { max: 128, pattern: ID });
  if (typeof set.fixtureOnly !== "boolean") throw new TypeError("fixtureOnly must be explicit boolean");
  boundedText(set.repositoryRevision, "repositoryRevision", { max: 40, pattern: REVISION });
  boundedText(set.corpusSha256, "corpusSha256", { max: 64, pattern: SHA256 });
  boundedText(set.promptPackSha256, "promptPackSha256", { max: 64, pattern: SHA256 });
  if (set.corpusSha256 !== context.corpusSha256) throw new Error("comparison corpus hash does not match the current deterministic corpus");
  if (set.promptPackSha256 !== context.promptPackSha256) throw new Error("comparison prompt-pack hash does not match the current prompt pack");
  const model = stableModel(set.model);
  boundedInt(set.repetitions, "repetitions", { min: 1, max: 20 });
  if (!Array.isArray(set.runs)) throw new TypeError("runs must be an array");

  const expected = new Set();
  for (const mode of EVALUATION_MODES) {
    for (const taskId of context.taskIds) {
      for (let repetition = 1; repetition <= set.repetitions; repetition += 1) expected.add(`${mode}\0${taskId}\0${repetition}`);
    }
  }
  if (set.runs.length !== expected.size) throw new Error(`comparison requires exactly ${expected.size} run records`);

  const seen = new Set();
  for (const [index, raw] of set.runs.entries()) {
    const run = object(raw, `runs[${index}]`);
    onlyKeys(run, RUN_KEYS, `runs[${index}]`);
    if (!EVALUATION_MODES.includes(run.mode)) throw new TypeError(`runs[${index}].mode is invalid`);
    if (!context.taskIds.includes(run.taskId)) throw new TypeError(`runs[${index}].taskId is not in the current corpus`);
    boundedInt(run.repetition, `runs[${index}].repetition`, { min: 1, max: set.repetitions });
    const key = `${run.mode}\0${run.taskId}\0${run.repetition}`;
    if (!expected.has(key) || seen.has(key)) throw new Error(`duplicate or unexpected run record ${key}`);
    seen.add(key);
    if (!new Set(["completed", "failed"]).has(run.status)) throw new TypeError(`runs[${index}].status is invalid`);
    if (typeof run.semanticCorrect !== "boolean") throw new TypeError(`runs[${index}].semanticCorrect must be boolean`);
    boolOrNull(run.visualCorrect, `runs[${index}].visualCorrect`);
    boolOrNull(run.unsupportedApiSuggestionCorrect, `runs[${index}].unsupportedApiSuggestionCorrect`);
    boundedInt(run.repairIterations, `runs[${index}].repairIterations`, { max: 1000 });
    if (run.firstUsefulFrameMs !== null) boundedNumber(run.firstUsefulFrameMs, `runs[${index}].firstUsefulFrameMs`, { max: 86_400_000 });
    validateUsage(run.usage, `runs[${index}].usage`);
    validateEvidence(run.evidence, `runs[${index}].evidence`);
    if (run.notes !== undefined) boundedText(run.notes, `runs[${index}].notes`, { max: 4096 });

    const kind = context.taskKinds[run.taskId];
    if (kind === "capability") {
      if (run.visualCorrect !== null || typeof run.unsupportedApiSuggestionCorrect !== "boolean" || run.firstUsefulFrameMs !== null) {
        throw new Error(`${run.taskId}: capability runs require null visual/latency and a boolean unsupported-API score`);
      }
    } else {
      if (typeof run.visualCorrect !== "boolean" || run.unsupportedApiSuggestionCorrect !== null ||
          typeof run.firstUsefulFrameMs !== "number") {
        throw new Error(`${run.taskId}: executable runs require visual/latency scores and null unsupported-API score`);
      }
    }
  }
  if (seen.size !== expected.size) throw new Error("comparison run matrix is incomplete");
  return Object.freeze({ ...set, model });
}

function summarizeMode(runs) {
  const executable = runs.filter((run) => run.visualCorrect !== null);
  const unsupported = runs.filter((run) => run.unsupportedApiSuggestionCorrect !== null);
  return Object.freeze({
    runs: runs.length,
    semanticCorrectRate: rate(runs.map((run) => run.semanticCorrect)),
    visualCorrectRate: rate(executable.map((run) => run.visualCorrect)),
    unsupportedApiSuggestionCorrectRate: rate(unsupported.map((run) => run.unsupportedApiSuggestionCorrect)),
    meanRepairIterations: mean(runs.map((run) => run.repairIterations)),
    meanFirstUsefulFrameMs: mean(executable.map((run) => run.firstUsefulFrameMs)),
    meanInputTokens: mean(runs.map((run) => run.usage.inputTokens)),
    meanOutputTokens: mean(runs.map((run) => run.usage.outputTokens)),
    meanTotalTokens: mean(runs.map((run) => run.usage.inputTokens + run.usage.outputTokens)),
    meanToolCalls: mean(runs.map((run) => run.usage.toolCalls)),
    meanToolTokens: mean(runs.map((run) => run.usage.toolInputTokens + run.usage.toolOutputTokens)),
  });
}

export function aggregateComparison(set, context) {
  const validated = validateComparisonSet(set, context);
  const byMode = Object.fromEntries(EVALUATION_MODES.map((mode) => [
    mode,
    summarizeMode(validated.runs.filter((run) => run.mode === mode)),
  ]));
  return Object.freeze({
    schema: 1,
    kind: "noon-agent-stochastic-comparison-report",
    evaluationId: validated.evaluationId,
    fixtureOnly: validated.fixtureOnly,
    repositoryRevision: validated.repositoryRevision,
    corpusSha256: validated.corpusSha256,
    promptPackSha256: validated.promptPackSha256,
    model: validated.model,
    repetitions: validated.repetitions,
    taskCount: context.taskIds.length,
    modes: byMode,
  });
}

function percent(value) {
  return value === null ? "n/a" : `${(100 * value).toFixed(1)}%`;
}

function number(value, digits = 1) {
  return value === null ? "n/a" : value.toFixed(digits);
}

export function comparisonMarkdown(report) {
  const banner = report.fixtureOnly
    ? "> **SYNTHETIC FIXTURE — NOT STOCHASTIC AGENT RESULTS.** This output only tests the reporting pipeline.\n\n"
    : "";
  const rows = EVALUATION_MODES.map((mode) => {
    const value = report.modes[mode];
    return `| ${mode} | ${percent(value.semanticCorrectRate)} | ${percent(value.visualCorrectRate)} | ${percent(value.unsupportedApiSuggestionCorrectRate)} | ${number(value.meanRepairIterations)} | ${number(value.meanFirstUsefulFrameMs)} | ${number(value.meanTotalTokens)} | ${number(value.meanToolCalls)} | ${number(value.meanToolTokens)} |`;
  }).join("\n");
  return `${banner}# Noon stochastic agent comparison\n\n` +
    `Evaluation: \`${report.evaluationId}\`  \nRepository: \`${report.repositoryRevision}\`  \nCorpus: \`${report.corpusSha256}\`  \nPrompt pack: \`${report.promptPackSha256}\`  \n` +
    `Model: \`${report.model.provider}/${report.model.name}\`  \nRepetitions: ${report.repetitions}; tasks: ${report.taskCount}\n\n` +
    `| Mode | Semantic | Visual | Unsupported API | Repairs | First useful frame (ms) | Tokens | Tool calls | Tool tokens |\n` +
    `| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n${rows}\n`;
}

export async function loadAndAggregateComparison(file, options = {}) {
  const context = await loadEvaluationContext(options);
  const input = JSON.parse(await readFile(file, "utf8"));
  return { context, report: aggregateComparison(input, context) };
}

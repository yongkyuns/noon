import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const DEFAULT_CORPUS_PATH = path.join(here, "corpus.json");

const REQUIRED_COVERAGE = new Set([
  "square-to-circle",
  "sequential-barriers",
  "fade-lifecycle",
  "composition",
  "text",
  "callbacks",
  "unsupported-plotting",
  "unsupported-latex",
  "unsupported-3d",
  "cancellation",
  "repeated-runs",
]);
const TASK_KINDS = new Set(["render", "capability", "cancellation"]);
const CAPABILITY_STATUSES = new Set(["blocked", "deferred", "missing"]);
const HASH_OPS = new Set(["equal", "notEqual"]);
const ID = /^[a-z0-9][a-z0-9-]{0,127}$/;
const FEATURE = /^[A-Za-z_][A-Za-z0-9_.-]{0,127}$/;

function record(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${label} must be a JSON object`);
  }
  return value;
}

function boundedNumber(value, label, { min = 0, max = 600, integer = false } = {}) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max ||
      (integer && !Number.isSafeInteger(value))) {
    throw new RangeError(`${label} is outside its deterministic bound`);
  }
  return value;
}

function identifiers(value, label, pattern = ID) {
  if (!Array.isArray(value) || value.length === 0 || value.some((item) => typeof item !== "string" || !pattern.test(item))) {
    throw new TypeError(`${label} must be a non-empty bounded identifier array`);
  }
  if (new Set(value).size !== value.length) throw new Error(`${label} contains duplicates`);
  return value;
}

function validateRender(task) {
  if (typeof task.example !== "string" || !ID.test(task.example)) throw new TypeError(`${task.id}: invalid example ID`);
  identifiers(task.requiredFeatures, `${task.id}.requiredFeatures`, FEATURE);
  boundedNumber(task.loopDurationSeconds, `${task.id}.loopDurationSeconds`, { min: Number.EPSILON });
  if (!Array.isArray(task.sampleTimes) || task.sampleTimes.length === 0 || task.sampleTimes[0] !== 0) {
    throw new TypeError(`${task.id}: sampleTimes must start at authored time 0`);
  }
  let previous = -Infinity;
  for (const [index, time] of task.sampleTimes.entries()) {
    boundedNumber(time, `${task.id}.sampleTimes[${index}]`);
    if (time < previous) throw new RangeError(`${task.id}: sampleTimes must be nondecreasing`);
    previous = time;
  }
  if (typeof task.expectedBackend !== "string" || task.expectedBackend.trim() === "" || task.expectedBackend.length > 64) {
    throw new TypeError(`${task.id}: expectedBackend must be bounded text`);
  }
  boundedNumber(task.expectedAuthoredDuration, `${task.id}.expectedAuthoredDuration`);
  boundedNumber(task.expectedFinalObjectCount, `${task.id}.expectedFinalObjectCount`, { max: 1_000_000, integer: true });
  boundedNumber(task.repeatFresh, `${task.id}.repeatFresh`, { min: 1, max: 3, integer: true });
  if (!Array.isArray(task.hashRelations)) throw new TypeError(`${task.id}: hashRelations must be an array`);
  for (const [index, relation] of task.hashRelations.entries()) {
    record(relation, `${task.id}.hashRelations[${index}]`);
    if (!HASH_OPS.has(relation.op)) throw new TypeError(`${task.id}: unknown hash relation ${String(relation.op)}`);
    for (const side of ["left", "right"]) {
      boundedNumber(relation[side], `${task.id}.hashRelations[${index}].${side}`, {
        max: task.sampleTimes.length - 1,
        integer: true,
      });
    }
  }
}

function validateCapability(task) {
  if (typeof task.example !== "string" || !ID.test(task.example)) throw new TypeError(`${task.id}: invalid example ID`);
  if (!CAPABILITY_STATUSES.has(task.expectedStatus)) throw new TypeError(`${task.id}: invalid unsupported status`);
  identifiers(task.requiredFeatures, `${task.id}.requiredFeatures`, FEATURE);
}

function validateCancellation(task) {
  if (typeof task.sourcePath !== "string" || !/^eval\/scenes\/[A-Za-z0-9_.-]+\.py$/.test(task.sourcePath) ||
      task.sourcePath.split("/").includes("..")) {
    throw new TypeError(`${task.id}: cancellation source must be a confined eval scene`);
  }
  boundedNumber(task.loopDurationSeconds, `${task.id}.loopDurationSeconds`, { min: Number.EPSILON });
  boundedNumber(task.sampleTime, `${task.id}.sampleTime`, { min: Number.EPSILON });
  boundedNumber(task.abortAfterMs, `${task.id}.abortAfterMs`, { min: 10, max: 5_000, integer: true });
  boundedNumber(task.expectedInitialObjectCount, `${task.id}.expectedInitialObjectCount`, { max: 1_000_000, integer: true });
}

export function validateEvaluationCorpus(input) {
  const corpus = record(input, "evaluation corpus");
  if (corpus.schema !== 1 || corpus.kind !== "noon-deterministic-agent-evaluation-corpus") {
    throw new Error("evaluation corpus schema/kind mismatch");
  }
  record(corpus.reference, "evaluation corpus reference");
  if (!Array.isArray(corpus.tasks) || corpus.tasks.length === 0 || corpus.tasks.length > 32) {
    throw new RangeError("evaluation corpus must contain 1..32 tasks");
  }
  const ids = new Set();
  const coverage = new Set();
  for (const [index, raw] of corpus.tasks.entries()) {
    const task = record(raw, `tasks[${index}]`);
    if (typeof task.id !== "string" || !ID.test(task.id) || ids.has(task.id)) throw new Error(`invalid or duplicate task ID: ${String(task.id)}`);
    ids.add(task.id);
    if (!TASK_KINDS.has(task.kind)) throw new TypeError(`${task.id}: unknown task kind`);
    for (const item of identifiers(task.covers, `${task.id}.covers`)) coverage.add(item);
    if (task.kind === "render") validateRender(task);
    else if (task.kind === "capability") validateCapability(task);
    else validateCancellation(task);
  }
  const missing = [...REQUIRED_COVERAGE].filter((item) => !coverage.has(item));
  if (missing.length) throw new Error(`evaluation corpus misses required coverage: ${missing.join(", ")}`);
  return corpus;
}

export async function loadEvaluationCorpus(file = DEFAULT_CORPUS_PATH) {
  const value = JSON.parse(await readFile(file, "utf8"));
  return validateEvaluationCorpus(value);
}

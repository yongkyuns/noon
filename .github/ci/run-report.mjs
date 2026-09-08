// Read-only measurements for completed Actions attempts. Never a qualification gate.
import assert from "node:assert/strict";
import { appendFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const phases = ["checkout", "restore", "setup", "build-or-lint", "test", "upload", "cleanup", "other"];
const absent = (value) => value === null || value === undefined;
function timestamp(value) {
  if (absent(value)) return null;
  assert.equal(typeof value, "string", "timestamp must be a string");
  const result = Date.parse(value);
  assert.ok(Number.isFinite(result) && result > 0, `invalid timestamp: ${value}`);
  return result;
}
function duration(start, end) {
  const a = timestamp(start);
  const b = timestamp(end);
  if (a === null || b === null) return null;
  assert.ok(b >= a, "reversed timestamps");
  return (b - a) / 1000;
}
function positive(value) {
  assert.ok(Number.isSafeInteger(value) && value > 0, "expected a positive safe integer");
  return value;
}
export function parseAttempts(value) {
  assert.equal(typeof value, "string");
  const specs = value.trim().split(/[\s,]+/);
  assert.ok(specs.length > 0 && specs.length <= 20, "expected 1..20 run:attempt pairs");
  assert.equal(new Set(specs).size, specs.length, "duplicate attempt");
  return specs.map((spec) => {
    assert.match(spec, /^[1-9][0-9]*:[1-9][0-9]*$/, "use explicit run:attempt pairs");
    const [id, attempt] = spec.split(":").map(Number);
    return { id: positive(id), attempt: positive(attempt) };
  });
}
export function phase(name) {
  if (/^(Post |Complete job$)/i.test(name)) return "cleanup";
  if (/checkout/i.test(name)) return "checkout";
  if (/(restore|download.*(package|artifact)|^Cache )/i.test(name)) return "restore";
  if (/upload|publish.*(artifact|cache|sources)/i.test(name)) return "upload";
  if (/^(Set up|Use |Install )/i.test(name)) return "setup";
  if (/(build.*package|package.*build|compile|clippy|lint workspace|seed.*compile)/i.test(name)) return "build-or-lint";
  if (/^(Test |Check |Run .*tests|Run web preflight)/i.test(name)) return "test";
  return "other";
}

export function summarizeRun(run, jobs) {
  positive(run.id);
  positive(run.run_attempt);
  assert.equal(run.status, "completed", "only completed attempts can be measured");
  assert.ok(run.conclusion, "missing run conclusion");
  assert.ok(Array.isArray(jobs) && jobs.length > 0, "missing jobs");
  assert.equal(new Set(jobs.map((job) => job.id)).size, jobs.length, "duplicate job");
  const start = run.run_started_at;
  timestamp(start);
  const rows = jobs.map((job) => {
    positive(job.id);
    assert.equal(job.run_id, run.id, "job belongs to another run");
    assert.equal(job.run_attempt, run.run_attempt, "mixed run attempts");
    assert.equal(job.status, "completed", "unfinished job");
    assert.ok(job.conclusion, "missing job conclusion");
    // Skipped jobs have no executed checks; a zero contribution is not a pass.
    const skipped = job.conclusion === "skipped";
    const seconds = skipped ? 0 : duration(job.started_at, job.completed_at);
    const buckets = Object.fromEntries(phases.map((name) => [name, 0]));
    const steps = (job.steps ?? []).map((step) => {
      const group = phase(step.name);
      const ran = step.status === "completed" && step.conclusion !== "skipped";
      const elapsed = ran ? duration(step.started_at, step.completed_at) : null;
      if (ran && elapsed === null) buckets[group] = null;
      else if (elapsed !== null && buckets[group] !== null) buckets[group] += elapsed;
      if (ran && step.started_at && step.completed_at && job.started_at && job.completed_at) {
        assert.ok(timestamp(step.started_at) >= timestamp(job.started_at)
          && timestamp(step.completed_at) <= timestamp(job.completed_at), "step outside job interval");
      }
      return { number: step.number, name: step.name, status: step.status,
        conclusion: step.conclusion ?? null, phase: group, seconds: elapsed };
    });
    const stepSeconds = !Array.isArray(job.steps) || steps.length === 0 || steps.some((s) => s.status !== "completed"
      || (s.conclusion !== "skipped" && s.seconds === null)) ? null
      : steps.reduce((sum, s) => sum + (s.seconds ?? 0), 0);
    if (!Array.isArray(job.steps) || steps.length === 0) for (const name of phases) buckets[name] = null;
    if (stepSeconds !== null && seconds !== null) assert.ok(stepSeconds <= seconds, "overlapping step durations");
    return { id: job.id, name: job.name, conclusion: job.conclusion,
      runnerSeconds: seconds,
      // Do not call run-start -> dependent-job-start a runner queue duration.
      schedulingDelaySeconds: skipped ? null : duration(job.created_at, job.started_at),
      beforeJobCreationSeconds: skipped ? null : duration(start, job.created_at),
      phases: buckets, unclassifiedRunnerSeconds: seconds === null || stepSeconds === null
        ? null : seconds - stepSeconds,
      failedSteps: steps.filter((s) => s.conclusion && !["success", "skipped"].includes(s.conclusion)).map((s) => s.name),
      skippedSteps: steps.filter((s) => s.conclusion === "skipped").map((s) => s.name),
      steps };
  });
  const allocated = jobs.filter((job) => job.conclusion !== "skipped");
  const ends = allocated.map((job) => timestamp(job.completed_at));
  const last = ends.length && ends.every((end) => end !== null) ? Math.max(...ends) : null;
  const firstStarts = allocated.map((job) => timestamp(job.started_at));
  const first = firstStarts.length && firstStarts.every((time) => time !== null) ? Math.min(...firstStarts) : null;
  const all = (key) => rows.every((row) => row[key] !== null);
  const knownRunnerSeconds = rows.reduce((sum, row) => sum + (row.runnerSeconds ?? 0), 0);
  return { schema: 1, id: run.id, attempt: run.run_attempt, workflow: run.name,
    event: run.event, apiHeadSha: run.head_sha, testedCheckoutSha: null,
    conclusion: run.conclusion, runStartedAt: start ?? null,
    observedLatencySeconds: last === null ? null : duration(start, new Date(last).toISOString()),
    initialSchedulingSeconds: first === null ? null : duration(start, new Date(first).toISOString()),
    runnerSeconds: all("runnerSeconds") ? knownRunnerSeconds : null, knownRunnerSeconds,
    schedulingDelaySeconds: rows.filter((row) => row.conclusion !== "skipped").every((row) => row.schedulingDelaySeconds !== null)
      ? rows.reduce((sum, row) => sum + (row.schedulingDelaySeconds ?? 0), 0) : null,
    allListedChecksSucceeded: run.conclusion === "success" && rows.every((row) => row.conclusion === "success"
      && row.skippedSteps.length === 0 && row.failedSteps.length === 0
      && row.steps.length > 0 && row.steps.every((step) => step.status === "completed" && step.conclusion === "success")),
    jobs: rows };
}

// Restrict all authenticated requests to fixed GitHub REST endpoints; no response URLs or redirects.
export async function collectRun(repository, spec, { token, fetchImpl = fetch } = {}) {
  assert.match(repository, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/, "invalid repository");
  assert.ok(!repository.split("/").some((part) => [".", ".."].includes(part)), "invalid repository");
  positive(spec.id);
  positive(spec.attempt);
  const prefix = `https://api.github.com/repos/${repository}/actions/runs/${spec.id}/attempts/${spec.attempt}`;
  async function get(url) {
    const response = await fetchImpl(url, { redirect: "error", signal: AbortSignal.timeout(30000),
      headers: { Accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
        ...(token ? { Authorization: `Bearer ${token}` } : {}) } });
    assert.ok(response.ok, `GitHub REST HTTP ${response.status}`);
    // Bound each page without exposing response bodies or credentials on error.
    const reader = response.body.getReader();
    const chunks = [];
    let size = 0;
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        size += value.byteLength;
        assert.ok(size <= 8 * 1024 * 1024, "GitHub REST response too large");
        chunks.push(value);
      }
    } finally { await reader.cancel(); }
    try { return JSON.parse(Buffer.concat(chunks).toString("utf8")); }
    catch { throw new Error("invalid GitHub REST JSON"); }
  }
  const run = await get(prefix);
  assert.equal(run.id, spec.id, "unexpected run identity");
  assert.equal(run.run_attempt, spec.attempt, "unexpected attempt identity");
  const jobs = [];
  let total;
  for (let page = 1; page <= 20; page += 1) {
    const data = await get(`${prefix}/jobs?per_page=100&page=${page}`);
    assert.ok(Number.isSafeInteger(data.total_count) && data.total_count > 0 && data.total_count <= 2000, "invalid job count");
    total ??= data.total_count;
    assert.equal(data.total_count, total, "job count changed during collection");
    assert.ok(Array.isArray(data.jobs) && data.jobs.length > 0, "incomplete job listing");
    jobs.push(...data.jobs);
    assert.ok(jobs.length <= total, "unexpected extra jobs");
    if (jobs.length === total) return { run, jobs, summary: summarizeRun(run, jobs) };
  }
  throw new Error("job pagination limit exceeded");
}

const cell = (value) => String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;")
  .replaceAll(">", "&gt;").replaceAll("|", "&#124;").replaceAll("`", "&#96;").replace(/[\r\n]/g, " ");
const seconds = (value) => value === null ? "unavailable" : value.toFixed(1);
export function formatReport(summaries) {
  const lines = ["# CI attempt measurements", "",
    "Historical observations, not a controlled speedup experiment or approval to merge.",
    "Runner time is measured execution (including post-job steps), not billed minutes. Scheduling delay includes dispatch/provisioning; pure queue time is unavailable.",
    "Observed latency ends at the last job completion, not a workflow-finalization timestamp. Dependent-job delay is not added to runner time.",
    "API head SHA is NOT the tested checkout SHA on pull requests. Verify checkout logs/artifact provenance separately.",
    "Step phases are name-based accounting buckets. Composite builds and Cargo test may include setup, compilation, linking, and tests; they cannot be split from step metadata alone.", "",
    "| Workflow / run:attempt | Result | Observed latency (s) | Runner minutes | Job scheduling delay sum (s) |",
    "| --- | --- | ---: | ---: | ---: |"];
  for (const sample of summaries) {
    lines.push(`| ${cell(sample.workflow)} / ${sample.id}:${sample.attempt} | ${cell(sample.conclusion)} | ${seconds(sample.observedLatencySeconds)} | ${seconds(sample.runnerSeconds === null ? null : sample.runnerSeconds / 60)} | ${seconds(sample.schedulingDelaySeconds)} |`);
  }
  for (const sample of summaries) {
    lines.push("", `## ${cell(sample.workflow)} — ${sample.id}:${sample.attempt}`, "",
      `Event: ${cell(sample.event)}; API head: ${cell(sample.apiHeadSha)}.`,
      "No speedup inferred from failures, cancellations, missing timestamps, changed workloads, or skipped checks.", "",
      "| Job | Result | Runner seconds | Scheduling delay (s) | Failed / skipped steps |",
      "| --- | --- | ---: | ---: | --- |");
    for (const job of sample.jobs) {
      lines.push(`| ${cell(job.name)} | ${cell(job.conclusion)} | ${seconds(job.runnerSeconds)} | ${seconds(job.schedulingDelaySeconds)} | ${cell([...job.failedSteps.map((name) => `FAILED: ${name}`), ...job.skippedSteps.map((name) => `SKIPPED: ${name}`)].join("; "))} |`);
    }
    lines.push("", "Full per-step phases and explicit unknown values are in report.json.");
  }
  return `${lines.join("\n")}\n`;
}
async function main() {
  const [mode, ...args] = process.argv.slice(2);
  const out = "ci-artifacts/run-timing";
  await mkdir(out, { recursive: true });
  let summaries;
  if (mode === "collect") {
    const [repository, ...runs] = args;
    const samples = [];
    for (const spec of parseAttempts(runs.join(" "))) {
      const result = await collectRun(repository, spec, { token: process.env.GH_TOKEN });
      // Whitelisted summary fields only: no actors, signed URLs, tokens, or logs.
      samples.push(result.summary);
      await writeFile(path.join(out, "report.json"), `${JSON.stringify(samples, null, 2)}\n`);
    }
    summaries = samples;
  } else if (mode === "offline" && args.length === 1) {
    const samples = JSON.parse(await readFile(args[0], "utf8"));
    assert.ok(Array.isArray(samples), "expected an array of {run, jobs} snapshots");
    summaries = samples.map(({ run, jobs }) => summarizeRun(run, jobs));
    await writeFile(path.join(out, "report.json"), `${JSON.stringify(summaries, null, 2)}\n`);
  } else throw new Error("usage: run-report.mjs collect owner/repo run:attempt [...] | offline snapshots.json");
  const markdown = formatReport(summaries);
  await writeFile(path.join(out, "summary.md"), markdown);
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, markdown);
  process.stdout.write(markdown);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}

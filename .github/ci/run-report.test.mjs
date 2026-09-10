import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { collectRun, formatReport, parseAttempts, phase, summarizeRun } from "./run-report.mjs";

const time = (seconds) => new Date(Date.UTC(2026, 8, 8) + seconds * 1000).toISOString();
function fixture() {
  const run = { id: 123, run_attempt: 1, name: "CI", event: "pull_request",
    status: "completed", conclusion: "success", head_sha: "a".repeat(40),
    created_at: time(0), run_started_at: time(0), updated_at: time(9999) };
  const job = (id, name, created, start, end, stepName) => ({ id, name, run_id: 123,
    run_attempt: 1, status: "completed", conclusion: "success", created_at: time(created),
    started_at: time(start), completed_at: time(end), steps: [{ number: 1, name: stepName,
      status: "completed", conclusion: "success", started_at: time(start + 1), completed_at: time(end - 1) }] });
  return { run, jobs: [job(1, "Web package", 0, 5, 65, "Build browser package"),
    job(2, "Rust", 0, 10, 110, "Test workspace"),
    job(3, "Browser", 65, 70, 100, "Test browser rendering")] };
}
const summarize = ({ run, jobs }) => summarizeRun(run, jobs);

test("parallel runner time is summed, not confused with elapsed time or dependency wait", () => {
  const report = summarize(fixture());
  assert.equal(report.runnerSeconds, 190);
  assert.equal(report.observedLatencySeconds, 110);
  assert.equal(report.schedulingDelaySeconds, 20);
  assert.equal(report.initialSchedulingSeconds, 5);
  assert.equal(report.jobs[2].beforeJobCreationSeconds, 65);
  assert.equal(report.jobs[2].schedulingDelaySeconds, 5);
  assert.equal(report.jobs[0].phases["build-or-lint"], 58);
  assert.equal(report.jobs[0].unclassifiedRunnerSeconds, 2);
  assert.equal(report.allListedChecksSucceeded, true);
});

test("workflow updated_at is not treated as completion and head is not treated as checkout", () => {
  const report = summarize(fixture());
  assert.equal(report.observedLatencySeconds, 110);
  assert.equal(report.apiHeadSha, "a".repeat(40));
  assert.equal(report.testedCheckoutSha, null);
  assert.match(formatReport([report]), /NOT the tested checkout/);
});

test("rerun timing starts at this attempt, not original workflow creation", () => {
  const data = fixture();
  data.run.run_attempt = 2;
  data.run.created_at = time(-10000);
  for (const job of data.jobs) job.run_attempt = 2;
  assert.equal(summarize(data).observedLatencySeconds, 110);
  data.jobs[0].run_attempt = 1;
  assert.throws(() => summarize(data), /mixed run attempts/);
});

test("failed and skipped work is visible and cannot indicate full qualification", () => {
  const data = fixture();
  data.run.conclusion = "failure";
  data.jobs[2].conclusion = "failure";
  data.jobs[2].steps[0].conclusion = "failure";
  data.jobs[2].steps.push({ name: "Remaining checks", number: 2, status: "completed", conclusion: "skipped" });
  data.jobs.push({ id: 4, name: "Parity", run_id: 123, run_attempt: 1,
    status: "completed", conclusion: "skipped", steps: null });
  const report = summarize(data);
  assert.equal(report.allListedChecksSucceeded, false);
  assert.equal(report.jobs[3].runnerSeconds, 0);
  assert.equal(report.jobs[3].schedulingDelaySeconds, null);
  assert.equal(report.jobs[3].phases.test, null);
  assert.equal(report.runnerSeconds, 190);
  assert.match(formatReport([report]), /FAILED: Test browser rendering; SKIPPED: Remaining checks/);
});

test("conditional skips on otherwise green runs are not silently called complete coverage", () => {
  const data = fixture();
  data.jobs[0].steps.push({ name: "Conditional WebGL", number: 2, status: "completed", conclusion: "skipped" });
  assert.equal(summarize(data).allListedChecksSucceeded, false);
  assert.equal(summarize(data).conclusion, "success");
});

test("missing timestamps remain unknown, with separately labeled known totals", () => {
  const data = fixture();
  data.jobs[0].completed_at = null;
  data.jobs[0].created_at = null;
  const report = summarize(data);
  assert.equal(report.runnerSeconds, null);
  assert.equal(report.knownRunnerSeconds, 130);
  assert.equal(report.observedLatencySeconds, null);
  assert.equal(report.schedulingDelaySeconds, null);
  assert.match(formatReport([report]), /unavailable/);
});

test("missing step metadata is not a fabricated zero duration", () => {
  const data = fixture();
  data.jobs[0].steps = null;
  const report = summarize(data);
  assert.equal(report.jobs[0].phases.setup, null);
  assert.equal(report.jobs[0].unclassifiedRunnerSeconds, null);
  assert.equal(report.allListedChecksSucceeded, false);
  data.jobs[1].steps[0].completed_at = null;
  assert.equal(summarize(data).jobs[1].phases.test, null);
});

test("unfinished runs, reversed clocks, duplicate and foreign jobs fail closed", () => {
  const changes = [
    (d) => { d.run.status = "in_progress"; },
    (d) => { d.jobs[0].started_at = time(99); },
    (d) => { d.jobs[0].completed_at = "not a date"; },
    (d) => { d.jobs[1].id = d.jobs[0].id; },
    (d) => { d.jobs[0].run_id = 999; },
    (d) => { d.jobs[0].status = "queued"; },
    (d) => { d.jobs[0].steps[0].completed_at = time(999); },
  ];
  for (const change of changes) {
    const data = fixture(); change(data);
    assert.throws(() => summarize(data));
  }
});

test("phase labels preserve setup/restore/cleanup and combined compiler boundaries", () => {
  for (const [name, expected] of [["Checkout", "checkout"], ["Restore WASM cache", "restore"],
    ["Download browser package", "restore"], ["Use stable Rust", "setup"],
    ["Build and validate browser package", "build-or-lint"], ["Clippy workspace", "build-or-lint"],
    ["Test workspace", "test"], ["Upload browser package", "upload"],
    ["Post Build and validate browser package", "cleanup"], ["Summarize browser timing", "other"]]) {
    assert.equal(phase(name), expected);
  }
});

test("run selection requires bounded explicit attempts without shell syntax or duplicate inputs", () => {
  assert.deepEqual(parseAttempts("123:1, 456:2"), [{ id: 123, attempt: 1 }, { id: 456, attempt: 2 }]);
  for (const value of ["", "123", "0:1", "123:0", "123:1;echo hello", "1:1 1:1",
    "9007199254740992:1", "01:1", Array.from({ length: 21 }, (_, i) => `${i + 1}:1`).join(" ")]) {
    assert.throws(() => parseAttempts(value));
  }
});

function api(data, pageJobs) {
  const urls = [];
  const fetchImpl = async (url, options) => {
    urls.push(url);
    assert.equal(options.redirect, "error");
    assert.ok(url.startsWith("https://api.github.com/repos/owner/repo/actions/runs/123/attempts/1"));
    assert.equal(options.headers.Authorization, "Bearer test-token");
    const value = url.includes("/jobs?") ? pageJobs(new URL(url).searchParams.get("page")) : data.run;
    return new Response(JSON.stringify(value), { status: 200 });
  };
  return { urls, fetchImpl, token: "test-token" };
}

test("collector pins every paginated request to the selected attempt", async () => {
  const data = fixture();
  const client = api(data, (page) => ({ total_count: 3, jobs: [data.jobs[Number(page) - 1]] }));
  const result = await collectRun("owner/repo", { id: 123, attempt: 1 }, client);
  assert.equal(result.jobs.length, 3);
  assert.equal(client.urls.length, 4);
  assert.equal(result.summary.runnerSeconds, 190);
});

test("collector rejects unstable, truncated, duplicate, and mixed-attempt listings", async () => {
  const data = fixture();
  for (const pages of [
    (page) => ({ total_count: Number(page) === 1 ? 3 : 4, jobs: [data.jobs[0]] }),
    (page) => ({ total_count: 3, jobs: Number(page) === 1 ? [data.jobs[0]] : [] }),
    () => ({ total_count: 3, jobs: [data.jobs[0]] }),
    () => ({ total_count: 3, jobs: data.jobs.map((job) => ({ ...job, run_attempt: 2 })) }),
  ]) {
    await assert.rejects(collectRun("owner/repo", { id: 123, attempt: 1 }, api(data, pages)));
  }
});

test("collector validates request identity before network access", async () => {
  const fetchImpl = () => { throw new Error("unexpected request"); };
  await assert.rejects(collectRun("owner/repo/../../elsewhere", { id: 123, attempt: 1 }, { fetchImpl }), /invalid repository/);
  await assert.rejects(collectRun("owner/repo", { id: -1, attempt: 1 }, { fetchImpl }), /positive safe integer/);
  const data = fixture();
  data.run.run_attempt = 2;
  await assert.rejects(collectRun("owner/repo", { id: 123, attempt: 1 }, api(data, () => ({}))), /unexpected attempt/);
});

test("HTTP error does not print token or untrusted response body", async () => {
  await assert.rejects(collectRun("owner/repo", { id: 123, attempt: 1 }, {
    token: "secret-value", fetchImpl: async () => new Response("secret-value", { status: 403 }),
  }), (error) => error.message === "GitHub REST HTTP 403");
});

test("collector bounds response bytes", async () => {
  await assert.rejects(collectRun("owner/repo", { id: 123, attempt: 1 }, {
    fetchImpl: async () => new Response(" ".repeat(8 * 1024 * 1024 + 1)),
  }), /too large/);
});

test("summary escapes job/step names rather than injecting HTML or Markdown cells", () => {
  const data = fixture();
  data.jobs[0].name = "<script>|`name`\nnext";
  const markdown = formatReport([summarize(data)]);
  assert.ok(!markdown.includes("<script>"));
  assert.match(markdown, /&lt;script&gt;&#124;&#96;name&#96; next/);
  assert.match(markdown, /pure queue time is unavailable/);
});

test("offline CLI writes actual JSON and Markdown from a complete snapshot", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-run-report-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFile(path.join(root, "snapshots.json"), JSON.stringify([fixture()]));
  const script = fileURLToPath(new URL("./run-report.mjs", import.meta.url));
  const stdout = execFileSync(process.execPath, [script, "offline", "snapshots.json"], {
    cwd: root, encoding: "utf8", env: { ...process.env, GITHUB_STEP_SUMMARY: path.join(root, "step-summary.md") },
  });
  assert.match(stdout, /123:1/);
  const report = JSON.parse(await readFile(path.join(root, "ci-artifacts/run-timing/report.json")));
  assert.equal(report[0].runnerSeconds, 190);
  assert.equal(await readFile(path.join(root, "ci-artifacts/run-timing/summary.md"), "utf8"), stdout);
  assert.equal(await readFile(path.join(root, "step-summary.md"), "utf8"), stdout);
});

test("repository dot segments and invalid JSON cannot change request scope or leak response data", async () => {
  for (const repository of ["../repo", "owner/.", "owner/.."]) {
    await assert.rejects(collectRun(repository, { id: 123, attempt: 1 }, {
      fetchImpl: () => { throw new Error("unexpected network request"); },
    }), /invalid repository/);
  }
  await assert.rejects(collectRun("owner/repo", { id: 123, attempt: 1 }, {
    fetchImpl: async () => new Response("untrusted-response-data", { status: 200 }),
  }), (error) => error.message === "invalid GitHub REST JSON");
});

// #1933 — diagnostic-only Mac Python HTTP server startup investigation.
// Not a performance sample, browser test, or candidate-vs-baseline comparison.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { readFile, writeFile, unlink, mkdir } from "node:fs/promises";
import path from "node:path";
import os from "node:os";

export const STUDY_ID = "1933-macos-server-preflight-20261009-01";

export function interpretation(observed) {
  const issues = [];
  if (observed?.childAliveBeforeHTTP !== true) issues.push("server child not alive before health probe");
  if (observed?.response?.status !== 200) issues.push("localhost GET failed or returned non-200");
  if (observed?.response?.bodyMatch !== true) issues.push("checkout-specific identity proof failed");
  if (observed?.owner?.ownerMatched === false) issues.push("localhost service belongs to a different process");
  return {
    status: issues.length ? "not-operational" : "operational",
    blockers: issues,
    bannerSeen: observed?.bannerSeen === true,
    bannerRequired: false,
    qualification: false, performanceAcceptance: false, mergeApproval: false,
  };
}

export function validatePlan(plan) {
  assert.equal(plan.studyId, STUDY_ID);
  assert.equal(plan.runner, "macos-15");
  assert.equal(plan.diagnosticOnly, true);
  assert.equal(plan.qualification, false);
  assert.equal(plan.performanceAcceptance, false);
  assert.equal(plan.mergeApproval, false);
  assert.deepEqual(plan.windows, [
    { id: "original-command", durationSeconds: 10 },
    { id: "response-and-owner", durationSeconds: 5 },
  ]);
  assert.equal(plan.priorAttempt.completedPairs, 0);
  assert.equal(plan.priorAttempt.artifactId, 11639543175);
  assert.equal(plan.retries, 0);
  assert.equal(plan.oneShot, true);
  return plan;
}

function safeExec(command, args, cwd) {
  try {
    return { output: execFileSync(command, args, { cwd, encoding: "utf8",
      maxBuffer: 128 * 1024, timeout: 5000, stdio: ["ignore", "pipe", "pipe"] }).trim(), error: null };
  } catch (e) {
    return { output: e.stdout ? String(e.stdout) : "",
      error: String(e.message ?? e), stderr: e.stderr ? String(e.stderr) : null };
  }
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function spawnArgs(kind, port, root) {
  assert.ok(kind === "original" || kind === "minimal", "unregistered process form");
  if (kind === "original") return ["-u", "-m", "http.server",
    String(port), "--bind", "127.0.0.1", "--directory", root];
  return ["-u", "-c",
    "import http.server,socketserver,sys,functools\n" +
    "handler=functools.partial(http.server.SimpleHTTPRequestHandler,directory=sys.argv[2])\n" +
    "with socketserver.TCPServer(('127.0.0.1',int(sys.argv[1])),handler) as server:\n" +
    " print('DIAGNOSTIC_CUSTOM_SERVER_BOUND',flush=True)\n" +
    " server.serve_forever()\n", String(port), root];
}

async function oneProcess({ kind, port, root, probePath, marker, bannerWaitMs, requestTimeoutMs }) {
  const args = spawnArgs(kind, port, root);
  const record = {
    kind, port, command: "python3", args, pid: null, childAliveBeforeHTTP: false,
    startedAt: new Date().toISOString(), elapsedBeforeProbeMs: bannerWaitMs,
    stdout: "", stderr: "", bannerSeen: false, response: null,
    owner: null, childEvents: [], errors: [],
  };
  const child = spawn("python3", args, { cwd: root,
    stdio: ["ignore", "pipe", "pipe"] });
  record.pid = child.pid ?? null;
  const maximumOutput = 131072;
  for (const channel of ["stdout", "stderr"]) {
    child[channel]?.on("data", chunk => {
      record[channel] = (record[channel] + String(chunk)).slice(-maximumOutput);
    });
  }
  child.on("error", e => {
    record.childEvents.push({ type: "error", message: String(e) });
  });
  child.on("exit", (code, signal) => {
    record.childEvents.push({ type: "exit", code, signal });
  });
  const shutdown = async () => {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill("SIGTERM");
      await Promise.race([new Promise(resolve => child.once("exit", resolve)), delay(2000)]);
      if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
    }
  };
  try {
    // One fixed wait to independently observe whether the original banner appears.
    // Do not use output text to gate the subsequent first HTTP request.
    await delay(bannerWaitMs);
    record.bannerSeen = /Serving HTTP on/.test(record.stdout + record.stderr);
    record.childAliveBeforeHTTP = child.exitCode === null && child.signalCode === null;
    const owner = safeExec("lsof", ["-nP", "-iTCP:" + port, "-sTCP:LISTEN"], root);
    record.owner = {
      command: "lsof -nP -iTCP:" + port + " -sTCP:LISTEN",
      ...owner,
      ownerMatched: owner.error === null && record.pid !== null
        ? owner.output.split("\n").slice(1).some(line =>
          line.trim().split(/\s+/)[1] === String(record.pid))
        : null,
    };
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), requestTimeoutMs);
    try {
      const response = await fetch("http://127.0.0.1:" + port + probePath, {
        signal: controller.signal, redirect: "error", cache: "no-store",
      });
      const body = await response.text();
      record.response = { status: response.status, contentType: response.headers.get("content-type"),
        bytes: body.length, bodyMatch: body === marker,
        responsePreview: body.slice(0, 240) };
    } catch (e) {
      record.response = { status: null, bodyMatch: false, error: String(e),
        cause: e?.cause ? String(e.cause) : null };
    } finally { clearTimeout(timer); }
  } finally {
    await shutdown();
    record.finishedAt = new Date().toISOString();
    record.decision = interpretation(record);
  }
  return record;
}

export async function run(plan, root, outfile) {
  validatePlan(plan);
  assert.equal(process.platform, "darwin", "not macOS");
  assert.equal(process.arch, "arm64", "not Apple Silicon");
  assert.equal(process.version, "v22.23.3", "Node changed");
  assert.equal(process.env.GITHUB_RUN_ATTEMPT, "1", "not original attempt");
  assert.equal(process.env.GITHUB_EVENT_NAME, "pull_request", "not PR opened");
  const meta = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  assert.equal(meta.action, "opened", "not original PR-opened event");
  const randomId = randomBytes(12).toString("hex");
  const marker = "noon-macos-http-startup-" + randomId + "\n";
  const markerFile = path.join(root, "web", "preflight-" + randomId + ".txt");
  const probePath = "/web/preflight-" + randomId + ".txt";
  const evidence = {
    schema: 1, studyId: STUDY_ID, diagnosticOnly: true,
    qualification: false, performanceAcceptance: false, mergeApproval: false,
    environment: {
      platform: process.platform, arch: process.arch,
      node: process.version, cpus: os.availableParallelism(),
      python: safeExec("python3", ["--version"], root),
      whichPython: safeExec("which", ["python3"], root),
      netstatPort: safeExec("lsof", ["-nP", "-iTCP:4205", "-sTCP:LISTEN"], root),
      runnerOS: process.env.RUNNER_OS || null, runnerArch: process.env.RUNNER_ARCH || null,
      imageOS: process.env.ImageOS || null, imageVersion: process.env.ImageVersion || null,
    },
    cases: [], studyComplete: false,
  };
  await writeFile(markerFile, marker, { flag: "wx" });
  try {
    const first = await oneProcess({ kind: "original", port: 4205, root,
      probePath, marker, bannerWaitMs: 10000, requestTimeoutMs: 5000 });
    evidence.cases.push(first);
    if (first.decision.status !== "operational") {
      // Preregistered independent fallback diagnostic with *different* port
      // and Python's explicit TCPServer. It is not a second A/A trial or retry.
      evidence.cases.push(await oneProcess({ kind: "minimal", port: 4206, root,
        probePath, marker, bannerWaitMs: 2000, requestTimeoutMs: 5000 }));
    }
    evidence.studyComplete = evidence.cases.length >= 1;
    evidence.originalServerOperational = first.decision.status === "operational";
    evidence.originalBannerSeen = first.bannerSeen;
    return evidence;
  } finally {
    await unlink(markerFile);
    await mkdir(path.dirname(outfile), { recursive: true });
    await writeFile(outfile, JSON.stringify(evidence, null, 2) + "\n", { flag: "wx" });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(new URL(import.meta.url).pathname)) {
  const args=process.argv.slice(2);
  assert.equal(args.length, 3, "usage: node macos-http-server-preflight.mjs PLAN ROOT OUTPUT");
  const plan=JSON.parse(await readFile(args[0], "utf8"));
  const evidence=await run(plan, path.resolve(args[1]), path.resolve(args[2]));
  console.log(JSON.stringify({ complete: evidence.studyComplete,
    original: evidence.cases[0]?.decision, cases: evidence.cases.map(c => ({
      kind: c.kind, pid: c.pid, bannerSeen: c.bannerSeen,
      response: c.response, owner: c.owner?.ownerMatched, decision: c.decision,
    })) }));
}
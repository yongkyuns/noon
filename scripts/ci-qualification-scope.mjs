import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const preflightOnlyFiles = new Set([
  "README.md", "AGENTS.md", "AGENTS.override.md", ".github/ci/README.md",
  ".github/ci/manim-raster-workflow-cadence.test.mjs",
  ".github/ci/run-report.mjs", ".github/ci/run-report.test.mjs",
  ".github/ci/physical-gpu-mode.test.mjs",
]);

const validRevision = revision => /^[0-9a-f]{40}$/.test(revision) && !/^0+$/.test(revision);
const gitOptions = { stdio: "pipe", timeout: 30000 };

function changedPaths(before, after, runGit) {
  runGit("git", ["fetch", "--no-tags", "--depth=1", "origin", before], gitOptions);
  const diff = runGit("git", ["diff", "--name-only", "--no-renames", "-z", before, after], gitOptions);
  if (!Buffer.isBuffer(diff) || !diff.length || diff.at(-1) !== 0) throw new Error("incomplete change list");
  const paths = diff.toString("utf8").slice(0, -1).split("\0");
  if (paths.some(path => !path)) throw new Error("empty changed path");
  return paths;
}

// Read the complete push diff, including deletions and both sides of renames.
// Missing history, malformed output and unknown inputs retain full qualification.
export function needsFullCiQualification({ event, before, after }, runGit = execFileSync) {
  if (event !== "push" || !validRevision(before) || !validRevision(after)) return true;
  try {
    const paths = changedPaths(before, after, runGit);
    return !paths.length || paths.some(path =>
      !preflightOnlyFiles.has(path) && !path.startsWith("docs/") &&
      !(path.startsWith("web/") && path.endsWith(".test.mjs") &&
        path !== "web/ci-workflow-topology.test.mjs"));
  } catch {
    return true;
  }
}

// Reuse the complete NUL-delimited diff, without the event path-list limit.
// Keep manual runs, unknown inputs, mixed changes and incomplete history eligible.
export function needsPagesDeployment({ event, before, after }, runGit = execFileSync) {
  if (event !== "push" || !validRevision(before) || !validRevision(after)) return true;
  try {
    return changedPaths(before, after, runGit).some(path =>
      !/^crates\/[^/]+\/tests\//.test(path) &&
      !/^\.github\/ci\/[^/]+\.test\.mjs$/.test(path) &&
      path !== ".github/ci/README.md" &&
      !(path.startsWith(".github/workflows/") && path !== ".github/workflows/pages.yml"));
  } catch {
    return true;
  }
}

// Ordinary PRs qualify the feature they change; the existing nightly/manual
// raster oracle still runs every CUDA fidelity case against generic engine edits.
export function needsCudaDemoQualification({ event, before, after }, runGit = execFileSync) {
  if (!["push", "pull_request"].includes(event) || !validRevision(after)) return true;
  try {
    if (event === "pull_request") {
      // Raw commit headers retain parents in a shallow checkout. Use the tested
      // merge's real base rather than potentially stale pull_request.base data.
      const raw = runGit("git", ["cat-file", "-p", after], gitOptions);
      if (!Buffer.isBuffer(raw)) return true;
      const header = raw.toString("utf8").split("\n\n", 1)[0];
      const parents = [...header.matchAll(/^parent ([0-9a-f]{40})$/gm)].map(match => match[1]);
      if (parents.length !== 2) return true;
      before = parents[0];
    }
    if (!validRevision(before)) return true;
    return changedPaths(before, after, runGit).some(path =>
      path === ".github/workflows/ci.yml" || path === ".github/workflows/manim-raster-differential.yml" ||
      path === "scripts/ci-qualification-scope.mjs" || path.startsWith("scripts/cuda-transpose") ||
      path.startsWith("tools/cuda-transpose/") || path.startsWith("tests/cuda_") ||
      path === "web/cuda-transpose-banks.test.mjs" || path === "web/python/examples/cuda_transpose_banks.py");
  } catch {
    return true;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const input = {
    event: process.env.GITHUB_EVENT_NAME,
    before: process.env.BEFORE_REVISION,
    after: process.env.GITHUB_SHA,
  };
  if (process.argv[2] === "pages") {
    const deploy = needsPagesDeployment(input);
    appendFileSync(process.env.GITHUB_OUTPUT, `deploy=${deploy}\n`);
    console.log(`Pages deployment required: ${deploy}`);
  } else {
    const full = needsFullCiQualification(input);
    const cuda = needsCudaDemoQualification(input);
    appendFileSync(process.env.GITHUB_OUTPUT, `full=${full}\ncuda_demo=${cuda}\n`);
    console.log(`Full CI qualification required: ${full}`);
    console.log(`CUDA scene fidelity required: ${cuda}`);
  }
}

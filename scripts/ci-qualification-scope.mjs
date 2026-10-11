import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const preflightOnlyFiles = new Set([
  "README.md", "AGENTS.md", "AGENTS.override.md", ".github/ci/README.md",
  ".github/ci/manim-raster-workflow-cadence.test.mjs",
  ".github/ci/run-report.mjs", ".github/ci/run-report.test.mjs",
  ".github/ci/physical-gpu-mode.test.mjs",
]);

// Read the complete push diff, including deletions and both sides of renames.
// Missing history, malformed output and unknown inputs retain full qualification.
export function needsFullCiQualification({ event, before, after }, runGit = execFileSync) {
  const validRevision = revision => /^[0-9a-f]{40}$/.test(revision) && !/^0+$/.test(revision);
  if (event !== "push" || !validRevision(before) || !validRevision(after)) return true;
  try {
    const options = { stdio: "pipe", timeout: 30000 };
    runGit("git", ["fetch", "--no-tags", "--depth=1", "origin", before], options);
    const diff = runGit("git", ["diff", "--name-only", "--no-renames", "-z", before, after], options);
    if (!Buffer.isBuffer(diff) || !diff.length || diff.at(-1) !== 0) return true;
    const paths = diff.toString("utf8").slice(0, -1).split("\0");
    if (paths.some(path => !path)) return true;
    return !paths.length || paths.some(path =>
      !preflightOnlyFiles.has(path) && !path.startsWith("docs/") &&
      !(path.startsWith("web/") && path.endsWith(".test.mjs") &&
        path !== "web/ci-workflow-topology.test.mjs"));
  } catch {
    return true;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const full = needsFullCiQualification({
    event: process.env.GITHUB_EVENT_NAME,
    before: process.env.BEFORE_REVISION,
    after: process.env.GITHUB_SHA,
  });
  appendFileSync(process.env.GITHUB_OUTPUT, `full=${full}\n`);
  console.log(`Full CI qualification required: ${full}`);
}

#!/usr/bin/env node
import { runFeedbackCli } from "../src/feedback-cli.mjs";

const USAGE = `Usage:
  noon-feedback import-live --input <viewer-feedback.json> --output <new-feedback-dir>
  noon-feedback check --feedback <feedback.json> --source <scene.py>
  noon-feedback compare --feedback <feedback.json> --candidate <manifest.json> [--sample <index>] --source <revised.py> --output <new-dir>

In the playground, choose Comment, mark an area or point, and download the note.
Use import-live to verify and extract the source, PNG, and feedback record.
Use schema-2 noon-preview output for the candidate. Sample indices start at zero.
Bundles retain PNG and source snapshots. Existing output directories are never reused.
Check exits 2 when the source is stale, 1 on error, and 0 when current.
Comparison preserves both observations and remains pending review; it never retimes
the interval or declares an educational, narration, or timing problem resolved.
`;

try {
  const result = await runFeedbackCli({ argv: process.argv.slice(2) });
  if (result.help) process.stdout.write(USAGE);
  else {
    process.stdout.write(`${JSON.stringify(result)}\n`);
    if (result.sourceStatus === "stale") process.exitCode = 2;
  }
} catch (error) {
  process.stderr.write(`${String(error?.message ?? error).slice(0, 2000)}\n`);
  process.exitCode = 1;
}

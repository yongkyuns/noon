#!/usr/bin/env node
import { runFeedbackCli } from "../src/feedback-cli.mjs";

const USAGE = `Usage:
  noon-feedback import-live --input <viewer-feedback.json> --output <new-feedback-dir>
  noon-feedback review --preview <manifest.json> --source <scene.py> --output <new-review-dir>
  noon-feedback import --review <review-dir> --note <downloaded-note.json> --output <new-feedback-dir>
  noon-feedback annotate --preview <manifest.json> [--sample <index>] --source <scene.py> --input <note.json> --output <new-dir>
  noon-feedback check --feedback <feedback.json> --source <scene.py>
  noon-feedback compare --feedback <feedback.json> --candidate <manifest.json> [--sample <index>] --source <revised.py> --output <new-dir>

In the playground, choose Comment, click the captured picture, and write one note.
Import-live unpacks that download into the existing verified feedback bundle.
It includes the source snapshot and PNG; do not share it where that source is private.

Use captured schema-2 noon-preview output. No renderer, provider, or credentials are needed.
Review writes a portable page: open index.html, select a captured moment, click
on the image to place a comment pin, type a comment, and download a feedback note.
Optional rectangle drawing and a suggested correction remain available. Import validates that note against the
preserved source and complete frame identity before creating a feedback bundle.
The page displays captured stills; it does not play or seek an animation.
Sample indices start at zero. Note JSON contains interval {start,end}, observation,
desiredChange, optional preserve, and optional normalized point {x,y} or region {x,y,width,height}
with a top-left origin. The interval must contain the published capture time.
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

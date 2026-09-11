#!/usr/bin/env node
import { resolve } from "node:path";

import { comparisonMarkdown, loadAndAggregateComparison } from "../eval/agent-comparison.mjs";

function usage() {
  return "usage: node scripts/report-agent-evaluation.mjs <comparison.json> [--format json|markdown] [--allow-fixture]";
}

const args = process.argv.slice(2);
if (!args.length || args.includes("--help")) {
  console.error(usage());
  process.exit(args.includes("--help") ? 0 : 2);
}
const file = args.shift();
let format = "markdown";
let allowFixture = false;
while (args.length) {
  const flag = args.shift();
  if (flag === "--format") {
    format = args.shift();
    if (!new Set(["json", "markdown"]).has(format)) throw new Error("--format must be json or markdown");
  } else if (flag === "--allow-fixture") {
    allowFixture = true;
  } else {
    throw new Error(`unknown argument: ${flag}`);
  }
}

const { report } = await loadAndAggregateComparison(resolve(file));
if (report.fixtureOnly && !allowFixture) {
  throw new Error("synthetic fixture comparison refused; pass --allow-fixture only when testing the reporting pipeline");
}
if (format === "json") process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else process.stdout.write(comparisonMarkdown(report));

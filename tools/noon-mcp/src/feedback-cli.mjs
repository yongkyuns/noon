import path from "node:path";

import { buildFeedbackComparison, checkFeedbackSource } from "./frame-feedback.mjs";
import { feedbackBundle, jsonBytes, previewFrame, sourceFile, writeBundle } from "./feedback-files.mjs";
import { importLiveFeedback } from "./feedback-live-import.mjs";

export function parseFeedbackCliArgs(argv) {
  if (!Array.isArray(argv) || argv.some((value) => typeof value !== "string")) {
    throw new TypeError("feedback CLI arguments must be strings");
  }
  if (argv.includes("--help") || argv.includes("-h")) return { help: true };
  const command = argv[0];
  const commands = {
    check: ["feedback", "source"],
    compare: ["feedback", "candidate", "sample", "source", "output"],
    "import-live": ["input", "output"],
  };
  const flags = Object.hasOwn(commands, command) ? commands[command] : null;
  if (!flags) throw new TypeError("expected import-live, check, or compare");
  const args = { command, sample: 0 };
  for (let index = 1; index < argv.length; index += 2) {
    const name = argv[index].replace(/^--/, "");
    if (argv[index] !== `--${name}` || !flags.includes(name)) {
      throw new TypeError(`unknown ${command} argument: ${argv[index]}`);
    }
    if (Object.hasOwn(args, name) && name !== "sample") throw new TypeError(`duplicate --${name}`);
    if (name === "sample" && args.sampleSupplied) throw new TypeError("duplicate --sample");
    const value = argv[index + 1];
    if (typeof value !== "string" || value.trim() === "" || value.includes("\0")) {
      throw new TypeError(`--${name} requires a non-empty value without NUL`);
    }
    if (name === "sample") {
      if (!/^(0|[1-9][0-9]*)$/.test(value) || Number(value) > 31) {
        throw new RangeError("--sample must be an integer from 0 to 31");
      }
      args.sample = Number(value);
      args.sampleSupplied = true;
    } else args[name] = value;
  }
  for (const name of flags.filter((value) => value !== "sample")) {
    if (!Object.hasOwn(args, name)) throw new TypeError(`--${name} is required`);
  }
  delete args.sampleSupplied;
  return Object.freeze(args);
}

/** Offline evidence packaging only; never starts a renderer or changes a scene. */
export async function runFeedbackCli({ argv, cwd = process.cwd() }) {
  const args = parseFeedbackCliArgs(argv);
  if (args.help) return Object.freeze({ help: true });
  if (args.command === "import-live") return importLiveFeedback({
    inputPath: path.resolve(cwd, args.input), outputDir: path.resolve(cwd, args.output),
  });
  const source = await sourceFile(path.resolve(cwd, args.source));

  const original = await feedbackBundle(path.resolve(cwd, args.feedback));
  if (args.command === "check") return checkFeedbackSource(original.record, source.sha256);

  const candidate = await previewFrame(path.resolve(cwd, args.candidate), args.sample, source.sha256);
  const comparison = buildFeedbackComparison(original.record, candidate, source.sha256);
  const outputDir = path.resolve(cwd, args.output);
  await writeBundle(outputDir, {
    "comparison.json": jsonBytes(comparison),
    "before.png": original.png, "after.png": candidate.png,
    "before-source.txt": original.source.bytes, "after-source.txt": source.bytes,
  });
  return Object.freeze({ outputDir, comparisonPath: path.join(outputDir, "comparison.json"), comparison });
}

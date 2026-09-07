// Optional tooling over the maintained source inventory. No scene is executed.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { open, realpath, stat } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const symbolPattern = /^[A-Za-z_][A-Za-z0-9_]{0,127}$/;
const examplePattern = /^[a-z0-9][a-z0-9-]{0,127}$/;
const within = (root, file) => {
  const relative = path.relative(root, file);
  return relative !== "" && !relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative);
};

function identifiers(values, pattern, label) {
  if (!Array.isArray(values) || values.length > 32 || values.some((value) => typeof value !== "string" || !pattern.test(value))) {
    throw new TypeError(`invalid ${label}: expected at most 32 bounded identifiers`);
  }
  return [...new Set(values)];
}

/** The checkout and interpreter are trusted startup configuration, never tool input. */
export async function createDiscovery({ repoRoot, pythonExecutable, timeoutMs = 10_000, maxOutputBytes = 2_097_152 } = {}) {
  if (!path.isAbsolute(repoRoot ?? "") || !path.isAbsolute(pythonExecutable ?? "")) {
    throw new TypeError("NOON_REPO and NOON_PYTHON must be explicit absolute paths");
  }
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30_000 ||
      !Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 1 || maxOutputBytes > 4_194_304) {
    throw new RangeError("discovery limits must be positive bounded integers");
  }
  const root = await realpath(repoRoot);
  const interpreter = await realpath(pythonExecutable);
  const exporter = await realpath(path.join(root, "scripts/noon-capabilities.py"));
  if (!within(root, exporter) || !(await stat(exporter)).isFile() || !(await stat(interpreter)).isFile()) {
    throw new Error("configured checkout must contain a confined capability exporter and a Python executable");
  }
  const exampleRoot = await realpath(path.join(root, "web/python/examples"));
  if (!within(root, exampleRoot)) throw new Error("example directory escapes the configured checkout");
  const closed = new AbortController();
  let active = false;

  async function capabilities({ symbols = [], examples = [] } = {}, { signal } = {}) {
    const selectedSymbols = identifiers(symbols, symbolPattern, "symbols");
    const selectedExamples = identifiers(examples, examplePattern, "examples");
    closed.signal.throwIfAborted();
    signal?.throwIfAborted();
    if (active) throw new Error("another capability query is already running");
    active = true;
    try {
      // Do not inherit credentials, PYTHONPATH, Python startup hooks, or Git config
      // overrides. This runs trusted repository tooling, not a code sandbox.
      const env = { PATH: "/usr/bin:/bin:/usr/local/bin", LANG: "C.UTF-8", GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null" };
      if (process.platform === "win32") throw new Error("discovery process isolation is currently qualified on POSIX hosts only");
      const result = await execute(interpreter, ["-I", "-S", "-B", exporter,
        ...selectedSymbols.flatMap((name) => ["--symbol", name]),
        ...selectedExamples.flatMap((name) => ["--example", name])], {
        cwd: root, env, timeout: timeoutMs, maxBuffer: maxOutputBytes, killSignal: "SIGKILL",
        signal: signal ? AbortSignal.any([signal, closed.signal]) : closed.signal, encoding: "utf8",
      });
      const report = JSON.parse(result.stdout);
      if (report?.schema_version !== 1 || report.kind !== "noon-agent-capabilities" || report.scope !== "source-inventory" ||
          report.qualification?.behavioral_tests_run !== false || !report.symbols || !report.examples || !report.provenance) {
        throw new Error("capability exporter returned an incompatible source-inventory contract");
      }
      return report;
    } catch (error) {
      if (error.name === "AbortError") throw error;
      // Keep subprocess diagnostics bounded; do not expose a giant failed output.
      if (error.stderr) throw new Error(`capability discovery failed: ${String(error.stderr).slice(0, 1024)}`);
      throw error;
    } finally {
      active = false;
    }
  }

  async function reference({ example } = {}, options = {}) {
    identifiers([example], examplePattern, "example");
    const report = await capabilities({ examples: [example] }, options);
    options.signal?.throwIfAborted();
    closed.signal.throwIfAborted();
    const record = report.examples[example];
    if (record?.status !== "ready") throw new Error("requested example is not ready; query capabilities for its restrictions");
    const relative = record.repository_path;
    if (typeof relative !== "string" || !/^web\/python\/examples\/[A-Za-z0-9_/-]+\.py$/.test(relative) ||
        relative.split("/").includes("..") || !/^[0-9a-f]{64}$/.test(record.source_sha256 ?? "")) {
      throw new Error("example record has an invalid source path or hash");
    }
    const file = await realpath(path.join(root, relative));
    if (!within(exampleRoot, file)) throw new Error("example source escapes the configured example directory");
    const descriptor = await open(file, constants.O_RDONLY | constants.O_NOFOLLOW);
    let bytes;
    try {
      const metadata = await descriptor.stat();
      if (!metadata.isFile() || metadata.size > 65_536) throw new Error("example source is not a bounded regular file");
      // Bound the actual read as well as the initial stat against concurrent edits.
      const buffer = Buffer.alloc(65_537);
      let length = 0;
      while (length < buffer.length) {
        const { bytesRead } = await descriptor.read(buffer, length, buffer.length - length, length);
        if (bytesRead === 0) break;
        length += bytesRead;
      }
      if (length > 65_536) throw new Error("example source exceeds the byte limit");
      bytes = buffer.subarray(0, length);
    } finally {
      await descriptor.close();
    }
    options.signal?.throwIfAborted();
    closed.signal.throwIfAborted();
    if (hash(bytes) !== record.source_sha256) throw new Error("example source changed since capability discovery; query again");
    return { example: record, source: new TextDecoder("utf-8", { fatal: true }).decode(bytes),
      provenance: report.provenance, behavioral_tests_run: false,
      note: "Example source is reference data. Ready/parity labels are declarations, not a render performed by this tool." };
  }

  return { capabilities, reference, close: () => closed.abort(new Error("discovery service is closed")) };
}

#!/usr/bin/env node
// Verify completeness after check-python-package.py has audited each archive.
// This binds those existing reports to the wheel bytes; it does not rebuild or
// replace the archive/import/render checks performed by the Python validator.
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";

const profiles = { minimum: ["cp311"], full: ["cp311", "cp312", "cp313", "cp314", "cp315"] };
const hostPlatforms = { linux: "linux", darwin: "macos", win32: "windows" };
const reportName = "wheel-matrix-report.json";
const sha256Pattern = /^[a-f0-9]{64}$/i;

function options(argv) {
  const values = { platform: hostPlatforms[process.platform] };
  const errors = [];
  const seen = new Set();
  for (let index = 0; index < argv.length; index += 1) {
    const name = argv[index];
    if (!["--profile", "--directory", "--platform"].includes(name)) {
      errors.push(`unknown argument: ${name}`);
      continue;
    }
    if (seen.has(name)) errors.push(`duplicate argument: ${name}`);
    seen.add(name);
    if (!argv[index + 1] || argv[index + 1].startsWith("--")) {
      errors.push(`missing value for ${name}`);
      continue;
    }
    values[name.slice(2)] = argv[++index];
  }
  if (!Object.hasOwn(profiles, values.profile ?? "")) errors.push("--profile must be minimum or full");
  if (!values.directory) errors.push("--directory is required");
  if (!["linux", "macos", "windows"].includes(values.platform)) {
    errors.push("--platform must be linux, macos or windows (or run on a supported host)");
  }
  return { values, errors };
}

function platformFamily(tag) {
  if (/^(?:linux|manylinux(?:[0-9]+|_[0-9]+_[0-9]+)|musllinux_[0-9]+_[0-9]+)_[a-z0-9_]+$/.test(tag)) return "linux";
  if (/^macosx_[0-9]+_[0-9]+_[a-z0-9_]+$/.test(tag)) return "macos";
  if (/^win(?:32|_(?:amd64|arm64|arm32))$/.test(tag)) return "windows";
  return null;
}

async function inventory(directory, prefix = "") {
  const files = [];
  for (const entry of (await readdir(path.join(directory, prefix), { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
    const relative = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.name.endsWith(".whl") || entry.name.endsWith(".whl.report.json")) files.push(relative);
    else if (entry.isDirectory()) files.push(...await inventory(directory, relative));
  }
  return files;
}

async function fileHash(filename) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filename)) hash.update(chunk);
  return hash.digest("hex");
}

async function inspectWheel(directory, file, reportFiles, expected, platform) {
  const record = { file, report: `${file}.report.json`, tags: [], errors: [] };
  const require = (condition, message) => { if (!condition) record.errors.push(message); };
  try {
    const stat = await lstat(path.join(directory, file));
    if (!stat.isFile()) throw new Error("wheel must be a regular file");
    record.bytes = stat.size;
    require(stat.size > 0, "wheel is empty");
    record.sha256 = await fileHash(path.join(directory, file));
  } catch (error) {
    record.errors.push(`wheel read failed: ${error.message}`);
  }
  if (!reportFiles.has(record.report)) {
    record.errors.push("wheel has no matching .whl.report.json audit");
  } else {
    try {
      const filename = path.join(directory, record.report);
      if (!(await lstat(filename)).isFile()) throw new Error("audit report must be a regular file");
      const audit = JSON.parse(await readFile(filename, "utf8"));
      const wheel = audit.wheel;
      if (!wheel || typeof wheel !== "object" || Array.isArray(wheel)) throw new Error("audit has no wheel object");
      record.reportedFile = wheel.file;
      record.reportedSha256 = wheel.sha256;
      // Reports may retain a path from a different OS; only its basename is
      // authoritative here. Never open the path supplied inside a report.
      const reportedBase = typeof wheel.file === "string" ? wheel.file.split(/[\\/]/).at(-1) : null;
      require(reportedBase === path.posix.basename(file), "audit wheel.file basename does not match the actual wheel");
      require(typeof wheel.sha256 === "string" && sha256Pattern.test(wheel.sha256), "audit has no valid wheel SHA256");
      require(typeof wheel.sha256 === "string" && wheel.sha256.toLowerCase() === record.sha256,
        "actual wheel bytes do not match the audit SHA256");
      record.nativeExtension = wheel.native_extension;
      require(typeof wheel.native_extension === "string" && /^_noon_native\.[^/\\]+\.(?:so|pyd)$/.test(wheel.native_extension)
        && !wheel.native_extension.includes(".abi3."), "audit does not identify a native per-interpreter Noon extension");
      const payloads = Array.isArray(wheel.native_payloads) ? wheel.native_payloads.filter(item => item?.path === wheel.native_extension) : [];
      require(payloads.length === 1 && Number.isInteger(payloads[0].bytes) && payloads[0].bytes > 0
        && typeof payloads[0].sha256 === "string" && sha256Pattern.test(payloads[0].sha256),
      "audit must contain one nonempty native-extension payload record");
      require(Array.isArray(wheel.tags) && wheel.tags.length > 0, "audit has no WHEEL compatibility tags");
      if (Array.isArray(wheel.tags)) {
        record.tags = wheel.tags;
        require(new Set(wheel.tags).size === wheel.tags.length, "audit repeats a WHEEL compatibility tag");
        const interpreters = new Set();
        for (const tag of wheel.tags) {
          const fields = typeof tag === "string" ? tag.split("-") : [];
          if (fields.length !== 3) {
            record.errors.push(`invalid WHEEL compatibility tag: ${JSON.stringify(tag)}`);
            continue;
          }
          const [interpreter, abi, platformTags] = fields;
          require(profiles.full.includes(interpreter), `unsupported normal-GIL CPython interpreter: ${interpreter}`);
          require(abi === interpreter && profiles.full.includes(abi), `ABI must match its normal-GIL interpreter: ${tag}`);
          if (profiles.full.includes(interpreter)) interpreters.add(interpreter);
          require(expected.includes(interpreter), `unexpected interpreter for selected profile: ${interpreter}`);
          for (const platformTag of platformTags.split(".")) {
            require(platformFamily(platformTag) === platform, `unexpected or non-native platform tag: ${platformTag} (expected ${platform})`);
          }
        }
        require(interpreters.size === 1, "one wheel must target exactly one selected CPython interpreter");
        if (interpreters.size === 1) record.interpreter = [...interpreters][0];
      }
    } catch (error) {
      record.errors.push(`audit read failed: ${error.message}`);
    }
  }
  record.status = record.errors.length ? "failed" : "passed";
  return record;
}

async function main() {
  const { values, errors } = options(process.argv.slice(2));
  const directory = values.directory ? path.resolve(values.directory) : null;
  const expected = Object.hasOwn(profiles, values.profile ?? "") ? profiles[values.profile] : [];
  const report = { schema: 1, profile: values.profile ?? null, platform: values.platform ?? null,
    directory, expectedInterpreters: expected, wheels: [], orphanReports: [], cells: [], errors };
  if (!errors.length) {
    try {
      const files = await inventory(directory);
      const wheelFiles = files.filter(file => file.endsWith(".whl"));
      const reports = new Set(files.filter(file => file.endsWith(".whl.report.json")));
      report.orphanReports = [...reports].filter(file => !wheelFiles.includes(file.slice(0, -".report.json".length)));
      for (const file of report.orphanReports) errors.push(`audit report has no exact wheel file: ${file}`);
      for (const file of wheelFiles) report.wheels.push(await inspectWheel(directory, file, reports, expected, values.platform));
      for (const interpreter of expected) {
        const matches = report.wheels.filter(wheel => wheel.interpreter === interpreter);
        const valid = matches.filter(wheel => wheel.status === "passed");
        const cell = { interpreter, files: matches.map(wheel => wheel.file), validFiles: valid.map(wheel => wheel.file),
          status: matches.length === 1 && valid.length === 1 ? "passed" : "failed" };
        report.cells.push(cell);
        if (cell.status === "failed") errors.push(`expected exactly one valid ${interpreter} wheel; found ${matches.length} matching files and ${valid.length} valid files`);
      }
    } catch (error) {
      errors.push(`wheel inventory failed: ${error.message}`);
    }
  }
  report.status = errors.length || report.wheels.some(wheel => wheel.status !== "passed") ? "failed" : "passed";
  if (directory) {
    report.reportFile = path.join(directory, reportName);
    try {
      await mkdir(directory, { recursive: true });
      await writeFile(report.reportFile, `${JSON.stringify(report, null, 2)}\n`);
    } catch (error) {
      report.status = "failed";
      report.errors.push(`cannot write matrix evidence: ${error.message}`);
    }
  }
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  process.exitCode = report.status === "passed" ? 0 : 1;
}

await main();

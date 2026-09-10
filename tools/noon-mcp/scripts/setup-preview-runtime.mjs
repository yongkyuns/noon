import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { deriveNoonPreviewSeccompProfile } from "../src/preview-seccomp.mjs";

const PLAYWRIGHT_VERSION = "1.62.1";
const SECCOMP_URL = `https://raw.githubusercontent.com/microsoft/playwright/v${PLAYWRIGHT_VERSION}/utils/docker/seccomp_profile.json`;
const SECCOMP_GIT_BLOB = "fddc05fb520affb145404e6f6f647ca96af8087d";
const IMAGE_TAG = "noon-preview-runtime:1.62.1";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(scriptDir, "..");
const cacheDir = process.env.NOON_PREVIEW_CACHE || path.join(homedir(), ".cache", "noon-preview");
const runtimePath = path.join(cacheDir, "runtime.json");
const seccompPath = path.join(cacheDir, "playwright-seccomp-v1.62.1-noon.json");

function run(executable, args, { cwd } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { cwd, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => { if (stdout.length < 65536) stdout += chunk; });
    child.stderr.on("data", (chunk) => { if (stderr.length < 65536) stderr += chunk; });
    child.once("error", reject);
    child.once("close", (code) => {
      if (code !== 0) reject(new Error(`${executable} failed (${code}): ${stderr.trim().slice(0, 1200)}`));
      else resolve(stdout.trim());
    });
  });
}

function gitBlobSha1(bytes) {
  const prefix = Buffer.from(`blob ${bytes.length}\0`, "utf8");
  return createHash("sha1").update(prefix).update(bytes).digest("hex");
}

async function installSeccompProfile() {
  const response = await fetch(SECCOMP_URL, { redirect: "error", signal: AbortSignal.timeout(15_000) });
  if (!response.ok) throw new Error(`failed to fetch pinned Playwright seccomp profile: HTTP ${response.status}`);
  const upstreamBytes = Buffer.from(await response.arrayBuffer());
  const identity = gitBlobSha1(upstreamBytes);
  if (identity !== SECCOMP_GIT_BLOB) {
    throw new Error(`pinned Playwright seccomp profile identity mismatch: ${identity}`);
  }
  const upstream = JSON.parse(upstreamBytes.toString("utf8"));
  const derived = deriveNoonPreviewSeccompProfile(upstream);
  const bytes = Buffer.from(`${JSON.stringify(derived, null, 2)}\n`, "utf8");
  await writeFile(seccompPath, bytes, { mode: 0o600 });
  await chmod(seccompPath, 0o600);
  return createHash("sha256").update(bytes).digest("hex");
}

await mkdir(cacheDir, { recursive: true, mode: 0o700 });
await chmod(cacheDir, 0o700);
await run("docker", ["version", "--format", "{{.Server.Version}}"]).catch((error) => {
  throw new Error(`Docker with a reachable local daemon is required: ${error.message}`);
});
const seccompProfileSha256 = await installSeccompProfile();
await run("docker", ["build", "--pull", "--tag", IMAGE_TAG, "--file", "preview.Dockerfile", "."], { cwd: packageRoot });
const imageId = await run("docker", ["image", "inspect", "--format", "{{.Id}}", IMAGE_TAG]);
if (!/^sha256:[0-9a-f]{64}$/.test(imageId)) throw new Error("Docker did not return a content-addressed preview image ID");

const record = {
  schemaVersion: 1,
  imageId,
  seccompProfile: seccompPath,
  playwrightVersion: PLAYWRIGHT_VERSION,
  seccompGitBlob: SECCOMP_GIT_BLOB,
  seccompProfileSha256,
};
await writeFile(runtimePath, `${JSON.stringify(record, null, 2)}\n`, { mode: 0o600 });
await chmod(runtimePath, 0o600);
const persisted = JSON.parse(await readFile(runtimePath, "utf8"));
if (persisted.imageId !== imageId || persisted.seccompProfileSha256 !== seccompProfileSha256) {
  throw new Error("preview runtime config verification failed");
}
console.log(JSON.stringify({ runtimeConfig: runtimePath, imageId, seccompProfile: seccompPath, seccompProfileSha256 }));

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstat, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const RUNTIME_BUILD_IDENTITY_SCHEMA = 1;
export const RUNTIME_BUILD_IDENTITY_PATH = "web/runtime-build-identity.json";

const runtimeFiles = Object.freeze({
  worker: Object.freeze({ path: "./python-worker.js", diskPath: "web/python-worker.js" }),
  wasm: Object.freeze({ path: "./pkg/noon_web_bg.wasm", diskPath: "web/pkg/noon_web_bg.wasm" }),
  glue: Object.freeze({ path: "./pkg/noon_web.js", diskPath: "web/pkg/noon_web.js" }),
});

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

async function fileSha256(root, name) {
  const filename = path.join(root, name);
  const info = await lstat(filename);
  if (!info.isFile() || info.isSymbolicLink()) {
    throw new Error(`runtime build input is not a regular file: ${name}`);
  }
  return sha256(await readFile(filename));
}

export function observedSourceRevision(root) {
  const status = spawnSync("git", ["status", "--porcelain", "--untracked-files=all"], {
    cwd: root,
    encoding: "utf8",
  });
  if (status.status !== 0 || status.stdout.trim() !== "") return null;
  const revision = spawnSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" });
  const value = revision.status === 0 ? revision.stdout.trim() : "";
  return /^[0-9a-f]{40}$/.test(value) ? value : null;
}

export function runtimeBuildIdentityCore({ sourceRevision, hashes }) {
  if (sourceRevision !== null && !/^[0-9a-f]{40}$/.test(sourceRevision)) {
    throw new TypeError("runtime source revision must be a full Git SHA or null");
  }
  for (const key of Object.keys(runtimeFiles)) {
    if (!/^[0-9a-f]{64}$/.test(hashes?.[key] ?? "")) {
      throw new TypeError(`runtime ${key} hash must be SHA-256`);
    }
  }
  return {
    schema: RUNTIME_BUILD_IDENTITY_SCHEMA,
    sourceRevision,
    files: {
      worker: { path: runtimeFiles.worker.path, sha256: hashes.worker },
      wasm: { path: runtimeFiles.wasm.path, sha256: hashes.wasm },
      glue: { path: runtimeFiles.glue.path, sha256: hashes.glue },
    },
  };
}

export async function createRuntimeBuildIdentity(root = process.cwd()) {
  const hashes = {};
  for (const [key, descriptor] of Object.entries(runtimeFiles)) {
    hashes[key] = await fileSha256(root, descriptor.diskPath);
  }
  const core = runtimeBuildIdentityCore({ sourceRevision: observedSourceRevision(root), hashes });
  return Object.freeze({ ...core, buildId: sha256(JSON.stringify(core)) });
}

export async function writeRuntimeBuildIdentity(root = process.cwd()) {
  const identity = await createRuntimeBuildIdentity(root);
  await writeFile(
    path.join(root, RUNTIME_BUILD_IDENTITY_PATH),
    `${JSON.stringify(identity, null, 2)}\n`,
    "utf8",
  );
  return identity;
}

async function main() {
  const root = process.argv[2] ? path.resolve(process.argv[2]) : process.cwd();
  const identity = await writeRuntimeBuildIdentity(root);
  console.log(`✓ runtime build ${identity.buildId}${identity.sourceRevision ? ` from ${identity.sourceRevision}` : " (source revision unavailable)"}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}

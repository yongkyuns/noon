// Temporary publication gate owned by #2056. Remove this runner and its candidate
// metadata when the reviewed poster/evidence and entry join the public manifest.
// Existing public catalog captures and evidence must survive this isolated run.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeShowcaseManifest } from "../web/showcase-gallery.js";

async function optionalRead(filename) {
  try { return await readFile(filename); }
  catch (error) { if (error.code === "ENOENT") return null; throw error; }
}

export async function qualifyTriangleLight(root, {
  backend = process.env.NOON_SHOWCASE_BACKEND ?? "webgl",
  run = (script) => execFileSync(process.execPath, [path.join(root, "scripts", script)], {
    cwd: root, stdio: "inherit", env: { ...process.env, NOON_SHOWCASE_BACKEND: backend },
  }),
} = {}) {
  assert.ok(["webgl", "webgpu"].includes(backend), "unknown capture backend");
  const manifestPath = path.join(root, "web/python/examples/noon_showcase_manifest.json");
  const evidencePath = path.join(root, "web/thumbnails/showcase/capture-evidence.json");
  const posterPath = path.join(root, "web/thumbnails/showcase/showcase-triangle-light.png");
  const entry = JSON.parse(await readFile(path.join(root,
    "web/python/examples/showcase_triangle_light.candidate.json"), "utf8"));
  const files = [manifestPath, evidencePath, posterPath];
  const originals = await Promise.all(files.map(optionalRead));
  assert.ok(originals[0], "public manifest missing");
  const published = JSON.parse(originals[0]);
  assert.ok(!published.entries.some(item => item.id === entry.id),
    "candidate is already public; retire the #2056 bootstrap instead of recapturing it");
  const candidate = { ...published, entries: [entry] };
  normalizeShowcaseManifest(candidate);

  const artifactsRoot = path.join(root, "browser-smoke-artifacts");
  const output = path.join(artifactsRoot, "showcase", backend);
  const result = path.join(artifactsRoot, "triangle-light", backend);
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(path.dirname(result), { recursive: true });
  const temporary = await mkdtemp(path.join(artifactsRoot, ".triangle-light-"));
  const previous = path.join(temporary, "previous-captures");
  let hadOutput = false;
  try { await rename(output, previous); hadOutput = true; }
  catch (error) { if (error.code !== "ENOENT") throw error; }
  try {
    await writeFile(manifestPath, JSON.stringify(candidate, null, 2) + "\n");
    // Use both existing qualification paths, not an imitation renderer or a
    // weaker interpretation of replay. Neither command publishes or commits.
    await run("showcase-capture.mjs");
    await run("showcase-live-review.mjs");
  } finally {
    try {
      // A fresh CI checkout has no result directory. Never overwrite old review
      // evidence: rename fails on an existing nonempty directory.
      try { await rename(output, result); }
      catch (error) { if (error.code !== "ENOENT") throw error; }
    } finally {
      for (let i = 0; i < files.length; i++) {
        if (originals[i] === null) await rm(files[i], { force: true });
        else await writeFile(files[i], originals[i]);
      }
      if (hadOutput) await rename(previous, output);
      await rm(temporary, { recursive: true, force: true });
    }
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await qualifyTriangleLight(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."));
}

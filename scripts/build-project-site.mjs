import assert from "node:assert/strict";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// Stage, never rewrite, the tested playground. Relative module/worker URLs stay
// in the same directory, including under a GitHub project prefix such as /noon/.
export async function buildProjectSite(root) {
  const web = path.join(root, "web");
  const output = path.join(root, "_site");
  const examples = JSON.parse(await readFile(path.join(web, "site/examples.json"), "utf8"));
  for (const { id } of examples) {
    assert.match(id, /^showcase-[a-z0-9-]+$/);
    const gif = await readFile(path.join(web, "site/media", `${id}.gif`));
    const mp4 = await readFile(path.join(web, "site/media", `${id}.mp4`));
    assert.match(gif.subarray(0, 6).toString(), /^GIF8[79]a$/, `${id}: missing/invalid GIF`);
    assert.equal(mp4.subarray(4, 8).toString(), "ftyp", `${id}: missing/invalid MP4`);
  }
  await readFile(path.join(web, "site/media/provenance.json"));
  const playground = await readFile(path.join(web, "index.html"));
  const homepage = await readFile(path.join(web, "site/index.html"));
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  await cp(web, output, { recursive: true });
  await writeFile(path.join(output, "playground.html"), playground);
  await writeFile(path.join(output, "index.html"), homepage);
  await writeFile(path.join(output, ".nojekyll"), "");
  await rm(path.join(output, "site/index.html"));
  return output;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  console.log(await buildProjectSite(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")));
}

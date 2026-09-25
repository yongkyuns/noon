import { loadLatexAssets } from "./assets.js";
import { createTexEngine } from "./engine-io.js";

// Explicit preparation is host work. The compiler is absent until requested,
// and never participates in retained rendering or per-frame evaluation.
export async function prepareLatexBackend(fetchAsset = fetch) {
  const { module, format, files, fonts, identity, licenses } = await loadLatexAssets(fetchAsset);
  const engine = createTexEngine(module, format, files);
  return {
    identity,
    licenses,
    snapshotBytes: engine.snapshotBytes,
    compile: document => engine.compile(document),
    font(name) {
      const tfm = files.get(`${name}.tfm`);
      const ttf = fonts.get(name);
      if (!tfm || !ttf) throw new Error(`Unsupported LaTeX font: ${name}`);
      return { tfm, ttf, faceKey: `${identity}:${name}` };
    },
  };
}

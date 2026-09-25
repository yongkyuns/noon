// Optional compiler data only. Nothing here is fetched during ordinary startup.
// Pin the complete distribution, including engine, format, packages and fonts.
export const LATEX_ASSETS = Object.freeze({
  version: "node-tikzjax-1.0.5",
  bundle: "https://registry.npmjs.org/node-tikzjax/-/node-tikzjax-1.0.5.tgz",
  bundleSha256: "8618a24afb93c70a32a3fbe8051677edfcf11a52aa681b6b7435fba7c8150eb9",
  metrics: "https://cdn.jsdelivr.net/npm/@prinsss/dvi2html@0.0.1/lib/tfm/fonts.json",
  metricsSha256: "47501544fbf1a3ffc95346bb12b98e8c89ccee604d57f3092b35c8136e05cd76",
});
const decoder = new TextDecoder();
const MIB = 1024 * 1024;

export async function readBounded(stream, maximum) {
  const reader = stream.getReader();
  const chunks = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > maximum) throw new Error(`LaTeX asset exceeds ${maximum} bytes`);
      chunks.push(value);
    }
  } catch (error) {
    await reader.cancel(error);
    throw error;
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.length; }
  return result;
}

export async function gunzipBounded(bytes, maximum) {
  return readBounded(new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip")), maximum);
}

async function verifiedFetch(url, expected, maximum, fetchAsset) {
  const response = await fetchAsset(url);
  if (!response.ok || !response.body) throw new Error(`Cannot load LaTeX asset: HTTP ${response.status}`);
  const bytes = await readBounded(response.body, maximum);
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const actual = Array.from(hash, byte => byte.toString(16).padStart(2, "0")).join("");
  if (actual !== expected) throw new Error("LaTeX asset integrity mismatch");
  return bytes;
}

// Extract only requested regular files into independent buffers. Subarray views
// would otherwise keep the entire package archive alive after preparation.
export function tarFiles(bytes, include = () => true) {
  const files = new Map();
  function text(block, offset, length) {
    const field = block.subarray(offset, offset + length);
    const end = field.indexOf(0);
    return decoder.decode(end < 0 ? field : field.subarray(0, end));
  }
  function octal(value) {
    const clean = value.trim();
    if (!/^[0-7]+$/.test(clean)) throw new Error("Invalid LaTeX tar numeric field");
    return Number.parseInt(clean, 8);
  }
  for (let offset = 0; offset + 512 <= bytes.length;) {
    const block = bytes.subarray(offset, offset + 512);
    if (block.every(byte => byte === 0)) return files;
    const checksum = block.reduce((sum, byte, index) => sum + (index >= 148 && index < 156 ? 32 : byte), 0);
    if (checksum !== octal(text(block, 148, 8))) throw new Error("Invalid LaTeX tar checksum");
    const prefix = text(block, 345, 155);
    const name = `${prefix ? `${prefix}/` : ""}${text(block, 0, 100)}`.replace(/^\.\//, "");
    if (name.startsWith("/") || name.split("/").includes("..")) throw new Error("Invalid LaTeX asset path");
    const size = octal(text(block, 124, 12));
    const start = offset + 512;
    const end = start + size;
    if (end > bytes.length) throw new Error("Truncated LaTeX asset archive");
    if ((block[156] === 0 || block[156] === 48) && include(name)) {
      if (files.has(name)) throw new Error("Duplicate LaTeX asset path");
      files.set(name, bytes.slice(start, end));
    }
    offset = start + Math.ceil(size / 512) * 512;
  }
  throw new Error("Truncated LaTeX tar terminator");
}

export async function loadLatexAssets(fetchAsset = fetch) {
  const [bundle, metrics] = await Promise.all([
    verifiedFetch(LATEX_ASSETS.bundle, LATEX_ASSETS.bundleSha256, 8 * MIB, fetchAsset),
    verifiedFetch(LATEX_ASSETS.metrics, LATEX_ASSETS.metricsSha256, MIB, fetchAsset),
  ]);
  const packageFiles = tarFiles(await gunzipBounded(bundle, 32 * MIB), name =>
    name.startsWith("package/tex/") || name.startsWith("package/css/bakoma/ttf/") ||
    name === "package/LICENSE" || name === "package/css/bakoma/LICENCE");
  function required(name) {
    const file = packageFiles.get(name);
    if (!file) throw new Error(`Missing LaTeX package resource: ${name}`);
    return file;
  }
  const [wasm, format, texArchive] = await Promise.all([
    gunzipBounded(required("package/tex/tex.wasm.gz"), 2 * MIB),
    gunzipBounded(required("package/tex/core.dump.gz"), 72 * MIB),
    gunzipBounded(required("package/tex/tex_files.tar.gz"), 16 * MIB),
  ]);
  const files = tarFiles(texArchive);
  for (const [name, encoded] of Object.entries(JSON.parse(decoder.decode(metrics)))) {
    if (!/^[\w-]+$/.test(name) || typeof encoded !== "string") throw new Error("Invalid TeX font metric resource");
    const binary = atob(encoded);
    files.set(`${name}.tfm`, Uint8Array.from(binary, char => char.charCodeAt(0)));
  }
  const fonts = new Map();
  for (const [path, bytes] of packageFiles) {
    if (path.startsWith("package/css/bakoma/ttf/") && path.endsWith(".ttf")) {
      fonts.set(path.slice(path.lastIndexOf("/") + 1, -4), bytes);
    }
  }
  return {
    module: await WebAssembly.compile(wasm), format, files, fonts,
    identity: `${LATEX_ASSETS.version}:${LATEX_ASSETS.bundleSha256}:${LATEX_ASSETS.metricsSha256}`,
    licenses: {
      engine: decoder.decode(required("package/LICENSE")),
      fonts: decoder.decode(required("package/css/bakoma/LICENCE")),
    },
  };
}


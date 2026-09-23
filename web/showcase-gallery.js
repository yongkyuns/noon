export const SHOWCASE_MANIFEST = "./python/examples/noon_showcase_manifest.json";

export function isShowcaseRequest(locationLike) {
  const params = new URLSearchParams(locationLike?.search ?? "");
  return params.get("catalog") === "showcase" ||
    (params.get("catalog") !== "reference" && (params.get("example") ?? "").startsWith("showcase-"));
}

export function normalizeShowcaseManifest(manifest) {
  if (manifest?.version !== 1 || manifest.publication !== "preview" || !Array.isArray(manifest.entries)) {
    throw new TypeError("Expected a version-1 preview showcase manifest");
  }
  const ids = new Set();
  const sources = new Set();
  const primaryFeatures = new Set();
  const examples = manifest.entries.map((entry) => {
    for (const field of ["id", "title", "lesson", "primary_feature", "category", "path", "thumbnail", "thumbnail_alt"]) {
      if (typeof entry[field] !== "string" || entry[field].trim() === "") {
        throw new TypeError(`Showcase entry requires ${field}`);
      }
    }
    if (!/^showcase-[a-z0-9-]+$/.test(entry.id) || ids.has(entry.id)) throw new Error(`Invalid or duplicate showcase id: ${entry.id}`);
    if (!/^python\/examples\/showcase_[a-z_]+\.py$/.test(entry.path) || sources.has(entry.path)) throw new Error(`${entry.id}: expected a unique local showcase source`);
    if (primaryFeatures.has(entry.primary_feature)) throw new Error(`${entry.id}: duplicated primary learning outcome`);
    if (entry.thumbnail !== `thumbnails/showcase/${entry.id}.png`) throw new Error(`${entry.id}: poster must be a local captured PNG`);
    if (!Number.isFinite(entry.duration) || entry.duration <= 0) throw new Error(`${entry.id}: duration must be positive`);
    if (!Number.isFinite(entry.thumbnail_time) || entry.thumbnail_time <= 0 || entry.thumbnail_time > entry.duration) throw new Error(`${entry.id}: invalid poster time`);
    if (!Array.isArray(entry.features) || entry.features.length === 0 || entry.features.some((feature) => typeof feature !== "string" || !feature.trim())) throw new Error(`${entry.id}: expected feature tags`);
    if (!Array.isArray(entry.beats) || entry.beats.length < 3 || entry.beats.some((beat) =>
      !Number.isFinite(beat.time) || beat.time <= 0 || beat.time > entry.duration || typeof beat.label !== "string" || !beat.label.trim()
    )) throw new Error(`${entry.id}: expected at least three review beats within the scene`);
    if (entry.interaction != null && entry.interaction.type !== "pointer-fill-selection") throw new Error(`${entry.id}: unsupported host interaction`);
    if (entry.interaction != null && (typeof entry.host_setup !== "string" || !entry.host_setup.trim())) throw new Error(`${entry.id}: interactive scenes must disclose their host setup`);
    ids.add(entry.id);
    sources.add(entry.path);
    primaryFeatures.add(entry.primary_feature);
    return {
      id: entry.id,
      title: entry.title,
      summary: entry.lesson + (entry.host_setup ? ` Host setup: ${entry.host_setup}` : ""),
      path: `./${entry.path}`,
      category: entry.category,
      features: [...entry.features],
      upstream: null,
      reuse: "noon-authored-showcase",
      parityStatus: "noon-showcase",
      parityFixture: null,
      thumbnail: `./${entry.thumbnail}`,
      thumbnailAlt: entry.thumbnail_alt,
      thumbnailTime: entry.thumbnail_time,
      order: examplesOrder(ids),
      interaction: entry.interaction ? { type: entry.interaction.type, maxMovement: 4 } : null,
      performance: entry.performance === true,
    };
  });
  return { reference: null, examples };
}

function examplesOrder(ids) {
  return ids.size;
}

export async function loadShowcaseGallery(fetchImpl = globalThis.fetch) {
  if (typeof fetchImpl !== "function") throw new TypeError("Showcase loading requires fetch");
  const response = await fetchImpl(SHOWCASE_MANIFEST);
  if (!response.ok) throw new Error(`Unable to load showcase manifest: HTTP ${response.status}`);
  return normalizeShowcaseManifest(await response.json());
}

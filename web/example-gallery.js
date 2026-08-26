const MANIM_REUSE = "source-equivalent-manim-v0.21";
const READY_PARITY = new Set(["candidate", "parity-qualified"]);

export function normalizeGalleryManifest(manifest) {
  if (!manifest || !Array.isArray(manifest.entries)) {
    throw new TypeError("Manim example manifest must contain an entries array");
  }

  const seen = new Set();
  const examples = [];
  for (const entry of manifest.entries) {
    if (!entry || typeof entry.id !== "string" || entry.id.trim() === "") {
      throw new TypeError("Every Manim example entry requires a stable non-empty id");
    }
    if (seen.has(entry.id)) {
      throw new Error(`Duplicate Manim example id ${entry.id}`);
    }
    seen.add(entry.id);

    if (entry.status !== "ready") {
      continue;
    }
    if (entry.reuse !== MANIM_REUSE) {
      throw new Error(
        `${entry.id}: runnable gallery examples must be source-equivalent ManimCE v0.21 scenes`,
      );
    }
    if (!READY_PARITY.has(entry.parity_status)) {
      throw new Error(`${entry.id}: runnable gallery example needs candidate/qualified parity status`);
    }
    if (typeof entry.path !== "string" || entry.path.trim() === "") {
      throw new Error(`${entry.id}: runnable gallery example requires a source path`);
    }
    if (typeof entry.thumbnail !== "string" || entry.thumbnail.trim() === "") {
      throw new Error(`${entry.id}: runnable gallery example requires a static thumbnail`);
    }
    if (!Array.isArray(entry.features) || entry.features.length === 0) {
      throw new Error(`${entry.id}: runnable gallery example requires feature tags`);
    }

    examples.push({
      id: entry.id,
      title: entry.title,
      summary: entry.summary ?? "Source-equivalent ManimCE v0.21 example.",
      path: `./${entry.path}`,
      category: entry.category ?? "manim",
      features: [...entry.features],
      upstream: entry.upstream ?? null,
      parityStatus: entry.parity_status,
      parityFixture: entry.parity_fixture ?? null,
      thumbnail: `./${entry.thumbnail}`,
      thumbnailAlt: entry.thumbnail_alt ?? `${entry.title} poster frame`,
      thumbnailTime: Number(entry.thumbnail_time ?? 0),
      order: Number(entry.order ?? 0),
    });
  }

  examples.sort((a, b) => a.order - b.order || a.title.localeCompare(b.title));
  return {
    reference: manifest.reference ?? null,
    examples,
  };
}

export async function loadGalleryManifest(
  url = "./python/examples/manim_tutorial_manifest.json",
  fetchImpl = globalThis.fetch,
) {
  if (typeof fetchImpl !== "function") {
    throw new TypeError("loadGalleryManifest requires fetch");
  }
  const response = await fetchImpl(url);
  if (!response.ok) {
    throw new Error(`Unable to load Manim example manifest: HTTP ${response.status}`);
  }
  return normalizeGalleryManifest(await response.json());
}

export function galleryCategories(examples) {
  return [...new Set(examples.map((example) => example.category))].sort();
}

export function filterGalleryExamples(
  examples,
  { query = "", category = "all", parityStatus = "all" } = {},
) {
  const needle = query.trim().toLocaleLowerCase();
  return examples.filter((example) => {
    if (category !== "all" && example.category !== category) {
      return false;
    }
    if (parityStatus !== "all" && example.parityStatus !== parityStatus) {
      return false;
    }
    if (needle === "") {
      return true;
    }
    const haystack = [
      example.title,
      example.summary,
      example.category,
      example.parityStatus,
      ...example.features,
    ]
      .join(" ")
      .toLocaleLowerCase();
    return haystack.includes(needle);
  });
}

export function requestedExampleId(locationLike = globalThis.location) {
  if (!locationLike) return null;
  const params = new URLSearchParams(locationLike.search ?? "");
  const id = params.get("example");
  return id && id.trim() !== "" ? id : null;
}

export function exampleUrl(id, locationLike = globalThis.location) {
  const href = locationLike?.href ?? "http://localhost/";
  const url = new URL(href);
  url.searchParams.set("example", id);
  return `${url.pathname}${url.search}${url.hash}`;
}

export function parityLabel(status) {
  return status === "parity-qualified" ? "Parity qualified" : "Parity candidate";
}

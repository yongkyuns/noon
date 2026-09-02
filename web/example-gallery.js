const MANIM_REUSE = "source-equivalent-manim-v0.21";
const MANIM_PARITY_REUSE = "manim-compatible-parity-v0.21";
const READY_REUSE = new Set([MANIM_REUSE, MANIM_PARITY_REUSE]);
const READY_PARITY = new Set(["candidate", "parity-qualified"]);
const THUMBNAIL_FALLBACK_MARKER = "noonThumbnailFallbackInstalled";
const DEFAULT_GALLERY_MANIFEST = "./python/examples/manim_tutorial_manifest.json";
const STRESS_GALLERY_MANIFEST = "./python/examples/manim_stress_manifest.json";

if (typeof document !== "undefined") {
  installGalleryThumbnailFallback(document);
}

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
    if (!READY_REUSE.has(entry.reuse)) {
      throw new Error(
        `${entry.id}: runnable gallery examples must be source-equivalent ManimCE v0.21 scenes or custom Manim-compatible parity workloads`,
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
      reuse: entry.reuse,
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

async function fetchGalleryManifest(url, fetchImpl) {
  const response = await fetchImpl(url);
  if (!response.ok) {
    throw new Error(`Unable to load Manim example manifest ${url}: HTTP ${response.status}`);
  }
  return response.json();
}

export async function loadGalleryManifest(
  url = DEFAULT_GALLERY_MANIFEST,
  fetchImpl = globalThis.fetch,
) {
  if (typeof fetchImpl !== "function") {
    throw new TypeError("loadGalleryManifest requires fetch");
  }

  const manifest = await fetchGalleryManifest(url, fetchImpl);
  if (url !== DEFAULT_GALLERY_MANIFEST) {
    return normalizeGalleryManifest(manifest);
  }

  const stressManifest = await fetchGalleryManifest(STRESS_GALLERY_MANIFEST, fetchImpl);
  const referenceVersion = manifest.reference?.version ?? null;
  const stressVersion = stressManifest.reference?.version ?? null;
  if (stressVersion !== referenceVersion) {
    throw new Error(
      `Manim stress manifest version ${stressVersion ?? "unknown"} does not match gallery version ${referenceVersion ?? "unknown"}`,
    );
  }

  return normalizeGalleryManifest({
    ...manifest,
    entries: [...manifest.entries, ...stressManifest.entries],
  });
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

export function applyGalleryThumbnailFailure(image, documentLike = globalThis.document) {
  if (
    !image ||
    image.tagName !== "IMG" ||
    !image.classList?.contains("example-thumb") ||
    image.dataset?.thumbnailFailed === "true" ||
    typeof documentLike?.createElement !== "function"
  ) {
    return false;
  }

  const alt = typeof image.alt === "string" && image.alt.trim() !== "" ? image.alt.trim() : "Example";
  const fallback = documentLike.createElement("span");
  fallback.className = "example-thumb-fallback";
  fallback.setAttribute("role", "img");
  fallback.setAttribute("aria-label", `${alt} — preview unavailable`);
  fallback.textContent = "Preview unavailable";

  image.dataset.thumbnailFailed = "true";
  image.hidden = true;
  image.alt = "";
  image.after(fallback);
  return true;
}

export function installGalleryThumbnailFallback(documentLike = globalThis.document) {
  if (
    !documentLike?.documentElement?.dataset ||
    typeof documentLike.addEventListener !== "function" ||
    typeof documentLike.createElement !== "function"
  ) {
    return false;
  }
  if (documentLike.documentElement.dataset[THUMBNAIL_FALLBACK_MARKER] === "true") {
    return false;
  }
  documentLike.documentElement.dataset[THUMBNAIL_FALLBACK_MARKER] = "true";

  documentLike.addEventListener(
    "error",
    (event) => {
      applyGalleryThumbnailFailure(event.target, documentLike);
    },
    true,
  );

  const style = documentLike.createElement("style");
  style.dataset.galleryThumbnailFallback = "true";
  style.textContent = `
    .example-thumb-fallback {
      display: grid;
      width: 100%;
      aspect-ratio: 16 / 9;
      place-items: center;
      border-bottom: 1px solid #222d41;
      background:
        radial-gradient(circle at 50% 45%, rgb(142 124 255 / 12%), transparent 45%),
        #111722;
      color: #77849d;
      font: 0.66rem ui-monospace, SFMono-Regular, Menlo, monospace;
      letter-spacing: 0.02em;
    }
  `;
  documentLike.head?.append(style);
  return true;
}

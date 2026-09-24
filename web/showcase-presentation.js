// Browser chrome only: reuse the playground's counters, never create another sampler or runtime.
export function installShowcasePresentation(documentLike, examples, showcase) {
  const topbar = documentLike.querySelector(".topbar");
  if (!topbar || documentLike.getElementById("showcase-catalog-link")) return false;
  documentLike.documentElement.dataset.noonCatalog = showcase ? "showcase" : "reference";
  const link = documentLike.createElement("a");
  link.id = "showcase-catalog-link";
  link.className = "secondary-button";
  link.href = showcase ? "./?catalog=reference" : "./?catalog=showcase";
  link.textContent = showcase ? "API reference" : "Showcase preview";
  topbar.append(link);

  const style = documentLike.createElement("style");
  style.textContent = `
    #showcase-catalog-link { text-decoration: none; white-space: nowrap; font-size: .7rem; }
    .example-browser-layer .example-thumb { object-fit: contain; }
    html[data-noon-catalog="showcase"] .selected-example { display: flex !important; }
    html[data-noon-catalog="showcase"] .selected-tag.parity,
    html[data-noon-catalog="showcase"] .example-browser-more-filters { display: none; }
    .metrics.showcase-live-metrics { display: flex !important; gap: 1rem; flex-wrap: wrap; padding: .75rem; border-top: 1px solid var(--border); }
    .showcase-live-metrics label { display: grid; gap: .15rem; color: var(--muted); font-size: .7rem; }
    .showcase-live-metrics output { color: var(--accent); font-variant-numeric: tabular-nums; }
  `;
  documentLike.head.append(style);
  if (!showcase) return true;

  const metrics = documentLike.querySelector(".metrics");
  const status = documentLike.querySelector("#patch-status");
  if (!metrics || !status || typeof MutationObserver !== "function") return true;
  for (const [id, text] of [["metric-objects", "Visible objects"], ["metric-draws", "Draw calls"], ["metric-upload", "Uploaded bytes"], ["metric-time", "Scene time"]]) {
    const output = documentLike.getElementById(id);
    if (!output) continue;
    const label = documentLike.createElement("label");
    label.textContent = text;
    output.replaceWith(label);
    label.append(output);
  }
  metrics.title = "Live renderer observations. These are not isolated CPU/GPU timings or a hardware-maximum claim.";
  const refresh = () => {
    const enabled = examples.some((entry) => entry.id === status.dataset.exampleId && entry.performance);
    metrics.hidden = !enabled;
    metrics.setAttribute("aria-hidden", String(!enabled));
    metrics.classList.toggle("showcase-live-metrics", enabled);
  };
  const observer = new MutationObserver(refresh);
  observer.observe(status, { attributes: true, attributeFilter: ["data-example-id"] });
  refresh();
  globalThis.addEventListener?.("pagehide", () => observer.disconnect(), { once: true });
  return true;
}

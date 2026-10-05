// Browser chrome only: reuse the playground's counters, never create another sampler or runtime.
export function installShowcasePresentation(documentLike, showcase) {
  const topbar = documentLike.querySelector(".topbar");
  if (!topbar || documentLike.getElementById("showcase-catalog-link")) return false;
  documentLike.documentElement.dataset.noonCatalog = showcase ? "showcase" : "reference";
  const link = documentLike.createElement("a");
  link.id = "showcase-catalog-link";
  link.className = "secondary-button";
  link.href = showcase ? "?catalog=reference" : "?catalog=showcase";
  link.textContent = showcase ? "API reference" : "Showcase";
  topbar.append(link);

  const style = documentLike.createElement("style");
  style.textContent = `
    #showcase-catalog-link { text-decoration: none; white-space: nowrap; font-size: .7rem; }
    @media (max-width: 44rem) {
      html[data-noon-catalog] .topbar {
        display: grid;
        grid-template-columns: auto minmax(8rem, 1fr) auto;
        grid-template-areas: "brand picker catalog" "status status status";
        gap: .4rem .5rem;
        align-items: center;
      }
      html[data-noon-catalog] .topbar .brand { grid-area: brand; }
      html[data-noon-catalog] #example-browser-trigger {
        grid-area: picker;
        min-width: 8rem;
        max-width: none;
      }
      html[data-noon-catalog] #showcase-catalog-link {
        grid-area: catalog;
        justify-self: end;
        min-width: 0;
        max-width: 6.5rem;
        white-space: normal;
        line-height: 1.2;
        text-align: center;
      }
      html[data-noon-catalog] .runtime-status {
        grid-area: status;
        width: 100%;
        max-width: none;
        overflow: visible;
        flex-wrap: wrap;
        padding: .2rem 0 0;
      }
      html[data-noon-catalog] #status-text {
        overflow: visible;
        overflow-wrap: anywhere;
        text-overflow: clip;
        white-space: normal;
      }
    }
    .example-browser-layer .example-thumb { object-fit: contain; }
    html[data-noon-catalog="showcase"] .selected-example { display: flex !important; }
    html[data-noon-catalog="showcase"] .selected-tag.parity,
    html[data-noon-catalog="showcase"] .example-browser-more-filters { display: none; }
    .metrics.catalog-live-metrics { display: flex !important; flex: none; gap: 1rem; flex-wrap: wrap; padding: .75rem; border-top: 1px solid var(--border); }
    .catalog-live-metrics label { display: grid; gap: .15rem; color: var(--muted); font-size: .7rem; }
    .catalog-live-metrics output { color: var(--accent); font-variant-numeric: tabular-nums; }
  `;
  documentLike.head.append(style);
  const metrics = documentLike.querySelector(".metrics");
  if (!metrics) return true;
  for (const [id, text, title] of [
    ["metric-fps", "FPS · target 60", "Approximate renderer presentations per second over about one second. Static holds can show 0. This is not physical display refresh or GPU timing."],
    ["metric-frame-gap", "Frame gap · p95 / max", "Renderer submission intervals during continuous animation. The 60 FPS target is 16.7 ms. This is not physical display scanout."],
    ["metric-objects", "Visible objects"], ["metric-draws", "Draw calls"], ["metric-upload", "Uploaded bytes"], ["metric-time", "Scene time"],
  ]) {
    const output = documentLike.getElementById(id);
    if (!output) continue;
    const label = documentLike.createElement("label");
    label.textContent = text;
    if (title) label.title = title;
    output.replaceWith(label);
    label.append(output);
  }
  metrics.hidden = false;
  metrics.setAttribute("aria-hidden", "false");
  metrics.classList.add("catalog-live-metrics");
  return true;
}

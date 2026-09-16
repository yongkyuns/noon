export const TUTORIALS = Object.freeze([
  Object.freeze({
    id: "ins-gnss",
    title: "INS + GNSS",
    eyebrow: "Navigation tutorial",
    summary:
      "A 14-chapter worked tutorial from sensor errors through aided navigation, with numerical Kalman-filter examples and attitude-error intuition.",
    href: "./tutorials/ins-gnss/index.html",
    badge: "14 chapters",
  }),
]);

const DISCOVERY_MARKER = "data-noon-tutorial-discovery";

function appendTextElement(documentLike, parent, tagName, className, text) {
  const element = documentLike.createElement(tagName);
  element.className = className;
  element.textContent = text;
  parent.append(element);
  return element;
}

export function installTutorialDiscovery(documentLike = globalThis.document) {
  if (
    typeof documentLike?.createElement !== "function" ||
    typeof documentLike?.querySelector !== "function"
  ) {
    return false;
  }
  if (documentLike.querySelector(`[${DISCOVERY_MARKER}]`)) {
    return false;
  }

  const workspace = documentLike.querySelector(".workspace");
  if (!workspace || typeof workspace.before !== "function") {
    return false;
  }

  const section = documentLike.createElement("section");
  section.className = "tutorial-discovery";
  section.setAttribute(DISCOVERY_MARKER, "true");
  section.setAttribute("aria-label", "Noon tutorials");

  const heading = documentLike.createElement("div");
  heading.className = "tutorial-discovery-head";
  appendTextElement(documentLike, heading, "strong", "tutorial-discovery-title", "Tutorials");
  appendTextElement(
    documentLike,
    heading,
    "span",
    "tutorial-discovery-subtitle",
    "Long-form Noon demonstrations and worked technical lessons",
  );
  section.append(heading);

  const grid = documentLike.createElement("div");
  grid.className = "tutorial-discovery-grid";
  for (const tutorial of TUTORIALS) {
    const link = documentLike.createElement("a");
    link.className = "tutorial-card";
    link.href = tutorial.href;
    link.setAttribute("data-tutorial-id", tutorial.id);

    const copy = documentLike.createElement("span");
    copy.className = "tutorial-card-copy";
    appendTextElement(documentLike, copy, "span", "tutorial-card-eyebrow", tutorial.eyebrow);
    appendTextElement(documentLike, copy, "strong", "tutorial-card-title", tutorial.title);
    appendTextElement(documentLike, copy, "span", "tutorial-card-summary", tutorial.summary);

    const action = documentLike.createElement("span");
    action.className = "tutorial-card-action";
    appendTextElement(documentLike, action, "span", "tutorial-card-badge", tutorial.badge);
    appendTextElement(documentLike, action, "span", "tutorial-card-open", "Open tutorial →");

    link.append(copy, action);
    grid.append(link);
  }
  section.append(grid);

  const style = documentLike.createElement("style");
  style.setAttribute(DISCOVERY_MARKER, "styles");
  style.textContent = `
    .tutorial-discovery {
      margin-bottom: 1rem;
      overflow: hidden;
      border: 1px solid var(--border);
      border-radius: 1rem;
      background:
        radial-gradient(circle at 85% 0%, rgb(142 124 255 / 12%), transparent 24rem),
        rgb(7 10 16 / 78%);
    }
    .tutorial-discovery-head {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 1rem;
      padding: 0.82rem 1rem;
      border-bottom: 1px solid var(--border);
    }
    .tutorial-discovery-title {
      color: #e4e8f2;
      font-size: 0.86rem;
    }
    .tutorial-discovery-subtitle {
      color: var(--muted-2);
      font-size: 0.68rem;
      text-align: right;
    }
    .tutorial-discovery-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
      gap: 0.72rem;
      padding: 0.85rem;
    }
    .tutorial-card {
      display: flex;
      min-width: 0;
      align-items: center;
      justify-content: space-between;
      gap: 1rem;
      padding: 0.9rem 1rem;
      border: 1px solid #323b5d;
      border-radius: 0.8rem;
      background: linear-gradient(135deg, #111627, #0c111b 65%);
      color: inherit;
      text-decoration: none;
    }
    .tutorial-card:hover {
      border-color: #6f61c9;
      background: linear-gradient(135deg, #171d32, #0d121d 65%);
    }
    .tutorial-card:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .tutorial-card-copy {
      display: block;
      min-width: 0;
    }
    .tutorial-card-eyebrow,
    .tutorial-card-title,
    .tutorial-card-summary,
    .tutorial-card-badge,
    .tutorial-card-open {
      display: block;
    }
    .tutorial-card-eyebrow {
      margin-bottom: 0.22rem;
      color: #9d91ef;
      font: 0.61rem ui-monospace, SFMono-Regular, Menlo, monospace;
      letter-spacing: 0.055em;
      text-transform: uppercase;
    }
    .tutorial-card-title {
      color: #f0f2f8;
      font-size: 0.92rem;
    }
    .tutorial-card-summary {
      max-width: 48rem;
      margin-top: 0.28rem;
      color: #8e9ab2;
      font-size: 0.7rem;
      line-height: 1.45;
    }
    .tutorial-card-action {
      display: flex;
      flex: none;
      flex-direction: column;
      align-items: flex-end;
      gap: 0.35rem;
    }
    .tutorial-card-badge {
      padding: 0.2rem 0.4rem;
      border: 1px solid #514691;
      border-radius: 999px;
      color: #c2b8ff;
      font: 0.6rem ui-monospace, SFMono-Regular, Menlo, monospace;
    }
    .tutorial-card-open {
      color: #d9d4ff;
      font-size: 0.68rem;
      font-weight: 750;
      white-space: nowrap;
    }
    @media (max-width: 44rem) {
      .tutorial-discovery-head {
        align-items: flex-start;
        flex-direction: column;
        gap: 0.2rem;
      }
      .tutorial-discovery-subtitle { text-align: left; }
      .tutorial-discovery-grid { grid-template-columns: 1fr; padding: 0.6rem; }
      .tutorial-card { align-items: flex-start; flex-direction: column; }
      .tutorial-card-action { width: 100%; align-items: flex-start; }
    }
  `;
  documentLike.head?.append(style);
  workspace.before(section);
  return true;
}

if (typeof document !== "undefined") {
  installTutorialDiscovery(document);
}

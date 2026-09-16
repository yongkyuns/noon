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

  const topbar = documentLike.querySelector(".topbar");
  if (!topbar || typeof topbar.append !== "function") {
    return false;
  }

  const nav = documentLike.createElement("nav");
  nav.className = "tutorial-discovery";
  nav.setAttribute(DISCOVERY_MARKER, "true");
  nav.setAttribute("aria-label", "Noon tutorials");

  for (const tutorial of TUTORIALS) {
    const link = documentLike.createElement("a");
    link.className = "tutorial-link";
    link.href = tutorial.href;
    link.setAttribute("data-tutorial-id", tutorial.id);
    link.setAttribute("title", `${tutorial.eyebrow}: ${tutorial.summary}`);

    appendTextElement(documentLike, link, "span", "tutorial-link-prefix", "Tutorial");
    appendTextElement(documentLike, link, "strong", "tutorial-link-title", tutorial.title);
    appendTextElement(documentLike, link, "span", "tutorial-link-badge", tutorial.badge);
    nav.append(link);
  }

  const style = documentLike.createElement("style");
  style.setAttribute(DISCOVERY_MARKER, "styles");
  style.textContent = `
    .tutorial-discovery {
      display: flex;
      min-width: 0;
      flex: none;
      align-items: center;
    }
    .tutorial-link {
      display: flex;
      height: 2rem;
      align-items: center;
      gap: 0.38rem;
      padding: 0 0.58rem;
      border: 1px solid #40386f;
      border-radius: 0.62rem;
      background: rgb(142 124 255 / 8%);
      color: #e7e3ff;
      text-decoration: none;
      white-space: nowrap;
    }
    .tutorial-link:hover {
      border-color: #7061d1;
      background: rgb(142 124 255 / 15%);
    }
    .tutorial-link:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .tutorial-link-prefix {
      color: #9d91ef;
      font: 0.58rem ui-monospace, SFMono-Regular, Menlo, monospace;
      letter-spacing: 0.045em;
      text-transform: uppercase;
    }
    .tutorial-link-title {
      font-size: 0.69rem;
      letter-spacing: -0.01em;
    }
    .tutorial-link-badge {
      padding-left: 0.38rem;
      border-left: 1px solid #40386f;
      color: #8f9ab1;
      font: 0.58rem ui-monospace, SFMono-Regular, Menlo, monospace;
    }
    @media (max-width: 52rem) {
      .tutorial-link-badge { display: none; }
    }
    @media (max-width: 44rem) {
      .tutorial-link {
        height: 1.85rem;
        padding: 0 0.42rem;
      }
      .tutorial-link-prefix { display: none; }
      .tutorial-link-title { font-size: 0.62rem; }
    }
  `;

  documentLike.head?.append(style);
  topbar.append(nav);
  return true;
}

if (typeof document !== "undefined") {
  installTutorialDiscovery(document);
}

// Minimal DOM for host-control unit tests. Browser layout and actual Fullscreen
// API behavior remain covered by the browser smoke tests.
export class Element extends EventTarget {
  constructor(tag = "div") {
    super();
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.dataset = {};
    this.attributes = new Map();
    this.className = "";
    this.value = "";
    this.title = "";
    this.disabled = false;
  }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) {
    this.attributes.delete(name);
    if (name.startsWith("data-")) delete this.dataset[name.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())];
    if (name === "title") this.title = "";
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  querySelector(selector) {
    const matches = (child) => selector.split(",").some((part) => {
      const value = part.trim();
      return value.startsWith(".") ? child.className.split(" ").includes(value.slice(1)) : child.tagName.toLowerCase() === value;
    });
    for (const child of this.children) {
      if (matches(child)) return child;
      const found = child.querySelector(selector);
      if (found) return found;
    }
    return null;
  }
}
export function dom() {
  const document = new EventTarget();
  const preview = new Element("section");
  preview.className = "preview-pane";
  document.head = new Element("head");
  document.createElement = (tag) => new Element(tag);
  document.getElementById = (id) => document.head.children.find((child) => child.id === id) ?? null;
  document.querySelector = (selector) => selector === ".preview-pane" ? preview : null;
  return { document, preview };
}
export const flush = async () => { for (let i = 0; i < 12; i += 1) await Promise.resolve(); };

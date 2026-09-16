import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

import { TUTORIALS, installTutorialDiscovery } from "./tutorial-discovery.js";

assert.ok(TUTORIALS.length > 0, "the playground should expose at least one long-form tutorial");
const insGnss = TUTORIALS.find((tutorial) => tutorial.id === "ins-gnss");
assert.ok(insGnss, "INS/GNSS tutorial must remain discoverable from the main playground");
assert.equal(insGnss.href, "./tutorials/ins-gnss/index.html");
assert.match(insGnss.summary, /Kalman-filter/i);
await access(new URL(insGnss.href, import.meta.url));

const bootstrapSource = await readFile(
  new URL("./python-editor-bootstrap.js", import.meta.url),
  "utf8",
);
assert.match(
  bootstrapSource,
  /^import "\.\/tutorial-discovery\.js";/m,
  "the main playground bootstrap must install tutorial discovery",
);

class FakeElement {
  constructor(tagName) {
    this.tagName = tagName.toUpperCase();
    this.className = "";
    this.textContent = "";
    this.href = "";
    this.children = [];
    this.attributes = new Map();
  }

  append(...children) {
    this.children.push(...children);
  }

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
  }
}

const inserted = [];
const workspace = new FakeElement("section");
workspace.before = (node) => inserted.push(node);
const head = new FakeElement("head");
const fakeDocument = {
  head,
  createElement(tagName) {
    return new FakeElement(tagName);
  },
  querySelector(selector) {
    if (selector === ".workspace") return workspace;
    if (selector === "[data-noon-tutorial-discovery]") {
      return (
        inserted.find((node) => node.attributes?.has("data-noon-tutorial-discovery")) ?? null
      );
    }
    return null;
  },
};

assert.equal(installTutorialDiscovery(fakeDocument), true);
assert.equal(inserted.length, 1, "discovery surface should be inserted exactly once");
assert.equal(inserted[0].className, "tutorial-discovery");
assert.equal(inserted[0].attributes.get("aria-label"), "Noon tutorials");
assert.equal(head.children.length, 1, "tutorial styling should be installed with the surface");

const grid = inserted[0].children.find((child) => child.className === "tutorial-discovery-grid");
assert.ok(grid);
const card = grid.children.find((child) => child.attributes.get("data-tutorial-id") === "ins-gnss");
assert.ok(card, "INS/GNSS tutorial should render as a main-playground tutorial card");
assert.equal(card.tagName, "A");
assert.equal(card.href, "./tutorials/ins-gnss/index.html");
assert.equal(installTutorialDiscovery(fakeDocument), false, "installation must be idempotent");
assert.equal(inserted.length, 1);

console.log("✓ tutorial discovery surface + INS/GNSS route contract");

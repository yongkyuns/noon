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

const topbar = new FakeElement("header");
const head = new FakeElement("head");
const fakeDocument = {
  head,
  createElement(tagName) {
    return new FakeElement(tagName);
  },
  querySelector(selector) {
    if (selector === ".topbar") return topbar;
    if (selector === "[data-noon-tutorial-discovery]") {
      return (
        topbar.children.find((node) => node.attributes?.has("data-noon-tutorial-discovery")) ??
        null
      );
    }
    return null;
  },
};

assert.equal(installTutorialDiscovery(fakeDocument), true);
assert.equal(topbar.children.length, 1, "discovery navigation should be inserted exactly once");
const nav = topbar.children[0];
assert.equal(nav.className, "tutorial-discovery");
assert.equal(nav.attributes.get("aria-label"), "Noon tutorials");
assert.equal(head.children.length, 1, "tutorial styling should be installed with the navigation");

const link = nav.children.find((child) => child.attributes.get("data-tutorial-id") === "ins-gnss");
assert.ok(link, "INS/GNSS tutorial should render as a top-bar tutorial link");
assert.equal(link.tagName, "A");
assert.equal(link.href, "./tutorials/ins-gnss/index.html");
assert.equal(installTutorialDiscovery(fakeDocument), false, "installation must be idempotent");
assert.equal(topbar.children.length, 1);

console.log("✓ tutorial top-bar discovery + INS/GNSS route contract");

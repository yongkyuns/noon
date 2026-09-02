import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("./example-browser-ui.js", import.meta.url), "utf8");

function cssRuleBody(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = source.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`, "m"));
  assert.ok(match, `missing CSS rule for ${selector}`);
  return match[1];
}

test("Examples browser cards bypass deferred painting inside the revealable overlay", () => {
  const body = cssRuleBody(".example-browser-layer .example-card");

  assert.match(
    body,
    /content-visibility\s*:\s*visible\s*;/,
    "overlay cards must opt out of content-visibility:auto so WebKit paints them immediately after reveal",
  );
  assert.match(
    body,
    /contain-intrinsic-size\s*:\s*none\s*;/,
    "overlay cards must not retain the intrinsic-size placeholder used by deferred gallery cards",
  );
});

test("WebKit paint workaround stays scoped to the Examples browser", () => {
  const occurrences = source.match(/\.example-browser-layer\s+\.example-card\s*\{/g) ?? [];

  assert.equal(
    occurrences.length,
    1,
    "keep the eager-paint override scoped to the paginated Examples overlay instead of disabling gallery virtualization globally",
  );
});

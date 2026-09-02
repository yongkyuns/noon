import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./python/noon.py", import.meta.url), "utf8");
const allMatch = source.match(/__all__\s*=\s*\[(?<body>[\s\S]*?)\]\s*$/m);
assert.ok(allMatch?.groups?.body, "noon.py must define a public __all__ export list");

const exported = new Set(
  [...allMatch.groups.body.matchAll(/"([A-Z][A-Z0-9_]*)"/g)].map((match) => match[1]),
);

const manimPaletteNames = [
  "WHITE",
  "BLACK",
  "BLUE_A",
  "BLUE_B",
  "BLUE_C",
  "BLUE_D",
  "BLUE_E",
  "BLUE",
  "TEAL_A",
  "TEAL_B",
  "TEAL_C",
  "TEAL_D",
  "TEAL_E",
  "TEAL",
  "GREEN_A",
  "GREEN_B",
  "GREEN_C",
  "GREEN_D",
  "GREEN_E",
  "GREEN",
  "YELLOW_A",
  "YELLOW_B",
  "YELLOW_C",
  "YELLOW_D",
  "YELLOW_E",
  "YELLOW",
  "GOLD",
  "RED_A",
  "RED_B",
  "RED_C",
  "RED_D",
  "RED_E",
  "RED",
  "MAROON",
  "PURPLE_A",
  "PURPLE_B",
  "PURPLE_C",
  "PURPLE_D",
  "PURPLE_E",
  "PURPLE",
  "ORANGE",
  "PINK",
  "LIGHT_PINK",
  "GRAY_A",
  "GRAY_B",
  "GRAY_C",
  "GRAY_D",
  "GRAY_E",
  "GRAY",
  "GREY_A",
  "GREY_B",
  "GREY_C",
  "GREY_D",
  "GREY_E",
  "GREY",
];

const missing = manimPaletteNames.filter((name) => !exported.has(name));
assert.deepEqual(
  missing,
  [],
  `from noon import * would omit defined Manim palette names: ${missing.join(", ")}`,
);

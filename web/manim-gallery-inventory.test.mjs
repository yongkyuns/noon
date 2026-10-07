import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const manifest = JSON.parse(
  await readFile(new URL("./python/examples/manim_tutorial_manifest.json", import.meta.url), "utf8"),
);

const required = Object.freeze({
  "basic concepts": [
    "ManimCELogo",
    "BraceAnnotation",
    "VectorArrow",
    "GradientImageFromArray",
    "BooleanOperations",
  ],
  "animations and updaters": [
    "PointMovingOnShapes",
    "MovingAround",
    "MovingAngle",
    "MovingDots",
    "MovingGroupToDestination",
    "MovingFrameBox",
    "RotationUpdater",
    "PointWithTrace",
  ],
  plotting: [
    "SinAndCosFunctionPlot",
    "ArgMinExample",
    "GraphAreaPlot",
    "PolygonOnAxes",
    "HeatDiagramPlot",
  ],
  "advanced projects": [
    "OpeningManim",
    "SineCurveUnitCircle",
  ],
});

test("every pinned B7 gallery child case remains explicitly represented", () => {
  const byTitle = new Map(manifest.entries.map((entry) => [entry.title, entry]));
  for (const [group, titles] of Object.entries(required)) {
    for (const title of titles) {
      const entry = byTitle.get(title);
      assert.ok(entry, `${group}: missing required upstream case ${title}`);
      assert.ok(
        entry.status === "ready" || entry.status === "blocked",
        `${group}: ${title} must remain ready or explicitly blocked`,
      );
      if (entry.status === "blocked") {
        assert.equal(
          typeof entry.dependency,
          "string",
          `${group}: blocked ${title} must identify its current owner`,
        );
        assert.match(
          entry.dependency,
          /#[0-9]+/,
          `${group}: blocked ${title} must reference at least one owning issue`,
        );
      }
    }
  }
});

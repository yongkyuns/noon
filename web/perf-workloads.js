export const ANALYTIC_LAYOUTS = Object.freeze(["fit", "fixed", "overdraw"]);

export function buildAnalyticScene({ count, layout = "fit", aspect = 16 / 9 } = {}) {
  if (!Number.isSafeInteger(count) || count <= 0) {
    throw new RangeError("count must be a positive integer");
  }
  if (!ANALYTIC_LAYOUTS.includes(layout)) {
    throw new RangeError(`unknown analytic layout: ${layout}`);
  }
  if (!Number.isFinite(aspect) || aspect <= 0) {
    throw new RangeError("aspect must be a positive finite number");
  }

  if (layout === "fit") {
    return buildFitGrid(count, aspect);
  }
  if (layout === "fixed") {
    return buildFixedGrid(count, aspect);
  }
  return buildOverdraw(count);
}

function buildFitGrid(count, aspect) {
  const columns = Math.ceil(Math.sqrt(count * aspect));
  const rows = Math.ceil(count / columns);
  return {
    document: documentFor(count, (id) => ({
      radius: 0.32,
      x: (id % columns) - columns / 2 + 0.5,
      y: Math.floor(id / columns) - rows / 2 + 0.5,
      alpha: 1,
    })),
    cameraHeight: rows,
    description: "objects fit the viewport; isolates instance/object scaling while visible size shrinks",
  };
}

function buildFixedGrid(count, aspect) {
  const cameraHeight = 6;
  const cameraWidth = cameraHeight * aspect;
  const columns = Math.ceil(Math.sqrt(count * aspect));
  const rows = Math.ceil(count / columns);
  const cellWidth = cameraWidth / columns;
  const cellHeight = cameraHeight / rows;
  return {
    document: documentFor(count, (id) => {
      const column = id % columns;
      const row = Math.floor(id / columns);
      return {
        radius: 0.06,
        x: -cameraWidth / 2 + (column + 0.5) * cellWidth,
        y: -cameraHeight / 2 + (row + 0.5) * cellHeight,
        alpha: 1,
      };
    }),
    cameraHeight,
    description: "fixed apparent circle size; exposes fragment cost as object count grows",
  };
}

function buildOverdraw(count) {
  const cameraHeight = 6;
  const goldenAngle = Math.PI * (3 - Math.sqrt(5));
  return {
    document: documentFor(count, (id) => {
      const radius = 0.4 * Math.sqrt((id + 0.5) / count);
      const angle = id * goldenAngle;
      return {
        radius: 0.35,
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius,
        alpha: 0.16,
      };
    }),
    cameraHeight,
    description: "heavily overlapping transparent circles; stresses alpha blending and overdraw",
  };
}

function documentFor(count, placement) {
  const objects = Array.from({ length: count }, (_, id) => {
    const { radius, x, y, alpha } = placement(id);
    return {
      id,
      geometry: { circle: { radius } },
      transform: {
        translation: { x, y },
        rotation: 0,
        scale: { x: 1, y: 1 },
      },
      style: {
        fill: { red: 0.27, green: 0.65, blue: 0.96, alpha },
        stroke: null,
        stroke_width: 0,
        opacity: 1,
      },
    };
  });
  return { version: 1, objects, tracks: [] };
}

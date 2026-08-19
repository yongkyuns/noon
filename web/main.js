import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");
const patchButton = document.querySelector("#apply-patch");
const patchStatus = document.querySelector("#patch-status");

const DEMO_IDS = Object.freeze({ circle: 0, rectangle: 1, line: 2 });
const PALETTES = [
  {
    name: "electric",
    circle: [1.0, 0.78, 0.22],
    rectangle: [0.72, 0.38, 0.96],
    line: [0.22, 0.88, 0.96],
  },
  {
    name: "original",
    circle: [0.98, 0.38, 0.36],
    rectangle: [0.27, 0.65, 0.96],
    line: [0.3, 0.88, 0.57],
  },
];

function color([red, green, blue]) {
  return { red, green, blue, alpha: 1.0 };
}

function stylePatch(object, fill, stroke, strokeWidth) {
  return {
    set_style: {
      object,
      style: {
        fill: fill === null ? null : color(fill),
        stroke: stroke === null ? null : color(stroke),
        stroke_width: strokeWidth,
        opacity: 1.0,
      },
    },
  };
}

function palettePatchBatch(sequence, palette) {
  return {
    version: 1,
    sequence,
    patches: [
      stylePatch(DEMO_IDS.circle, palette.circle, [1.0, 1.0, 1.0], 0.04),
      stylePatch(DEMO_IDS.rectangle, palette.rectangle, [1.0, 1.0, 1.0], 0.04),
      stylePatch(DEMO_IDS.line, null, palette.line, 0.1),
    ],
  };
}

function showError(error) {
  console.error(error);
  status.value = `Error: ${error}`;
  status.dataset.state = "error";
  patchStatus.value = "Patch failed";
  patchStatus.dataset.state = "error";
}

try {
  if (!navigator.gpu) {
    throw new Error("This browser does not expose WebGPU");
  }

  await init();
  const player = await NoonCanvasPlayer.create(canvas, demoSceneJson(), 4.0);

  function resize() {
    const scale = window.devicePixelRatio || 1;
    const width = Math.round(canvas.clientWidth * scale);
    const height = Math.round(canvas.clientHeight * scale);
    player.resize(width, height);
  }

  resize();
  new ResizeObserver(resize).observe(canvas);

  let paletteIndex = 0;
  patchButton.disabled = false;
  patchStatus.value = "Versioned PatchBatch ready";
  patchStatus.dataset.state = "ready";
  patchStatus.dataset.sequence = String(player.nextSequence());
  patchButton.addEventListener("click", () => {
    try {
      const sequence = Number(player.nextSequence());
      if (!Number.isSafeInteger(sequence)) {
        throw new Error("Patch sequence exceeds JavaScript's safe integer range");
      }
      const palette = PALETTES[paletteIndex];
      const playhead = player.time();

      player.applyPatchBatch(JSON.stringify(palettePatchBatch(sequence, palette)));

      const nextSequence = Number(player.nextSequence());
      if (nextSequence !== sequence + 1) {
        throw new Error("Runtime did not acknowledge the ordered patch batch");
      }
      const preservedPlayhead = player.time();
      if (preservedPlayhead !== playhead) {
        throw new Error("Patch batch changed the current playhead");
      }
      patchStatus.value = `Patch ${sequence} accepted · ${palette.name} · playhead ${preservedPlayhead.toFixed(2)} s preserved`;
      patchStatus.dataset.state = "applied";
      patchStatus.dataset.sequence = String(nextSequence);
      patchStatus.dataset.theme = palette.name;
      paletteIndex = (paletteIndex + 1) % PALETTES.length;
    } catch (error) {
      showError(error);
    }
  });

  let lastStatusUpdate = -Infinity;
  function frame(timestamp) {
    try {
      const presented = player.renderFrame(timestamp);
      if (presented && timestamp - lastStatusUpdate > 200) {
        status.value = `${player.objectCount()} objects · ${player.lastDrawCalls()} draws · ${player.time().toFixed(2)} s`;
        status.dataset.state = "running";
        status.dataset.instances = String(player.lastInstancesDrawn());
        lastStatusUpdate = timestamp;
      }
      requestAnimationFrame(frame);
    } catch (error) {
      showError(error);
    }
  }

  requestAnimationFrame(frame);
} catch (error) {
  showError(error);
}

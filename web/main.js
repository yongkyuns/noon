import init, { NoonCanvasPlayer, demoSceneJson } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");

function showError(error) {
  console.error(error);
  status.value = `Error: ${error}`;
  status.dataset.state = "error";
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

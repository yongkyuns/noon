import init, { createBackgroundAudioRenderer } from "./pkg/noon_web.js";
import { attachNativeInputs } from "./native-inputs.js";

// Browser lifecycle only. Rust owns clip timing, section navigation, playback
// time, wake cadence, and every audio-device scheduling instruction.
const $ = id => document.getElementById(id);
const controls = ["play", "previous", "next", "restart", "mute", "volume", "scrubber"];
let renderer = null;
let wakeHandle = null;
let wakeKind = null;
let retired = false;
let commandPending = false;
let commandCancelled = false;
let detachInputs = null;
let sceneUnlockPromise = null;
const sectionButtons = [];

function cancelWake() {
  if (wakeKind === "frame") cancelAnimationFrame(wakeHandle);
  if (wakeKind === "timer") clearTimeout(wakeHandle);
  wakeHandle = null;
  wakeKind = null;
}

function timeLabel(time) {
  return `${Math.floor(time / 60)}:${String(Math.floor(time % 60)).padStart(2, "0")}`;
}

function update() {
  if (!renderer) return;
  const time = renderer.time();
  const playing = renderer.isDirectPlaying();
  $("elapsed").value = timeLabel(time);
  $("scrubber").value = String(time);
  $("play").innerHTML = playing ? 'Pause <span aria-hidden="true">Ⅱ</span>'
    : `${renderer.audioEnabled() ? "Play" : "Play with sound"} <span aria-hidden="true">▶</span>`;
  $("stage-state").textContent = time >= 30 ? "COMPLETE" : playing ? "PLAYING" : "PAUSED";
  $("mute").textContent = renderer.audioMuted() ? "Sound off" : "Sound on";
  $("mute").setAttribute("aria-pressed", String(renderer.audioMuted()));
  for (let index = 0; index < sectionButtons.length; index++) {
    const active = time >= renderer.sectionTime(index)
      && (index + 1 === sectionButtons.length || time < renderer.sectionTime(index + 1));
    sectionButtons[index].setAttribute("aria-current", active ? "step" : "false");
  }
}

function fail(error) {
  cancelWake();
  renderer?.disposeAudio();
  $("status").textContent = error instanceof Error ? error.message : String(error);
  update();
  $("stage-state").textContent = "STOPPED";
}

function schedule() {
  cancelWake();
  if (retired || !renderer) return;
  const directive = JSON.parse(renderer.directWakeDirectiveJson(performance.now()));
  if (directive.presentNow) renderer.render();
  update();
  if (directive.cadence === "animation-frame") {
    wakeKind = "frame";
    wakeHandle = requestAnimationFrame(wake);
  } else if (directive.cadence === "timer") {
    wakeKind = "timer";
    wakeHandle = setTimeout(() => wake(performance.now()), directive.delayMs);
  }
}

function wake(timestamp) {
  wakeHandle = null;
  wakeKind = null;
  try {
    if (retired || !renderer) return;
    if (renderer.advanceDirectRealtime(timestamp)) renderer.render();
    schedule();
  } catch (error) { fail(error); }
}

async function command(action) {
  if (!renderer || retired || commandPending || document.hidden) return;
  commandPending = true;
  commandCancelled = false;
  const current = renderer;
  cancelWake();
  try {
    await action(current);
    if (retired || current !== renderer || commandCancelled) return;
    renderer.render();
    schedule();
  } catch (error) {
    if (!retired && current === renderer) fail(error);
  } finally { commandPending = false; }
}

// The common collector sends physical input only. Rust picks the scene object,
// commits the click, and dispatches its typed control. This callback schedules
// presentation and carries an opaque input receipt across a native device Promise.
function sceneInput() {
  if (retired || !renderer || document.hidden) return;
  const current = renderer;
  const sequence = current.pendingSceneControlSequence();
  const unlock = current.sceneControlAudioUnlock();
  schedule();
  if (sequence === undefined || !unlock || sceneUnlockPromise === unlock) return;
  sceneUnlockPromise = unlock;
  Promise.resolve(unlock).then(() => {
    if (retired || renderer !== current || document.hidden) return;
    // Device preparation may serve a newer explicit click after cancellation.
    // Rust still owns and validates that request's exact input/publication.
    if (current.sceneControlAudioUnlock() !== unlock) return;
    const admitted = current.pendingSceneControlSequence();
    if (admitted !== undefined && current.completeSceneControl(admitted)) {
      $("status").textContent = "A Small Discovery · original music · CC0 1.0";
      current.render();
      schedule();
    }
  }, error => {
    // A superseded request cannot fault or resume the newer session action.
    if (!retired && renderer === current && current.sceneControlAudioUnlock() === unlock) fail(error);
  }).catch(error => {
    if (!retired && renderer === current) fail(error);
  }).finally(() => {
    if (sceneUnlockPromise === unlock) sceneUnlockPromise = null;
  });
}

$("play").addEventListener("click", () => command(async current => {
  if (current.isDirectPlaying()) {
    current.pauseDirect(performance.now());
    return;
  }
  if (!current.audioEnabled()) await current.enableAudio();
  if (retired || renderer !== current || commandCancelled || document.hidden) return;
  if (current.time() >= 30) current.seekDirect(0);
  current.resumeDirect();
  $("status").textContent = "A Small Discovery · original music · CC0 1.0";
}));
$("previous").addEventListener("click", () => command(current => current.seekSectionDirect(true, performance.now())));
$("next").addEventListener("click", () => command(current => current.seekSectionDirect(false, performance.now())));
$("restart").addEventListener("click", () => command(current => current.seekDirect(0)));
$("scrubber").addEventListener("input", event => command(current => current.seekDirect(Number(event.target.value))));
$("mute").addEventListener("click", () => command(current => current.setAudioMuted(!current.audioMuted())));
$("volume").addEventListener("input", event => command(current => current.setAudioVolume(Number(event.target.value))));

function resize() {
  if (!renderer || retired) return;
  const bounds = $("scene").getBoundingClientRect();
  const ratio = Math.min(devicePixelRatio || 1, 2);
  renderer.resize(Math.max(1, Math.round(bounds.width * ratio)), Math.max(1, Math.round(bounds.height * ratio)));
  renderer.render();
}

function retire() {
  retired = true;
  cancelWake();
  detachInputs?.();
  detachInputs = null;
  renderer?.disposeAudio();
  renderer?.free();
  renderer = null;
}
window.addEventListener("pagehide", retire, { once: true });
// Browser backgrounding explicitly pauses this demo. Returning to the tab does
// not automatically resume sound or create another playback epoch.
document.addEventListener("visibilitychange", () => {
  if (document.hidden && renderer) {
    // Cancel this serialized command even if the tab returns before an async
    // device unlock resolves. A fresh user action may start a new command.
    commandCancelled = true;
    cancelWake();
    try { renderer.pauseDirect(performance.now()); update(); } catch (error) { fail(error); }
  }
});
window.addEventListener("resize", () => { try { resize(); } catch (error) { fail(error); } });

try {
  await init();
  const canvas = $("scene").transferControlToOffscreen();
  const prepared = await createBackgroundAudioRenderer(canvas);
  if (retired) prepared.free();
  else {
    renderer = prepared;
    renderer.setAudioVolume(Number($("volume").value));
    for (let index = 0; index < renderer.sectionCount(); index++) {
      const button = document.createElement("button");
      button.className = "section";
      const label = document.createElement("span");
      label.textContent = `${String(index + 1).padStart(2, "0")}  ${renderer.sectionName(index)}`;
      const time = document.createElement("time");
      time.textContent = timeLabel(renderer.sectionTime(index));
      button.append(label, time);
      button.addEventListener("click", () => command(current => current.seekDirect(current.sectionTime(index))));
      sectionButtons.push(button);
      $("sections").append(button);
    }
    resize();
    detachInputs = attachNativeInputs(renderer, $("scene"), {
      keyboardTarget: $("scene"), onInput: sceneInput, onError: fail,
    });
    $("loading").hidden = true;
    for (const id of controls) $(id).disabled = false;
    $("status").textContent = "Ready · use the canvas Play button or Play with sound";
    schedule();
  }
} catch (error) { fail(error); $("loading").textContent = $("status").textContent; }

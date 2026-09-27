const STYLE_ID = "noon-playground-playback-controls-style";

if (typeof document !== "undefined") {
  ensurePlaybackSlot();
}

export class PlaygroundPlaybackControls {
  #player;
  #onError;
  #root;
  #playButton;
  #restartButton;
  #scrubber;
  #timeOutput;
  #liveStatus;
  #durationSeconds;
  #timeSeconds = 0;
  #playing = true;
  #externalBusy = false;
  #controllable = true;
  #unavailableReason = null;
  #commandPending = false;
  #seekActive = false;
  #desiredSeek = null;
  #destroyed = false;

  constructor(
    player,
    previewPane,
    { durationSeconds = 4, onError = null } = {},
  ) {
    validatePlayer(player);
    if (!(previewPane instanceof HTMLElement)) {
      throw new TypeError("playback controls require a preview HTMLElement");
    }
    if (onError !== null && typeof onError !== "function") {
      throw new TypeError("playback controls onError must be a function");
    }
    this.#player = player;
    this.#onError = onError;
    this.#durationSeconds = validateDuration(durationSeconds);

    installStyles();

    this.#root = previewPane.querySelector(".playback-slot");
    if (!(this.#root instanceof HTMLElement)) {
      this.#root = document.createElement("section");
      previewPane.append(this.#root);
    }
    this.#root.className = "playback-controls";
    this.#root.setAttribute("aria-label", "Animation playback controls");
    this.#root.replaceChildren();

    this.#playButton = document.createElement("button");
    this.#playButton.type = "button";
    this.#playButton.className = "playback-button playback-toggle";
    this.#playButton.addEventListener("click", this.#handleToggle);

    this.#restartButton = document.createElement("button");
    this.#restartButton.type = "button";
    this.#restartButton.className = "playback-button playback-restart";
    this.#restartButton.textContent = "↺";
    this.#restartButton.setAttribute("aria-label", "Restart animation from the beginning");
    this.#restartButton.title = "Restart";
    this.#restartButton.addEventListener("click", this.#handleRestart);

    const timeline = document.createElement("label");
    timeline.className = "playback-timeline";

    this.#scrubber = document.createElement("input");
    this.#scrubber.type = "range";
    this.#scrubber.className = "playback-scrubber";
    this.#scrubber.min = "0";
    // A millisecond step truncates endpoints such as 2.5999999999999996 to
    // 2.599 in the browser. Preserve the exact authored range and seek value.
    this.#scrubber.step = "any";
    this.#scrubber.setAttribute("aria-label", "Animation playhead");
    this.#scrubber.addEventListener("input", this.#handleSeekInput);
    this.#liveStatus = document.createElement("span");
    this.#liveStatus.className = "playback-live-status";
    this.#liveStatus.textContent = "Duration pending";
    timeline.append(this.#scrubber, this.#liveStatus);

    this.#timeOutput = document.createElement("output");
    this.#timeOutput.className = "playback-time";
    this.#timeOutput.setAttribute("aria-live", "off");
    this.#timeOutput.setAttribute("aria-label", "Animation time");

    this.#root.append(this.#playButton, this.#restartButton, timeline, this.#timeOutput);
    this.#render();
  }

  get element() {
    return this.#root;
  }

  get durationSeconds() {
    return this.#controllable ? this.#durationSeconds : null;
  }

  setDuration(durationSeconds) {
    this.#durationSeconds = validateDuration(durationSeconds);
    if (this.#controllable) this.#timeSeconds = Math.min(this.#timeSeconds, this.#durationSeconds);
    this.#render();
  }

  setControllable(controllable) {
    this.#controllable = Boolean(controllable);
    this.#unavailableReason = null;
    this.#root.dataset.controllable = String(this.#controllable);
    this.#root.title = this.#controllable ? "" : "Live progress · seeking is available after Python finishes";
    this.#render();
  }

  setUnavailable(reason) {
    this.setControllable(false);
    this.#unavailableReason = String(reason ?? "History not retained");
    this.#root.title = `Replay unavailable: ${this.#unavailableReason}`;
    this.#render();
  }

  // A read-only observation must not fight a user's in-flight scrub or command.
  observe(state) {
    if (this.#destroyed || this.#seekActive || this.#commandPending) return;
    // A continuation's current play/wait endpoint is not its full duration.
    // Only the completed source result may set the replay range via setDuration.
    this.sync({ time: state.time, playing: state.playing });
  }

  setBusy(busy) {
    this.#externalBusy = Boolean(busy);
    this.#renderDisabled();
  }

  sync({ time, playing, durationSeconds = undefined }) {
    if (this.#controllable && durationSeconds !== undefined) {
      this.#durationSeconds = validateDuration(durationSeconds);
    }
    if (!Number.isFinite(time) || time < 0) {
      throw new TypeError("playback state time must be finite and non-negative");
    }
    if (typeof playing !== "boolean") {
      throw new TypeError("playback state playing must be boolean");
    }
    this.#timeSeconds = this.#controllable ? Math.min(time, this.#durationSeconds) : time;
    this.#playing = playing;
    this.#render();
  }

  updateTime(time) {
    if (
      !Number.isFinite(time) ||
      time < 0 ||
      !this.#playing ||
      this.#seekActive ||
      this.#desiredSeek !== null
    ) {
      return;
    }
    this.#timeSeconds = this.#controllable ? Math.min(time, this.#durationSeconds) : time;
    this.#renderTime();
  }

  destroy() {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.#playButton.removeEventListener("click", this.#handleToggle);
    this.#restartButton.removeEventListener("click", this.#handleRestart);
    this.#scrubber.removeEventListener("input", this.#handleSeekInput);
    renderPlaybackPlaceholder(this.#root);
  }

  #handleToggle = () => {
    void this.#runCommand(async () => {
      const result = this.#playing ? await this.#player.pause() : await this.#player.resume();
      if (!this.#destroyed) this.sync(result);
    });
  };

  #handleRestart = () => {
    void this.#runCommand(async () => {
      const result = await this.#player.restartPlayback();
      if (!this.#destroyed) this.sync(result);
    });
  };

  #handleSeekInput = () => {
    if (this.#destroyed || !this.#controllable || this.#externalBusy || this.#commandPending) return;
    const target = Number(this.#scrubber.value);
    if (!Number.isFinite(target)) return;
    const bounded = Math.min(Math.max(target, 0), this.#durationSeconds);
    // Browsers may serialize a range value to fewer digits than its max.
    // Restore only an endpoint within floating-point roundoff, not a time step.
    const endpointRoundoff = 8 * Number.EPSILON * this.#durationSeconds;
    this.#timeSeconds = this.#durationSeconds - bounded <= endpointRoundoff
      ? this.#durationSeconds : bounded;
    this.#desiredSeek = this.#timeSeconds;
    this.#renderTime();
    void this.#drainSeek();
  };

  async #runCommand(operation) {
    if (this.#destroyed || !this.#controllable || this.#externalBusy || this.#commandPending || this.#seekActive) return;
    this.#commandPending = true;
    this.#renderDisabled();
    try {
      await operation();
    } catch (error) {
      this.#reportError(error);
    } finally {
      this.#commandPending = false;
      this.#renderDisabled();
    }
  }

  async #drainSeek() {
    if (this.#seekActive || this.#destroyed) return;
    this.#seekActive = true;
    this.#renderDisabled();
    try {
      while (!this.#destroyed && this.#desiredSeek !== null) {
        const target = this.#desiredSeek;
        this.#desiredSeek = null;
        const result = await this.#player.seek(target);
        if (this.#desiredSeek === null) {
          if (!this.#destroyed) this.sync(result);
        } else if (typeof result.playing === "boolean") {
          // Keep the latest user-selected playhead visible while an older seek
          // completion is superseded by another queued direct seek.
          this.#playing = result.playing;
          this.#renderDisabled();
        }
      }
    } catch (error) {
      this.#desiredSeek = null;
      this.#reportError(error);
    } finally {
      this.#seekActive = false;
      this.#renderDisabled();
    }
  }

  #reportError(error) {
    if (this.#destroyed) return;
    if (this.#onError !== null) {
      this.#onError(error);
      return;
    }
    console.error(error);
  }

  #render() {
    if (this.#destroyed) return;
    this.#playButton.textContent = this.#unavailableReason !== null ? "Complete" : !this.#controllable ? "Live" : this.#playing ? "Pause" : "Play";
    this.#playButton.setAttribute(
      "aria-label",
      !this.#controllable ? (this.#unavailableReason !== null ? "Replay unavailable" : "Python owns live playback") : this.#playing ? "Pause animation" : "Play animation",
    );
    this.#scrubber.hidden = !this.#controllable;
    this.#liveStatus.hidden = this.#controllable;
    this.#liveStatus.textContent = this.#unavailableReason !== null ? "Replay unavailable" : "Duration pending";
    this.#scrubber.max = String(this.#durationSeconds);
    this.#renderTime();
    this.#renderDisabled();
  }

  #renderTime() {
    if (this.#destroyed) return;
    const time = this.#controllable
      ? Math.min(this.#timeSeconds, this.#durationSeconds) : this.#timeSeconds;
    this.#root.dataset.elapsedSeconds = String(time);
    // Keep the layout slot, but do not draw a percentage thumb before the total
    // is known. Expanding a segment-sized range makes forward time jump back.
    this.#scrubber.value = String(Math.min(time, this.#durationSeconds));
    this.#scrubber.setAttribute(
      "aria-valuetext",
      this.#controllable
        ? `${formatTime(time)} seconds of ${formatTime(this.#durationSeconds)} seconds`
        : `${formatTime(time)} seconds elapsed · total duration not yet known`,
    );
    this.#timeOutput.value = this.#controllable
      ? `${formatTime(time)} / ${formatTime(this.#durationSeconds)} s`
      : this.#unavailableReason !== null ? `${formatTime(time)} s · completed` : `${formatTime(time)} / — s`;
  }

  #renderDisabled() {
    if (this.#destroyed) return;
    // Capability denial is not an unfinished command. A completed, non-replayable
    // scene stays disabled without displaying an indefinite wait cursor/aria-busy.
    const busy = this.#externalBusy || this.#commandPending || this.#seekActive;
    const blockCommands = !this.#controllable || busy;
    this.#playButton.disabled = blockCommands;
    this.#restartButton.disabled = blockCommands;
    this.#scrubber.disabled = !this.#controllable || this.#externalBusy || this.#commandPending;
    this.#root.dataset.busy = String(busy);
    this.#root.dataset.playing = String(this.#playing);
    this.#root.setAttribute("aria-busy", String(busy));
  }
}

function ensurePlaybackSlot() {
  installStyles();
  const previewPane = document.querySelector(".preview-pane");
  if (!(previewPane instanceof HTMLElement)) return null;
  const existing = previewPane.querySelector(".playback-slot, .playback-controls");
  if (existing instanceof HTMLElement) return existing;
  const root = document.createElement("section");
  renderPlaybackPlaceholder(root);
  previewPane.append(root);
  return root;
}

function renderPlaybackPlaceholder(root) {
  root.className = "playback-slot";
  root.removeAttribute("aria-busy");
  root.removeAttribute("data-busy");
  root.removeAttribute("data-playing");
  root.removeAttribute("data-controllable");
  root.removeAttribute("data-elapsed-seconds");
  root.removeAttribute("title");
  root.setAttribute("aria-label", "Animation playback controls");

  const playButton = document.createElement("button");
  playButton.type = "button";
  playButton.className = "playback-button playback-toggle";
  playButton.textContent = "Play";
  playButton.disabled = true;
  playButton.setAttribute("aria-label", "Run the example to enable playback");

  const restartButton = document.createElement("button");
  restartButton.type = "button";
  restartButton.className = "playback-button playback-restart";
  restartButton.textContent = "↺";
  restartButton.disabled = true;
  restartButton.setAttribute("aria-label", "Run the example to enable restart");
  restartButton.title = "Restart";

  const timeline = document.createElement("label");
  timeline.className = "playback-timeline";
  const scrubber = document.createElement("input");
  scrubber.type = "range";
  scrubber.className = "playback-scrubber";
  scrubber.min = "0";
  scrubber.max = "1";
  scrubber.value = "0";
  scrubber.disabled = true;
  scrubber.setAttribute("aria-label", "Animation playhead unavailable until Run");
  timeline.append(scrubber);

  const timeOutput = document.createElement("output");
  timeOutput.className = "playback-time";
  timeOutput.setAttribute("aria-live", "off");
  timeOutput.setAttribute("aria-label", "Animation time unavailable until Run");
  timeOutput.value = "0.00 / —";

  root.replaceChildren(playButton, restartButton, timeline, timeOutput);
}

function installStyles() {
  if (document.getElementById(STYLE_ID) !== null) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    .playback-slot,
    .playback-controls {
      display: grid;
      grid-template-columns: auto auto minmax(4rem, 1fr) auto;
      align-items: center;
      gap: 0.45rem;
      min-height: 3rem;
      padding: 0.5rem 0.75rem;
      border-top: 1px solid var(--border);
      background: rgb(9 12 19 / 94%);
    }
    .playback-button {
      appearance: none;
      border: 1px solid var(--border-strong);
      border-radius: 0.55rem;
      padding: 0.4rem 0.56rem;
      background: #171d2a;
      color: #d9deeb;
      cursor: pointer;
      font-size: 0.7rem;
      font-weight: 700;
    }
    .playback-toggle { min-width: 4.1rem; }
    .playback-restart { min-width: 2.25rem; }
    .playback-button:hover:not(:disabled) { background: #20283a; }
    .playback-button:focus-visible,
    .playback-scrubber:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .playback-timeline {
      display: block;
      min-width: 0;
    }
    .playback-scrubber {
      width: 100%;
      min-width: 0;
      accent-color: var(--accent);
      cursor: pointer;
    }
    .playback-time {
      min-width: 6.2rem;
      color: #b9c3d6;
      font: 0.68rem ui-monospace, SFMono-Regular, Menlo, monospace;
      text-align: right;
      white-space: nowrap;
    }
    .playback-slot button:disabled,
    .playback-slot input:disabled,
    .playback-controls button:disabled,
    .playback-controls input:disabled {
      cursor: default;
      opacity: 0.42;
    }
    .playback-live-status {
      display: block;
      overflow: hidden;
      color: #b9c3d6;
      font-size: 0.68rem;
      white-space: nowrap;
      text-overflow: ellipsis;
    }
    .playback-controls [hidden] { display: none !important; }
    .playback-controls[data-busy="true"] button:disabled,
    .playback-controls[data-busy="true"] input:disabled {
      cursor: wait;
    }
    @media (max-width: 44rem) {
      .playback-slot,
      .playback-controls {
        grid-template-columns: auto auto minmax(3rem, 1fr) auto;
        gap: 0.32rem;
        min-height: 2.75rem;
        padding: 0.42rem 0.5rem;
      }
      .playback-button {
        padding: 0.36rem 0.45rem;
        font-size: 0.66rem;
      }
      .playback-toggle { min-width: 3.6rem; }
      .playback-restart { min-width: 2rem; }
      .playback-time {
        min-width: 5.25rem;
        font-size: 0.62rem;
      }
    }
  `;
  document.head.append(style);
}

function formatTime(seconds) {
  return seconds.toFixed(2);
}

function validateDuration(durationSeconds) {
  if (!Number.isFinite(durationSeconds) || durationSeconds <= 0) {
    throw new TypeError("playback duration must be positive and finite");
  }
  return durationSeconds;
}

function validatePlayer(player) {
  for (const method of ["pause", "resume", "seek", "restartPlayback"]) {
    if (typeof player?.[method] !== "function") {
      throw new TypeError(`playback controls require player.${method}()`);
    }
  }
}

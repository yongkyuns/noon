const STYLE_ID = "noon-playground-playback-controls-style";

export class PlaygroundPlaybackControls {
  #player;
  #onError;
  #root;
  #playButton;
  #restartButton;
  #scrubber;
  #timeOutput;
  #durationSeconds;
  #timeSeconds = 0;
  #playing = true;
  #externalBusy = false;
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

    this.#root = document.createElement("section");
    this.#root.className = "playback-controls";
    this.#root.setAttribute("aria-label", "Animation playback controls");

    this.#playButton = document.createElement("button");
    this.#playButton.type = "button";
    this.#playButton.className = "playback-button playback-toggle";
    this.#playButton.addEventListener("click", this.#handleToggle);

    this.#restartButton = document.createElement("button");
    this.#restartButton.type = "button";
    this.#restartButton.className = "playback-button";
    this.#restartButton.textContent = "Restart";
    this.#restartButton.setAttribute("aria-label", "Restart animation from the beginning");
    this.#restartButton.addEventListener("click", this.#handleRestart);

    const timeline = document.createElement("label");
    timeline.className = "playback-timeline";
    const timelineLabel = document.createElement("span");
    timelineLabel.className = "playback-label";
    timelineLabel.textContent = "Timeline";

    this.#scrubber = document.createElement("input");
    this.#scrubber.type = "range";
    this.#scrubber.className = "playback-scrubber";
    this.#scrubber.min = "0";
    this.#scrubber.step = "0.001";
    this.#scrubber.setAttribute("aria-label", "Animation playhead");
    this.#scrubber.addEventListener("input", this.#handleSeekInput);
    timeline.append(timelineLabel, this.#scrubber);

    this.#timeOutput = document.createElement("output");
    this.#timeOutput.className = "playback-time";
    this.#timeOutput.setAttribute("aria-live", "off");
    this.#timeOutput.setAttribute("aria-label", "Animation time");

    this.#root.append(this.#playButton, this.#restartButton, timeline, this.#timeOutput);
    const metrics = previewPane.querySelector(".metrics");
    if (metrics !== null) {
      previewPane.insertBefore(this.#root, metrics);
    } else {
      previewPane.append(this.#root);
    }
    this.#render();
  }

  get element() {
    return this.#root;
  }

  get durationSeconds() {
    return this.#durationSeconds;
  }

  setDuration(durationSeconds) {
    this.#durationSeconds = validateDuration(durationSeconds);
    this.#timeSeconds = Math.min(this.#timeSeconds, this.#durationSeconds);
    this.#render();
  }

  setBusy(busy) {
    this.#externalBusy = Boolean(busy);
    this.#renderDisabled();
  }

  sync({ time, playing, durationSeconds = undefined }) {
    if (durationSeconds !== undefined) {
      this.#durationSeconds = validateDuration(durationSeconds);
    }
    if (!Number.isFinite(time) || time < 0) {
      throw new TypeError("playback state time must be finite and non-negative");
    }
    if (typeof playing !== "boolean") {
      throw new TypeError("playback state playing must be boolean");
    }
    this.#timeSeconds = Math.min(time, this.#durationSeconds);
    this.#playing = playing;
    this.#render();
  }

  updateTime(time) {
    if (!Number.isFinite(time) || time < 0 || this.#seekActive || this.#desiredSeek !== null) {
      return;
    }
    this.#timeSeconds = Math.min(time, this.#durationSeconds);
    this.#renderTime();
  }

  destroy() {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.#playButton.removeEventListener("click", this.#handleToggle);
    this.#restartButton.removeEventListener("click", this.#handleRestart);
    this.#scrubber.removeEventListener("input", this.#handleSeekInput);
    this.#root.remove();
  }

  #handleToggle = () => {
    void this.#runCommand(async () => {
      const result = this.#playing ? await this.#player.pause() : await this.#player.resume();
      this.sync(result);
    });
  };

  #handleRestart = () => {
    void this.#runCommand(async () => {
      const result = await this.#player.restartPlayback();
      this.sync(result);
    });
  };

  #handleSeekInput = () => {
    if (this.#destroyed || this.#externalBusy || this.#commandPending) return;
    const target = Number(this.#scrubber.value);
    if (!Number.isFinite(target)) return;
    this.#timeSeconds = Math.min(Math.max(target, 0), this.#durationSeconds);
    this.#desiredSeek = this.#timeSeconds;
    this.#renderTime();
    void this.#drainSeek();
  };

  async #runCommand(operation) {
    if (this.#destroyed || this.#externalBusy || this.#commandPending || this.#seekActive) return;
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
          this.sync(result);
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
    if (this.#onError !== null) {
      this.#onError(error);
      return;
    }
    console.error(error);
  }

  #render() {
    this.#playButton.textContent = this.#playing ? "Pause" : "Play";
    this.#playButton.setAttribute(
      "aria-label",
      this.#playing ? "Pause animation" : "Play animation",
    );
    this.#scrubber.max = String(this.#durationSeconds);
    this.#renderTime();
    this.#renderDisabled();
  }

  #renderTime() {
    const clampedTime = Math.min(this.#timeSeconds, this.#durationSeconds);
    this.#scrubber.value = String(clampedTime);
    this.#scrubber.setAttribute(
      "aria-valuetext",
      `${formatTime(clampedTime)} of ${formatTime(this.#durationSeconds)}`,
    );
    this.#timeOutput.value = `${formatTime(clampedTime)} / ${formatTime(this.#durationSeconds)}`;
  }

  #renderDisabled() {
    const blockCommands = this.#externalBusy || this.#commandPending || this.#seekActive;
    this.#playButton.disabled = blockCommands;
    this.#restartButton.disabled = blockCommands;
    this.#scrubber.disabled = this.#externalBusy || this.#commandPending;
    this.#root.dataset.busy = String(blockCommands);
    this.#root.dataset.playing = String(this.#playing);
    this.#root.setAttribute("aria-busy", String(blockCommands));
  }
}

function installStyles() {
  if (document.getElementById(STYLE_ID) !== null) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    .playback-controls {
      display: grid;
      grid-template-columns: auto auto minmax(8rem, 1fr) auto;
      align-items: center;
      gap: 0.55rem;
      min-height: 3.15rem;
      padding: 0.55rem 0.85rem;
      border-top: 1px solid var(--border);
      background: rgb(9 12 19 / 94%);
    }
    .playback-button {
      appearance: none;
      min-width: 4.5rem;
      border: 1px solid var(--border-strong);
      border-radius: 0.55rem;
      padding: 0.42rem 0.6rem;
      background: #171d2a;
      color: #d9deeb;
      cursor: pointer;
      font-size: 0.72rem;
      font-weight: 700;
    }
    .playback-button:hover:not(:disabled) { background: #20283a; }
    .playback-button:focus-visible,
    .playback-scrubber:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .playback-timeline {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: center;
      gap: 0.5rem;
      min-width: 0;
    }
    .playback-label {
      color: var(--muted-2);
      font-size: 0.66rem;
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }
    .playback-scrubber {
      width: 100%;
      min-width: 0;
      accent-color: var(--accent);
      cursor: pointer;
    }
    .playback-time {
      min-width: 7.2rem;
      color: #b9c3d6;
      font: 0.7rem ui-monospace, SFMono-Regular, Menlo, monospace;
      text-align: right;
      white-space: nowrap;
    }
    .playback-controls button:disabled,
    .playback-controls input:disabled {
      cursor: wait;
      opacity: 0.48;
    }
    @media (max-width: 44rem) {
      .playback-controls {
        grid-template-columns: 1fr 1fr;
        gap: 0.45rem;
        padding: 0.55rem;
      }
      .playback-button { width: 100%; }
      .playback-timeline { grid-column: 1 / -1; }
      .playback-time {
        grid-column: 1 / -1;
        min-width: 0;
        text-align: center;
      }
    }
  `;
  document.head.append(style);
}

function formatTime(seconds) {
  return `${seconds.toFixed(2)} s`;
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

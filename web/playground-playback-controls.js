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
  #undoDragButton;
  #scrubber;
  #timeOutput;
  #liveStatus;
  #audioControls;
  #previousSection;
  #nextSection;
  #sectionControls;
  #sectionNames;
  #sectionMarkers;
  #sectionSignature = "";
  #muteButton;
  #volume;
  #hasAudio = false;
  #audioReady = false;
  #audioMuted = false;
  #sections = [];
  #sectionState = null;
  #durationSeconds;
  // Constant-size UI observations, never a source cursor or a second clock.
  #preparedFrontier = null;
  #seekRange = null;
  #gestureRange = null;
  #buffering = false;
  #seekNotice = null;
  #confirmedTimeSeconds = 0;
  #timeSeconds = 0;
  #playing = true;
  #externalBusy = false;
  #controllable = true;
  #forwardPlayback = false;
  #unavailableReason = null;
  #canUndoTranslationDrag = false;
  #undoDragSupported = false;
  #commandPending = false;
  #seekActive = false;
  #desiredSeek = null;
  #desiredPlaying = null;
  #pendingActivation = null;
  #transportActive = false;
  #presentedSession = null;
  #presentedSequence = null;
  #controlGeneration = 0;
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
    this.#undoDragSupported = typeof player.undoTranslationDrag === "function";
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
    this.#root.setAttribute("tabindex", "0");
    this.#root.setAttribute("aria-keyshortcuts", "Space ArrowLeft ArrowRight Home");
    this.#root.setAttribute("aria-description", "When this panel is focused: Space plays or pauses, Left and Right navigate key points, Home restarts.");
    this.#root.addEventListener("keydown", this.#handleKeyboard);
    this.#root.addEventListener("keyup", this.#handleKeyboard);
    this.#root.dataset.undoSupport = String(this.#undoDragSupported);
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

    this.#undoDragButton = document.createElement("button");
    this.#undoDragButton.type = "button";
    this.#undoDragButton.className = "playback-button playback-undo-drag";
    this.#undoDragButton.textContent = "Undo drag";
    this.#undoDragButton.setAttribute("aria-label", "Undo the most recent drag");
    this.#undoDragButton.addEventListener("click", this.#handleUndoDrag);
    this.#undoDragButton.hidden = !this.#undoDragSupported;

    const timeline = document.createElement("div");
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
    this.#scrubber.addEventListener("pointerdown", this.#handlePointerDown);
    this.#scrubber.addEventListener("pointerup", this.#handlePointerEnd);
    this.#scrubber.addEventListener("pointercancel", this.#handlePointerEnd);
    this.#liveStatus = document.createElement("span");
    this.#liveStatus.className = "playback-live-status";
    this.#liveStatus.textContent = "Duration pending";
    this.#liveStatus.setAttribute("role", "status");
    this.#liveStatus.setAttribute("aria-live", "polite");
    this.#sectionMarkers = document.createElement("fieldset");
    this.#sectionMarkers.className = "playback-section-markers";
    this.#sectionMarkers.setAttribute("aria-label", "Lesson key points");
    timeline.append(this.#scrubber, this.#sectionMarkers, this.#liveStatus);

    this.#timeOutput = document.createElement("output");
    this.#timeOutput.className = "playback-time";
    this.#timeOutput.setAttribute("aria-live", "off");
    this.#timeOutput.setAttribute("aria-label", "Animation time");

    this.#root.append(this.#playButton, this.#restartButton, this.#undoDragButton, timeline, this.#timeOutput);
    this.#audioControls = document.createElement("div");
    this.#audioControls.className = "playback-audio-controls";
    this.#audioControls.hidden = true;
    this.#previousSection = document.createElement("button");
    this.#previousSection.type = "button";
    this.#previousSection.className = "playback-button playback-previous-section";
    this.#previousSection.textContent = "Previous";
    this.#previousSection.setAttribute("aria-label", "Previous lesson section");
    this.#previousSection.addEventListener("click", this.#handlePreviousSection);
    this.#nextSection = document.createElement("button");
    this.#nextSection.type = "button";
    this.#nextSection.className = "playback-button playback-next-section";
    this.#nextSection.textContent = "Next";
    this.#nextSection.setAttribute("aria-label", "Next lesson section");
    this.#nextSection.addEventListener("click", this.#handleNextSection);
    this.#sectionNames = document.createElement("select");
    this.#sectionNames.className = "playback-section-names";
    this.#sectionNames.setAttribute("aria-label", "Jump to lesson key point");
    this.#sectionNames.addEventListener("change", this.#handleSectionJump);
    this.#sectionControls = document.createElement("div");
    this.#sectionControls.className = "playback-section-controls";
    this.#sectionControls.append(this.#previousSection, this.#nextSection, this.#sectionNames);
    this.#root.append(this.#sectionControls);
    this.#muteButton = document.createElement("button");
    this.#muteButton.type = "button";
    this.#muteButton.className = "playback-button playback-mute";
    this.#muteButton.addEventListener("click", this.#handleMute);
    const volumeLabel = document.createElement("label");
    volumeLabel.className = "playback-volume-label";
    volumeLabel.textContent = "Music";
    this.#volume = document.createElement("input");
    this.#volume.type = "range";
    this.#volume.min = "0";
    this.#volume.max = "1";
    this.#volume.step = "0.01";
    this.#volume.value = "1";
    this.#volume.setAttribute("aria-label", "Music volume");
    this.#volume.addEventListener("input", this.#handleVolume);
    volumeLabel.append(this.#volume);
    this.#audioControls.append(this.#muteButton, volumeLabel);
    this.#root.append(this.#audioControls);
    this.#render();
  }

  get element() {
    return this.#root;
  }

  get durationSeconds() {
    return this.#controllable ? this.#durationSeconds : null;
  }

  setDuration(durationSeconds) {
    if (this.#preparedFrontier !== null) {
      throw new Error("prepared duration comes from adopted terminal history, not source completion");
    }
    this.#durationSeconds = validateDuration(durationSeconds);
    if (this.#controllable) this.#timeSeconds = Math.min(this.#timeSeconds, this.#durationSeconds);
    this.#render();
  }

  setControllable(controllable) {
    const next = Boolean(controllable);
    if (this.#controllable && !next) this.#invalidateCommands();
    this.#controllable = next;
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
    if (this.#destroyed) return;
    if (this.#seekActive || this.#commandPending || this.#transportActive) {
      if (state.preparedPlayback === false || state.sourceExecution === "observed") {
        this.#invalidateCommands();
        this.sync(state);
        return;
      }
      if (state.preparedPlayback === true) {
        validatePreparedState(state);
        this.#preparedFrontier = state.preparedFrontier;
        this.#seekRange = state.seekRange;
        this.#durationSeconds = state.durationSeconds;
        this.#buffering = state.buffering;
        this.#render();
      }
      return;
    }
    // Prepared state already distinguishes an open frontier from the adopted
    // final endpoint. Ordinary source-driven progress still has no replay range.
    if (state.preparedPlayback === true || this.#preparedFrontier !== null || state.sourceExecution === "observed") this.sync(state);
    // A continuation's current play/wait endpoint is not its full duration.
    // Only the completed source result may set the replay range via setDuration.
    else this.sync({ time: state.time, playing: state.playing, canUndoTranslationDrag: state.canUndoTranslationDrag,
      sourcePlaybackControllable: state.sourcePlaybackControllable,
      hasAudio: state.hasAudio, audioReady: state.audioReady, audioMuted: state.audioMuted,
      audioVolume: state.audioVolume, sections: state.sections, sectionState: state.sectionState,
      ...(state.hasAudio ? { durationSeconds: state.durationSeconds } : {}),
    });
  }

  setBusy(busy) {
    const next = Boolean(busy);
    if (next && !this.#externalBusy) this.#invalidateCommands();
    this.#externalBusy = next;
    this.#renderDisabled();
  }

  #invalidateCommands() {
    this.#controlGeneration += 1;
    this.#desiredSeek = null;
    this.#desiredPlaying = null;
    this.#pendingActivation = null;
    this.#gestureRange = null;
  }

  #currentSeekRange() {
    if (this.#preparedFrontier !== null) return this.#seekRange;
    if (this.#durationSeconds === null) return null;
    return { start: 0, end: this.#durationSeconds };
  }

  sync(state) {
    const { time, playing, durationSeconds, canUndoTranslationDrag,
      hasAudio, audioReady, audioMuted, audioVolume, sections, sectionState } = state;
    if (!Number.isFinite(time) || time < 0) {
      throw new TypeError("playback state time must be finite and non-negative");
    }
    if (typeof playing !== "boolean") {
      throw new TypeError("playback state playing must be boolean");
    }
    if (canUndoTranslationDrag !== undefined && typeof canUndoTranslationDrag !== "boolean") {
      throw new TypeError("canUndoTranslationDrag must be boolean when provided");
    }
    if (state.sourcePlaybackControllable !== undefined && typeof state.sourcePlaybackControllable !== "boolean") {
      throw new TypeError("sourcePlaybackControllable must be boolean when provided");
    }
    const observed = state.sourceExecution === "observed";
    if (state.preparedPlayback === false && state.preparedFrontier !== undefined) {
      throw new TypeError("completed playback state cannot contain a prepared frontier");
    }
    const prepared = !observed && state.preparedPlayback !== false &&
      (state.preparedPlayback === true || this.#preparedFrontier !== null);
    let duration = this.#durationSeconds;
    if (prepared) {
      // Validate the entire observation before changing controls. It is a reader
      // capability, not inferred from Python completion or the bootstrap horizon.
      validatePreparedState(state);
      duration = durationSeconds;
    } else if (observed) {
      duration = null;
    } else if (durationSeconds !== undefined) {
      duration = validateDuration(durationSeconds);
    }
    this.#durationSeconds = duration;
    this.#forwardPlayback = state.sourcePlaybackControllable === true;
    if (observed || state.preparedPlayback === false) {
      this.#preparedFrontier = null;
      this.#seekRange = null;
      this.#buffering = false;
      if (observed && this.#controllable) this.setControllable(false);
    }
    if (prepared) {
      this.#preparedFrontier = state.preparedFrontier;
      this.#seekRange = state.seekRange;
      this.#buffering = state.buffering;
    }
    this.#confirmedTimeSeconds = this.#controllable && duration !== null ? Math.min(time, duration) : time;
    this.#timeSeconds = this.#confirmedTimeSeconds;
    this.#playing = playing;
    this.#desiredPlaying = null;
    if (prepared) this.#hasAudio = false;
    else if (hasAudio !== undefined) this.#hasAudio = hasAudio === true;
    if (audioReady !== undefined) this.#audioReady = audioReady === true;
    if (audioMuted !== undefined) this.#audioMuted = audioMuted === true;
    if (audioVolume !== undefined) this.#volume.value = String(audioVolume);
    if (prepared) { this.#sections = []; this.#sectionState = null; }
    else if (sections !== undefined) this.#sections = sections;
    if (sectionState !== undefined && !prepared) this.#sectionState = sectionState;
    this.#seekNotice = null;
    if (canUndoTranslationDrag !== undefined) this.#canUndoTranslationDrag = canUndoTranslationDrag;
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
    if (this.#preparedFrontier !== null) return; // Prepared observations require their readiness metadata.
    this.#timeSeconds = this.#controllable ? Math.min(time, this.#durationSeconds) : time;
    this.#confirmedTimeSeconds = this.#timeSeconds;
    this.#renderTime();
  }

  destroy() {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.#root.removeEventListener("keydown", this.#handleKeyboard);
    this.#root.removeEventListener("keyup", this.#handleKeyboard);
    this.#root.removeAttribute("tabindex");
    this.#root.removeAttribute("aria-keyshortcuts");
    this.#root.removeAttribute("aria-description");
    this.#playButton.removeEventListener("click", this.#handleToggle);
    this.#restartButton.removeEventListener("click", this.#handleRestart);
    this.#undoDragButton.removeEventListener("click", this.#handleUndoDrag);
    this.#scrubber.removeEventListener("input", this.#handleSeekInput);
    this.#scrubber.removeEventListener("pointerdown", this.#handlePointerDown);
    this.#scrubber.removeEventListener("pointerup", this.#handlePointerEnd);
    this.#scrubber.removeEventListener("pointercancel", this.#handlePointerEnd);
    this.#previousSection.removeEventListener("click", this.#handlePreviousSection);
    this.#nextSection.removeEventListener("click", this.#handleNextSection);
    this.#sectionNames.removeEventListener("change", this.#handleSectionJump);
    this.#muteButton.removeEventListener("click", this.#handleMute);
    this.#volume.removeEventListener("input", this.#handleVolume);
    renderPlaybackPlaceholder(this.#root);
  }

  #handleKeyboard = event => {
    // Native buttons, menus and the scrubber keep their own keyboard behavior.
    if (event.target !== this.#root) return;
    const button = {
      Space: this.#playButton,
      Home: this.#restartButton,
      ArrowLeft: this.#previousSection,
      ArrowRight: this.#nextSection,
    }[event.code];
    if (!button) return;
    // Neither edge of a viewer shortcut may reach scene-native key bindings.
    event.stopPropagation();
    if (event.type !== "keydown" || event.defaultPrevented || event.isComposing ||
        event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    event.preventDefault();
    if (event.repeat || button.disabled ||
        (event.code.startsWith("Arrow") && this.#sectionControls.hidden)) return;
    button.click();
  };

  #handleToggle = () => {
    if (this.#destroyed || this.#externalBusy ||
        (!this.#controllable && !this.#forwardPlayback)) return;
    const next = !(this.#desiredPlaying ?? this.#playing);
    let activation = null;
    try {
      activation = next && typeof this.#player.preparePlaybackActivation === "function"
        ? this.#player.preparePlaybackActivation() : null;
    } catch (error) {
      this.#reportError(error);
      return;
    }
    this.#desiredPlaying = next;
    this.#pendingActivation = activation;
    this.#render();
    if (!this.#seekActive) void this.#drainTransport();
  };

  #handlePointerDown = event => {
    if (this.#scrubber.disabled) return;
    const range = this.#currentSeekRange();
    if (range === null) return;
    this.#gestureRange = { ...range, pointerId: event.pointerId ?? null };
  };

  #handlePointerEnd = event => {
    if (this.#gestureRange !== null &&
        (event.pointerId === undefined || this.#gestureRange.pointerId === null ||
         event.pointerId === this.#gestureRange.pointerId)) {
      this.#gestureRange = null;
      this.#render();
    }
  };

  #handleRestart = () => {
    void this.#runCommand(async (generation) => {
      const result = await this.#player.restartPlayback();
      if (!this.#destroyed && generation === this.#controlGeneration) this.sync(result);
    });
  };

  #handleUndoDrag = () => {
    if (!this.#undoDragSupported || this.#preparedFrontier !== null) return;
    void this.#runCommand(async (generation) => {
      const result = await this.#player.undoTranslationDrag();
      if (!this.#destroyed && generation === this.#controlGeneration) this.sync(result);
    });
  };

  #handlePreviousSection = () => this.#handleSection(true);
  #handleNextSection = () => this.#handleSection(false);
  #handleSection(previous) {
    void this.#runCommand(async generation => {
      const result = await this.#player.seekSection(previous);
      if (!this.#destroyed && generation === this.#controlGeneration) this.sync(result);
    });
  }

  #handleSectionJump = () => {
    const section = this.#sections.find(section => String(section.index) === this.#sectionNames.value);
    this.#sectionNames.value = "";
    this.#jumpToSection(section);
  };

  #jumpToSection(section) {
    if (!section) return;
    // An explicit user-selected Rust cue supplies the absolute seek target;
    // Previous/Next selection continues to run in shared Rust.
    void this.#runCommand(async generation => {
      const result = await this.#player.seek(section.time);
      if (!this.#destroyed && generation === this.#controlGeneration) this.sync(result);
    });
  }

  #handleMute = () => {
    try {
      this.#player.setAudioMuted(!this.#audioMuted);
      this.#audioMuted = !this.#audioMuted;
      this.#render();
    } catch (error) { this.#reportError(error); }
  };

  #handleVolume = () => {
    try { this.#player.setAudioVolume(Number(this.#volume.value)); }
    catch (error) { this.#reportError(error); }
  };

  #handleSeekInput = () => {
    if (this.#destroyed || !this.#controllable || this.#externalBusy) return;
    const range = this.#gestureRange ?? this.#currentSeekRange();
    if (range === null || range.end < range.start) return;
    const target = Number(this.#scrubber.value);
    if (!Number.isFinite(target)) {
      this.#seekNotice = `Choose a time from ${formatTime(range.start)} to ${formatTime(range.end)} s`;
      this.#render();
      return;
    }
    if (target < range.start || target > range.end) {
      this.#seekNotice = `Choose a time from ${formatTime(range.start)} to ${formatTime(range.end)} s`;
      this.#render();
      return;
    }
    if (range.end === range.start &&
        !(this.#preparedFrontier !== null && this.#durationSeconds === null &&
          this.#preparedFrontier > range.end)) return;
    this.#timeSeconds = target;
    this.#desiredSeek = target;
    this.#seekNotice = `Seeking to ${formatTime(target)} s · waiting for display`;
    this.#renderTime();
    this.#renderStatus();
    void this.#drainSeek();
  };

  async #runCommand(operation, allowForward = false) {
    if (this.#destroyed || (!this.#controllable && !(allowForward && this.#forwardPlayback)) || this.#externalBusy || this.#commandPending || this.#seekActive || this.#transportActive) return;
    const generation = this.#controlGeneration;
    this.#commandPending = true;
    this.#renderDisabled();
    try {
      await operation(generation);
    } catch (error) {
      if (generation === this.#controlGeneration) this.#reportError(error);
    } finally {
      this.#commandPending = false;
      this.#renderDisabled();
    }
  }

  async #drainSeek() {
    if (this.#seekActive || this.#transportActive || this.#commandPending || this.#destroyed) return;
    const generation = this.#controlGeneration;
    this.#seekActive = true;
    this.#renderDisabled();
    try {
      while (!this.#destroyed && generation === this.#controlGeneration && this.#desiredSeek !== null) {
        const target = this.#desiredSeek;
        this.#desiredSeek = null;
        const result = await this.#player.seek(target);
        if (generation !== this.#controlGeneration) break;
        if (this.#destroyed) break;
        this.#acceptSeekResult(result, target);
      }
    } catch (error) {
      if (generation === this.#controlGeneration) {
        this.#desiredSeek = null;
        this.#timeSeconds = this.#confirmedTimeSeconds;
        this.#seekNotice = null;
        this.#renderTime();
        this.#renderStatus();
        this.#reportError(error);
      }
    } finally {
      this.#seekActive = false;
      this.#renderDisabled();
      if (!this.#destroyed && !this.#externalBusy) {
        if (this.#desiredSeek !== null && this.#controllable) void this.#drainSeek();
        else if (this.#desiredPlaying !== null) void this.#drainTransport();
      }
    }
  }

  #acceptSeekResult(result, target) {
    if (result.preparedPlayback !== true) {
      const pendingSeek = this.#desiredSeek;
      const pendingPlaying = this.#desiredPlaying;
      this.sync(result);
      this.#desiredPlaying = pendingPlaying;
      if (pendingPlaying !== null) this.#playing = result.playing;
      if (pendingSeek !== null) this.#timeSeconds = pendingSeek;
      return;
    }
    validatePreparedState(result);
    const staleReadiness = this.#preparedFrontier !== null &&
      (result.preparedFrontier < this.#preparedFrontier ||
       (this.#durationSeconds !== null && result.durationSeconds === null));
    if (!staleReadiness) {
      this.#preparedFrontier = result.preparedFrontier;
      this.#seekRange = result.seekRange;
      this.#durationSeconds = result.durationSeconds;
      this.#buffering = result.buffering;
    }
    const receipt = result.presentationReceipt;
    const matches = Number.isSafeInteger(receipt?.session) && receipt.session >= 0 &&
      Number.isSafeInteger(receipt?.sequence) && receipt.sequence >= 0 &&
      receipt.time === target && result.time === target &&
      (this.#presentedSession !== receipt.session ||
        this.#presentedSequence === null || receipt.sequence >= this.#presentedSequence);
    if (matches) {
      this.#presentedSession = receipt.session;
      this.#presentedSequence = receipt.sequence;
      this.#confirmedTimeSeconds = result.time;
      this.#seekNotice = null;
    } else {
      this.#seekNotice = `Accepted at ${formatTime(result.time)} s · waiting for display`;
    }
    if (this.#desiredSeek === null) this.#timeSeconds = result.time;
    if (this.#desiredPlaying === null) this.#playing = result.playing;
    this.#render();
  }

  async #drainTransport() {
    if (this.#transportActive || this.#seekActive || this.#commandPending ||
        this.#desiredPlaying === null || this.#destroyed) return;
    const generation = this.#controlGeneration;
    const desired = this.#desiredPlaying;
    const activation = desired ? this.#pendingActivation : null;
    this.#pendingActivation = null;
    this.#transportActive = true;
    this.#renderDisabled();
    try {
      const result = desired
        ? await this.#player.resume(activation)
        : await this.#player.pause();
      if (generation !== this.#controlGeneration || this.#destroyed) return;
      if (this.#desiredPlaying === desired) {
        this.#desiredPlaying = null;
        this.sync(result);
      } else {
        this.#playing = result.playing;
        this.#render();
      }
    } catch (error) {
      if (generation === this.#controlGeneration) {
        if (this.#desiredPlaying === desired) {
          this.#desiredPlaying = null;
          this.#pendingActivation = null;
        }
        this.#reportError(error);
      }
    } finally {
      this.#transportActive = false;
      this.#renderDisabled();
      if (!this.#destroyed && !this.#externalBusy) {
        if (this.#desiredSeek !== null) void this.#drainSeek();
        else if (this.#desiredPlaying !== null) void this.#drainTransport();
      }
    }
  }

  #reportError(error) {
    if (this.#destroyed || error?.name === "AbortError") return;
    if (this.#onError !== null) {
      this.#onError(error);
      return;
    }
    console.error(error);
  }

  #render() {
    if (this.#destroyed) return;
    const playing = this.#desiredPlaying ?? this.#playing;
    this.#playButton.textContent = this.#forwardPlayback ? (playing ? "Pause" : "Play")
      : this.#unavailableReason !== null ? "Complete" : !this.#controllable ? "Live" : playing ? "Pause" : "Play";
    if (this.#hasAudio && this.#controllable && !playing && !this.#audioReady) {
      this.#playButton.textContent = "Play with sound";
    }
    this.#playButton.setAttribute(
      "aria-label",
      !this.#controllable && !this.#forwardPlayback ? (this.#unavailableReason !== null ? "Replay unavailable" : "Python owns live playback") : playing ? "Pause animation" : "Play animation",
    );
    const range = this.#currentSeekRange();
    this.#scrubber.hidden = false;
    if (this.#gestureRange === null && range !== null) {
      this.#scrubber.min = String(range.start);
      this.#scrubber.max = String(range.end);
    }
    this.#scrubber.title = range === null ? "Seeking is unavailable" :
      this.#preparedFrontier !== null && this.#durationSeconds === null
        ? `Available history: ${formatTime(range.start)} to ${formatTime(range.end)} seconds; total duration unknown`
        : `Animation time from ${formatTime(range.start)} to ${formatTime(range.end)} seconds`;
    this.#liveStatus.hidden = this.#controllable &&
      !(this.#preparedFrontier !== null && this.#durationSeconds === null);
    this.#renderStatus();
    this.#undoDragButton.hidden = !this.#undoDragSupported || this.#preparedFrontier !== null;
    this.#root.dataset.undoSupport = String(!this.#undoDragButton.hidden);
    this.#root.dataset.buffering = String(this.#buffering);
    this.#root.dataset.durationKnown = String(this.#controllable && this.#durationSeconds !== null);

    this.#audioControls.hidden = !this.#hasAudio;
    this.#sectionControls.hidden = this.#sections.length === 0;
    this.#sectionMarkers.hidden = this.#sections.length === 0 || !this.#controllable || this.#durationSeconds === null;
    const signature = JSON.stringify([this.#sections, this.#durationSeconds]);
    if (signature !== this.#sectionSignature) {
      this.#sectionSignature = signature;
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "Jump to a key point";
      this.#sectionNames.replaceChildren(placeholder);
      this.#sectionMarkers.replaceChildren();
      for (const section of this.#sections) {
        const label = `${section.index + 1}. ${section.name} · ${formatTime(section.time)} s`;
        const option = document.createElement("option");
        option.value = String(section.index);
        option.textContent = label;
        this.#sectionNames.append(option);
        if (this.#durationSeconds !== null && section.time <= this.#durationSeconds) {
          const marker = document.createElement("button");
          marker.type = "button";
          marker.className = "playback-section-marker";
          marker.title = label;
          marker.setAttribute("aria-label", `Jump to ${label}`);
          marker.style.left = `${this.#durationSeconds > 0 ? 100 * section.time / this.#durationSeconds : 0}%`;
          marker.addEventListener("click", () => this.#jumpToSection(section));
          this.#sectionMarkers.append(marker);
        }
      }
      this.#sectionNames.value = "";
    }
    this.#sectionNames.value = this.#sectionState?.current == null ? "" : String(this.#sectionState.current);
    this.#muteButton.textContent = this.#audioMuted ? "Unmute" : "Mute";
    this.#muteButton.setAttribute("aria-pressed", String(this.#audioMuted));
    this.#renderTime();
    this.#renderDisabled();
  }

  #renderTime() {
    if (this.#destroyed) return;
    const known = this.#controllable && this.#durationSeconds !== null;
    const time = known ? Math.min(this.#timeSeconds, this.#durationSeconds) : this.#timeSeconds;
    this.#root.dataset.elapsedSeconds = String(time);
    const range = this.#gestureRange ?? this.#currentSeekRange();
    if (this.#gestureRange === null) {
      this.#scrubber.value = range === null ? "0" :
        String(Math.min(Math.max(this.#desiredSeek ?? time, range.start), range.end));
    }
    this.#scrubber.setAttribute(
      "aria-valuetext",
      known
        ? `${formatTime(time)} seconds of ${formatTime(this.#durationSeconds)} seconds`
        : range === null
          ? `${formatTime(time)} seconds elapsed · seeking unavailable`
          : `${formatTime(time)} seconds elapsed · available through ${formatTime(range.end)} seconds · total duration unknown`,
    );
    this.#timeOutput.value = known
      ? `${formatTime(time)} / ${formatTime(this.#durationSeconds)} s`
      : this.#unavailableReason !== null && !this.#forwardPlayback ? `${formatTime(time)} s · completed` : `${formatTime(time)} / — s`;
  }

  #renderDisabled() {
    if (this.#destroyed) return;
    // Capability denial is not an unfinished command. A completed, non-replayable
    // scene stays disabled without displaying an indefinite wait cursor/aria-busy.
    const busy = this.#externalBusy || this.#commandPending || this.#seekActive || this.#transportActive;
    const blockCommands = !this.#controllable || busy;
    this.#playButton.disabled = this.#externalBusy || (!this.#controllable && !this.#forwardPlayback) ||
      (this.#controllable && this.#durationSeconds === 0);
    this.#restartButton.disabled = blockCommands;
    this.#undoDragButton.disabled = blockCommands || this.#preparedFrontier !== null || !this.#canUndoTranslationDrag;
    const range = this.#currentSeekRange();
    this.#scrubber.disabled = !this.#controllable || this.#externalBusy || this.#commandPending ||
      range === null || range.end < range.start ||
      (range.end === range.start && !(this.#preparedFrontier !== null &&
        this.#durationSeconds === null && this.#preparedFrontier > range.end));
    // Buffering is reader readiness, not a command lock: pause and rewind stay usable.
    this.#previousSection.disabled = blockCommands || this.#sectionState?.previous === null;
    this.#nextSection.disabled = blockCommands || this.#sectionState?.next === null;
    this.#sectionNames.disabled = blockCommands;
    this.#sectionMarkers.disabled = blockCommands;
    this.#muteButton.disabled = this.#externalBusy;
    this.#volume.disabled = this.#externalBusy;
    this.#root.dataset.busy = String(busy);
    this.#root.dataset.playing = String(this.#desiredPlaying ?? this.#playing);
    this.#root.setAttribute("aria-busy", String(busy));
  }

  #renderStatus() {
    if (this.#destroyed) return;
    const range = this.#currentSeekRange();
    const open = this.#preparedFrontier !== null && this.#durationSeconds === null;
    const status = this.#unavailableReason !== null ? "Replay unavailable"
      : this.#seekNotice !== null ? this.#seekNotice
        : open ? `${this.#buffering ? "Buffering · " : ""}Available history: ${range === null ? "none" : `${formatTime(range.start)}–${formatTime(range.end)} s`} · total unknown`
          : range === null ? "Seeking unavailable" : "Duration pending";
    if (this.#liveStatus.textContent !== status) {
      this.#liveStatus.textContent = status;
      this.#liveStatus.title = status;
    }
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
  root.removeAttribute("data-buffering");
  root.removeAttribute("data-duration-known");
  root.removeAttribute("data-undo-support");
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
    .playback-controls:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .playback-controls[data-undo-support="true"] {
      grid-template-columns: auto auto auto minmax(4rem, 1fr) auto;
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
    .playback-scrubber:focus-visible,
    .playback-section-names:focus-visible,
    .playback-section-marker:focus-visible {
      outline: 2px solid var(--accent-strong);
      outline-offset: 2px;
    }
    .playback-timeline {
      display: flex;
      flex-direction: column;
      justify-content: center;
      min-width: 0;
      min-height: 2.5rem;
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
    .playback-section-controls,
    .playback-audio-controls {
      grid-column: 1 / -1;
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 0.45rem;
    }
    .playback-section-names {
      flex: 1 1 9rem;
      min-width: 0;
      max-width: 100%;
      padding: 0.35rem;
      border: 1px solid var(--border-strong);
      border-radius: 0.4rem;
      background: #171d2a;
      color: #d9deeb;
      font-size: 0.7rem;
    }
    .playback-section-markers {
      position: relative;
      min-width: 0;
      height: 1.5rem;
      margin: 0 0.4rem;
      padding: 0;
      border: 0;
    }
    .playback-section-marker {
      position: absolute;
      width: 1.5rem;
      height: 1.5rem;
      padding: 0;
      border: 0;
      border-radius: 0.2rem;
      background: transparent;
      transform: translateX(-50%);
      cursor: pointer;
    }
    .playback-section-marker::before {
      content: "";
      display: block;
      width: 2px;
      height: 0.5rem;
      margin: auto;
      background: var(--accent);
    }
    .playback-section-marker:hover:not(:disabled)::before { background: var(--accent-strong); }
    .playback-volume-label {
      display: flex;
      align-items: center;
      gap: 0.35rem;
      color: #b9c3d6;
      font-size: 0.68rem;
    }
    .playback-volume-label input { width: 5rem; accent-color: var(--accent); }
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
      .playback-controls[data-undo-support="true"] {
        grid-template-columns: auto auto auto minmax(3rem, 1fr) auto;
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

function validatePreparedState(state) {
  const { time, preparedFrontier, durationSeconds, buffering, seekRange } = state;
  const rangeValid = seekRange === null || (seekRange !== undefined &&
    Number.isFinite(seekRange.start) && seekRange.start >= 0 &&
    Number.isFinite(seekRange.end) && seekRange.end >= seekRange.start);
  if (state.preparedPlayback !== true || state.replaySupported !== true ||
      !Number.isFinite(preparedFrontier) || preparedFrontier < 0 ||
      (preparedFrontier === 0 && durationSeconds !== 0) ||
      !Number.isFinite(time) || time < 0 || time > preparedFrontier ||
      !rangeValid || typeof buffering !== "boolean" ||
      (durationSeconds !== null && (!Number.isFinite(durationSeconds) ||
       durationSeconds !== preparedFrontier || buffering || seekRange === null ||
       seekRange.end !== durationSeconds))) {
    throw new TypeError("prepared playback state requires valid selectable bounds, frontier, terminal duration and buffering status");
  }
}

function validateDuration(durationSeconds) {
  if (!Number.isFinite(durationSeconds) || durationSeconds < 0) {
    throw new TypeError("playback duration must be non-negative and finite");
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

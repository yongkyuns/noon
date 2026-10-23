// Installed only by this harness, before the unmodified product page loads.
export function observeAudio() {
  const NativeAudioContext = window.AudioContext;
  const contexts = [], starts = [], stops = [], buffers = [], parameters = [];
  const nodes = [], connections = [], disconnects = [], ended = [], parameterRefs = [];
  const contextIds = new WeakMap(), sourceIds = new WeakMap(), bufferIds = new WeakMap();
  const parameterIds = new WeakMap(), parameterContexts = new WeakMap();
  const sourceGains = new WeakMap(), gainDestinations = new WeakMap(), nodeIds = new WeakMap();
  const pendingResumes = [];
  const connect = AudioNode.prototype.connect;
  const disconnect = AudioNode.prototype.disconnect;
  const start = AudioBufferSourceNode.prototype.start;
  const stop = AudioBufferSourceNode.prototype.stop;
  const parameterValue = Object.getOwnPropertyDescriptor(AudioParam.prototype, "value");
  const contextId = context => contextIds.get(context);
  const parameterId = (parameter, context) => {
    if (!parameterIds.has(parameter)) {
      parameterIds.set(parameter, parameters.length);
      parameters.push({ context: contextId(context), events: [] });
      parameterRefs.push(parameter);
    }
    if (context) parameterContexts.set(parameter, context);
    return parameterIds.get(parameter);
  };
  const nodeId = node => {
    if (!nodeIds.has(node)) {
      nodeIds.set(node, nodes.length);
      nodes.push({ context: contextId(node.context), kind: node.constructor.name,
        ...(node instanceof GainNode ? { parameter: parameterId(node.gain, node.context) } : {}) });
    }
    return nodeIds.get(node);
  };
  const parameterEvent = (parameter, method, args) =>
    parameters[parameterId(parameter)].events.push({ method, args: Array.from(args),
      observedAt: parameterContexts.get(parameter)?.currentTime });
  for (const method of ["setValueAtTime", "linearRampToValueAtTime", "cancelScheduledValues", "cancelAndHoldAtTime"]) {
    const native = AudioParam.prototype[method];
    AudioParam.prototype[method] = function (...args) {
      const value = native.apply(this, args);
      parameterEvent(this, method, args);
      return value;
    };
  }
  Object.defineProperty(AudioParam.prototype, "value", {
    ...parameterValue,
    set(value) { parameterValue.set.call(this, value); parameterEvent(this, "value", [value]); },
  });
  AudioNode.prototype.connect = function (destination, ...args) {
    if (this instanceof AudioBufferSourceNode && destination instanceof GainNode) {
      sourceGains.set(this, destination);
    }
    if (this instanceof GainNode && destination instanceof GainNode) {
      gainDestinations.set(this, destination);
    }
    connections.push({ from: nodeId(this), to: nodeId(destination), observedAt: this.context.currentTime });
    const record = contexts[contextId(this.context)];
    if (record && destination === this.context.destination && this !== record.analyser) {
      // Insert one unity-gain analyser after the actual master node. The native
      // destination remains connected and real device processing still runs.
      connect.call(this, record.analyser, ...args);
      if (!record.connected) {
        connect.call(record.analyser, destination);
        record.connected = true;
      }
      return destination;
    }
    return connect.call(this, destination, ...args);
  };
  AudioNode.prototype.disconnect = function (...args) {
    const value = disconnect.apply(this, args);
    disconnects.push({ node: nodeId(this), observedAt: this.context.currentTime });
    return value;
  };
  AudioBufferSourceNode.prototype.start = function (...args) {
    const value = start.apply(this, args);
    const gain = sourceGains.get(this);
    const bus = gainDestinations.get(gain);
    const master = gainDestinations.get(bus);
    starts.push({ context: contextId(this.context), source: sourceIds.get(this),
      node: nodeId(this), buffer: bufferIds.get(this.buffer),
      gainParameter: gain && parameterId(gain.gain, this.context),
      busParameter: bus && parameterId(bus.gain, this.context),
      masterParameter: master && parameterId(master.gain, this.context),
      busNode: bus && nodeId(bus),
      loop: this.loop, loopStart: this.loopStart, loopEnd: this.loopEnd,
      args, observedAt: this.context.currentTime });
    return value;
  };
  AudioBufferSourceNode.prototype.stop = function (...args) {
    const value = stop.apply(this, args);
    stops.push({ context: contextId(this.context), source: sourceIds.get(this),
      node: nodeId(this), args, observedAt: this.context.currentTime });
    return value;
  };
  class ObservedAudioContext extends NativeAudioContext {
    constructor(...args) {
      super(...args);
      const id = contexts.length;
      contextIds.set(this, id);
      const analyser = this.createAnalyser();
      analyser.fftSize = 2048;
      analyser.smoothingTimeConstant = 0;
      const record = { context: this, analyser, connected: false, states: [this.state] };
      contexts.push(record);
      this.addEventListener("statechange", () => record.states.push(this.state));
    }
    createBuffer(...args) {
      const buffer = super.createBuffer(...args);
      bufferIds.set(buffer, buffers.length);
      buffers.push({ context: contextId(this), channels: buffer.numberOfChannels,
        frames: buffer.length, sampleRate: buffer.sampleRate, duration: buffer.duration });
      return buffer;
    }
    createBufferSource(...args) {
      const source = super.createBufferSource(...args);
      sourceIds.set(source, this.nextObservedSource = (this.nextObservedSource ?? 0) + 1);
      source.addEventListener("ended", () => ended.push({ context: contextId(this),
        source: sourceIds.get(source), node: nodeId(source), observedAt: this.currentTime }));
      return source;
    }
    createGain(...args) {
      const gain = super.createGain(...args);
      nodeId(gain);
      return gain;
    }
    resume() {
      const native = super.resume();
      if (!window.audioQualification.holdNextResume) return native;
      window.audioQualification.holdNextResume = false;
      return new Promise((resolve, reject) => {
        native.then(value => pendingResumes.push(() => resolve(value)), reject);
      });
    }
  }
  window.AudioContext = ObservedAudioContext;
  window.audioQualification = {
    contexts, starts, stops, buffers, parameters, pendingResumes,
    nodes, connections, disconnects, ended,
    holdNextResume: false, renderer: null,
    parameterValue(id) { return parameterRefs[id].value; },
    sample() {
      return contexts.map(({ context, analyser, states }) => {
        const samples = new Float32Array(analyser.fftSize);
        analyser.getFloatTimeDomainData(samples);
        let sum = 0, peak = 0;
        for (const value of samples) { sum += value * value; peak = Math.max(peak, Math.abs(value)); }
        return { state: context.state, deviceTime: context.currentTime, states,
          rms: Math.sqrt(sum / samples.length), peak };
      });
    },
    snapshot() {
      const renderer = this.renderer;
      // Tests may retain several observations within one page.evaluate call.
      // Copy the plain diagnostic data now, before later native events append.
      return structuredClone({ output: this.sample(), starts, stops, buffers,
        parameters: parameters.map((record, id) => ({ ...record, value: this.parameterValue(id) })),
        nodes, connections, disconnects, ended,
        transport: renderer ? { time: renderer.time(), playing: renderer.isDirectPlaying(),
          enabled: renderer.audioEnabled(), voices: renderer.audioVoiceCount(),
          volume: renderer.audioVolume(), muted: renderer.audioMuted(),
          sceneRevision: String(renderer.directSceneRevision()) } : null });
    },
  };
}


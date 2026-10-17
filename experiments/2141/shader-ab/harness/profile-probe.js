import * as wasmExports from './pkg/noon_web.js';

const profile = globalThis.__noonProfile = {
  schema: 1, timeOrigin: performance.timeOrigin, events: [], totals: {}, dropped: 0,
};
const mark = globalThis.__noonProfileMark = (type, details = {}) => {
  if (profile.events.length >= 100000) { profile.dropped++; return; }
  profile.events.push({type, at: performance.now(), ...details});
};
for (const [className, value] of Object.entries(wasmExports)) {
  if (typeof value !== 'function' || !value.prototype) continue;
  for (const method of Object.getOwnPropertyNames(value.prototype)) {
    if (method === 'constructor') continue;
    const descriptor = Object.getOwnPropertyDescriptor(value.prototype, method);
    if (typeof descriptor.value !== 'function') continue;
    const original = descriptor.value;
    const name = `${className}.${method}`;
    Object.defineProperty(value.prototype, method, {...descriptor, value: function(...args) {
      const start = performance.now();
      try { return Reflect.apply(original, this, args); }
      finally {
        const elapsed = performance.now() - start;
        const total = profile.totals[name] ??= {count: 0, ms: 0, max: 0};
        total.count++; total.ms += elapsed; total.max = Math.max(total.max, elapsed);
        if (elapsed >= 1 || /driveLiveSegment|completeLiveSegment|resumeExecutionPlayer|returnExecutionPlayer|drainDeltaTransportJson|liveSegmentWake/.test(method)) {
          mark('api', {name, start, ms: elapsed});
        }
      }
    }});
  }
}
if (typeof globalThis.requestAnimationFrame === 'function') {
  const request = globalThis.requestAnimationFrame.bind(globalThis);
  const cancel = globalThis.cancelAnimationFrame.bind(globalThis);
  globalThis.requestAnimationFrame = callback => {
    const start = performance.now();
    const id = request(timestamp => {
      mark('raf_fire', {id, start, timestamp, ms: performance.now() - start});
      callback(timestamp);
    });
    mark('raf_request', {id, start});
    return id;
  };
  globalThis.cancelAnimationFrame = id => { mark('raf_cancel', {id}); return cancel(id); };
}
const portPost = MessagePort.prototype.postMessage;
MessagePort.prototype.postMessage = function(message, ...args) {
  if (['tick', 'execution_wake', 'execution_presented'].includes(message?.type)) {
    mark('port_post', {kind: message.type, cadence: message.cadence,
      sequence: message.sequence, session: message.session, timestamp: message.timestamp});
  }
  return Reflect.apply(portPost, this, [message, ...args]);
};

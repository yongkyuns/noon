// Keep cooperative worker turns as tasks: microtasks starve incoming controls,
// while nested zero-delay timers accumulate browser timer-clamping delays.
export function createWorkerTaskYield(channel = new MessageChannel()) {
  const pending = [];
  channel.port1.onmessage = () => pending.shift()();
  return () => new Promise(resolve => {
    pending.push(resolve);
    channel.port2.postMessage(null);
  });
}

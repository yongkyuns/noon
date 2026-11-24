// Qualification only. Native event causality, not another audio clock.
// Web Audio renders in blocks and drains control messages before the next block:
// https://www.w3.org/TR/webaudio/#rendering-loop
// A trusted ended event witnesses source completion even if the control thread's
// currentTime read still names the block containing the scheduled stop.
// https://www.w3.org/TR/webaudio/#dom-audioscheduledsourcenode-onended
export function releaseRetirementWitness(snapshot, source, deadline) {
  const ordered = event => Number.isSafeInteger(event?.order) && event.order >= 0;
  const sameSource = event => event.node === source.node && event.context === source.context;
  if (!Number.isFinite(deadline) || source.loop
      || source.args?.length !== 3
      || !Number.isFinite(source.args[0] + source.args[2])
      || source.args[0] + source.args[2] < deadline) return null;
  const buffer = snapshot.buffers?.[source.buffer];
  if (!buffer || !Number.isFinite(buffer.duration) || source.args[1] < 0
      || source.args[0] + Math.min(source.args[2], buffer.duration - source.args[1]) < deadline) return null;
  const stops = snapshot.stops.filter(sameSource);
  if (stops.some(event => !ordered(event))) return null;
  const disconnects = snapshot.disconnects.filter(event =>
    event.node === source.busNode && event.context === source.context);
  // The first disconnection matters: a later valid one cannot hide an early cut.
  if (!disconnects.length || disconnects.some(event => !ordered(event))) return null;
  const disconnect = disconnects.reduce((a,b) => a.order < b.order ? a : b);
  const priorStops = stops.filter(event => event.order < disconnect.order);
  const stop = priorStops.find(event => event.args.length === 1
    && event.args[0] === deadline && event.observedAt < deadline);
  if (!stop) return null;
  const ended = snapshot.ended.find(event => sameSource(event) && ordered(event)
    && event.trusted === true && event.order > stop.order);
  if (!ended) return null;
  const earlyOverride = priorStops.some(event => event.order > stop.order
    && (event.args.length !== 1 || event.args[0] !== deadline)
    && event.observedAt < deadline && event.order < ended.order);
  if (earlyOverride) return null;
  const deviceTime = disconnect.observedAt >= deadline;
  const nativeEnded = ended.order < disconnect.order;
  if (!deviceTime && !nativeEnded) return null;
  // A premature immediate gain write could cut the tail even if disconnection
  // waited. Scheduled release automation is checked separately without changes.
  const writes = snapshot.parameters[source.busParameter]?.events ?? [];
  if (writes.some(event => !ordered(event))) return null;
  const earlySilence = writes.some(event => event.order > stop.order
    && event.order < disconnect.order && event.method === "value" && event.args[0] === 0
    && event.observedAt < deadline && event.order < ended.order);
  if (earlySilence) return null;
  return { kind: deviceTime ? "device-time" : "native-ended", deviceTime, nativeEnded, stop, ended, disconnect };
}

// External browser qualification helper over the normal direct host clock.
export async function sampleDirectProgram(renderer, time) {
  let wallTimeMs = time * 1000;
  if (time > 0) renderer.advanceDirectRealtime(wallTimeMs);
  for (let attempt = 0; attempt < 100; attempt++) {
    const wake = JSON.parse(renderer.directWakeDirectiveJson(wallTimeMs));
    if (!wake.presentNow) {
      const actual = renderer.time();
      // Seconds -> milliseconds -> reanchored seconds can round just
      // below an interval boundary. Cross only that representational
      // gap; a substantive clock mismatch still fails the assertions.
      if (actual < time && time - actual <= 16 * Number.EPSILON * Math.max(1, time)) {
        wallTimeMs += Number.EPSILON * Math.max(1, wallTimeMs);
        renderer.advanceDirectRealtime(wallTimeMs);
        continue;
      }
      return { time: actual, objectCount: renderer.objectCount(),
        backend: renderer.rendererBackend(), drawCalls: renderer.lastDrawCalls(),
        cadence: wake.cadence, delayMs: wake.delayMs };
    }
    if (!renderer.render()) await new Promise(resolve => setTimeout(resolve, 10));
  }
  throw new Error("direct program publication did not settle");
}

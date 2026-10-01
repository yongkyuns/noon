import assert from "node:assert/strict";

export function resolvePageTargetCpuThrottleRate(profile, override) {
  assert.ok(["desktop", "mobile-class"].includes(profile), `unknown profile: ${profile}`);
  if (override === undefined) return profile === "mobile-class" ? 4 : 1;
  assert.ok(["1", "4"].includes(override), "page CPU throttle override must be 1 or 4");
  return Number(override);
}

export function summarizePageTargetThrottleSamples(samples) {
  const median = (values, rate) => {
    assert.ok(values.length >= 2, `need at least two samples at ${rate}x`);
    assert.ok(values.every((value) => Number.isFinite(value) && value > 0),
      `all samples at ${rate}x must be finite and positive`);
    const ordered = [...values].sort((left, right) => left - right);
    const middle = Math.floor(ordered.length / 2);
    return ordered.length % 2 === 0
      ? (ordered[middle - 1] + ordered[middle]) / 2
      : ordered[middle];
  };
  const elapsedForRate = (rate) => samples
    .filter((sample) => sample.requestedPageTargetRate === rate)
    .map((sample) => sample.elapsedMs);
  const oneX = median(elapsedForRate(1), 1);
  const fourX = median(elapsedForRate(4), 4);
  assert.ok(Number.isFinite(oneX) && oneX > 0, "1x median must be finite and positive");
  assert.ok(Number.isFinite(fourX) && fourX > 0, "4x median must be finite and positive");
  return {
    medianElapsedMsByRequestedPageTargetRate: { "1": oneX, "4": fourX },
    observedFourXToOneXRatio: fourX / oneX,
  };
}

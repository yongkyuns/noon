import assert from "node:assert/strict";
import test from "node:test";

// Pinned to Manim Community v0.21.0:
// - manim/animation/animation.py: Animation.get_sub_alpha
// - manim/animation/creation.py: Create, Uncreate, Write,
//   AddTextLetterByLetter, RemoveTextLetterByLetter
//
// Keep this oracle renderer-independent. Production retained Text animation must
// reproduce these per-family-member values; an object-wide reveal scalar is not a
// sufficient substitute.

function manimSmooth(value) {
  const t = Math.max(0, Math.min(1, Number(value)));
  const inflection = 10;
  const sigmoid = (x) => 1 / (1 + Math.exp(-x));
  const error = sigmoid(-inflection / 2);
  return (sigmoid(inflection * (t - 0.5)) - error) / (1 - 2 * error);
}

function linear(value) {
  return Math.max(0, Math.min(1, Number(value)));
}

function getSubAlpha({
  alpha,
  index,
  count,
  lagRatio,
  reverseRateFunction = false,
  rateFunc = manimSmooth,
}) {
  assert.ok(Number.isInteger(index) && index >= 0 && index < count);
  assert.ok(Number.isInteger(count) && count > 0);
  const fullLength = (count - 1) * lagRatio + 1;
  const value = alpha * fullLength;
  const lower = index * lagRatio;
  return reverseRateFunction
    ? rateFunc(1 - (value - lower))
    : rateFunc(value - lower);
}

function familyProgress(options) {
  return Array.from({ length: options.count }, (_, index) =>
    getSubAlpha({ ...options, index }),
  );
}

function assertClose(actual, expected, tolerance = 1e-12) {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `expected ${expected}, got ${actual}`,
  );
}

function writeDefaults(familyMemberCount, { runTime = null, lagRatio = null } = {}) {
  assert.ok(Number.isInteger(familyMemberCount) && familyMemberCount >= 0);
  return {
    runTime: runTime ?? (familyMemberCount < 15 ? 1 : 2),
    lagRatio:
      lagRatio ?? Math.min(4 / Math.max(1, familyMemberCount), 0.2),
  };
}

function letterByLetterRunTime(
  renderedCharacterCount,
  { frameRate = 30, timePerChar = 0.1, runTime = null } = {},
) {
  assert.ok(Number.isInteger(renderedCharacterCount) && renderedCharacterCount >= 0);
  return (
    runTime ??
    Math.max(1 / frameRate, timePerChar) * renderedCharacterCount
  );
}

test("Manim Create(Text) default lag produces non-uniform character progress", () => {
  const quarter = familyProgress({
    alpha: 0.25,
    count: 4,
    lagRatio: 1,
  });

  // full_length = 4. At alpha=.25 only the first rendered character has
  // completed. This is the core invariant an object-wide Text reveal cannot
  // represent.
  assertClose(quarter[0], 1);
  assertClose(quarter[1], 0);
  assertClose(quarter[2], 0);
  assertClose(quarter[3], 0);
  assert.notEqual(new Set(quarter.map((value) => value.toFixed(12))).size, 1);

  const midpoint = familyProgress({
    alpha: 0.5,
    count: 5,
    lagRatio: 1,
  });
  assertClose(midpoint[0], 1);
  assertClose(midpoint[1], 1);
  assertClose(midpoint[2], 0.5);
  assertClose(midpoint[3], 0);
  assertClose(midpoint[4], 0);
});

test("Manim Uncreate keeps family order and reverses each subanimation rate", () => {
  const early = familyProgress({
    alpha: 0.2,
    count: 5,
    lagRatio: 1,
    reverseRateFunction: true,
  });

  // Uncreate does not simply run the Create family ordering backwards. With
  // reverse_rate_function=True, the first character is erased first while later
  // characters remain complete.
  assertClose(early[0], 0);
  assertClose(early[1], 1);
  assertClose(early[2], 1);
  assertClose(early[3], 1);
  assertClose(early[4], 1);

  const midpoint = familyProgress({
    alpha: 0.5,
    count: 5,
    lagRatio: 1,
    reverseRateFunction: true,
  });
  assertClose(midpoint[0], 0);
  assertClose(midpoint[1], 0);
  assertClose(midpoint[2], 0.5);
  assertClose(midpoint[3], 1);
  assertClose(midpoint[4], 1);
});

test("rate function is applied after per-member lag mapping", () => {
  const alpha = 0.35;
  const count = 4;
  const lagRatio = 0.2;
  const fullLength = (count - 1) * lagRatio + 1;

  const expected = Array.from({ length: count }, (_, index) =>
    manimSmooth(alpha * fullLength - index * lagRatio),
  );
  const actual = familyProgress({ alpha, count, lagRatio });
  actual.forEach((value, index) => assertClose(value, expected[index]));

  // Applying smooth once to the object-wide alpha and then lagging that result is
  // a different operation. Keep this inequality explicit so retained runtime code
  // cannot accidentally move the easing before family scheduling.
  const globallyEased = manimSmooth(alpha);
  const wrong = Array.from({ length: count }, (_, index) =>
    linear(globallyEased * fullLength - index * lagRatio),
  );
  assert.ok(
    actual.some((value, index) => Math.abs(value - wrong[index]) > 1e-3),
    "per-character easing must not collapse into globally eased reveal progress",
  );
});

test("Manim Write derives duration and lag ratio from family size", () => {
  assert.deepEqual(writeDefaults(0), { runTime: 1, lagRatio: 0.2 });
  assert.deepEqual(writeDefaults(5), { runTime: 1, lagRatio: 0.2 });
  assert.deepEqual(writeDefaults(14), { runTime: 1, lagRatio: 0.2 });
  assert.deepEqual(writeDefaults(15), { runTime: 2, lagRatio: 0.2 });
  assert.deepEqual(writeDefaults(20), { runTime: 2, lagRatio: 0.2 });
  assert.deepEqual(writeDefaults(40), { runTime: 2, lagRatio: 0.1 });
  assert.deepEqual(writeDefaults(40, { runTime: 3, lagRatio: 0.05 }), {
    runTime: 3,
    lagRatio: 0.05,
  });
});

test("Manim letter-by-letter runtime is character-count based and frame-rate bounded", () => {
  assertClose(letterByLetterRunTime(5), 0.5);
  assertClose(letterByLetterRunTime(5, { timePerChar: 0.01 }), 5 / 30);
  assertClose(letterByLetterRunTime(5, { frameRate: 60, timePerChar: 0.01 }), 5 / 60);
  assertClose(letterByLetterRunTime(5, { runTime: 2.25 }), 2.25);
});

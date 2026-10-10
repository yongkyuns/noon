// Regression contract for the original Manim SquareToCircle gallery source.
// Each default play lasts one authored second: Create [0,1], Transform (1,2),
// FadeOut [2,3]. The intermediate sample must be strictly *inside* the
// transformation (not a retained start/end frame), and the live WebKit smoke
// independently verifies pixels and final completion after finding it.
export const SQUARE_TO_CIRCLE_TRANSFORM_START = 1;
export const SQUARE_TO_CIRCLE_TRANSFORM_END = 2;

export function isSquareToCircleTransformFrame(sample) {
  return Number.isFinite(sample?.time)
    && sample.time > SQUARE_TO_CIRCLE_TRANSFORM_START
    && sample.time < SQUARE_TO_CIRCLE_TRANSFORM_END
    && sample.objectCount === 1;
}

// Spread at most maxSamples over the measured frame range without retaining
// the full frame history. The integer boundaries distribute stride remainders
// deterministically, including frame counts that exceed Number's exact products.
export function shouldSampleRendererStageFrame(frameIndex, frameCount, maxSamples = 32) {
  if (!Number.isSafeInteger(frameIndex) || frameIndex < 0 ||
      !Number.isSafeInteger(frameCount) || frameCount <= 0 || frameIndex >= frameCount ||
      !Number.isSafeInteger(maxSamples) || maxSamples <= 0) {
    throw new RangeError("invalid renderer stage sample frame bounds");
  }
  const sampleCount = BigInt(Math.min(frameCount, maxSamples));
  const totalFrames = BigInt(frameCount);
  const frame = BigInt(frameIndex);
  return ((frame + 1n) * sampleCount / totalFrames) >
    (frame * sampleCount / totalFrames);
}

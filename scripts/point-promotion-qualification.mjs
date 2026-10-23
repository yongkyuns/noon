import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_POINT_PROMOTION_ARTIFACTS
    ?? "browser-smoke-artifacts/point-promotion",
  cases: [0.125, 0.375, 0.625, 0.75].map((sampleTime, index) => ({
    id: ["initial-order", "left-promoted", "both-promoted", "completed-endpoint"][index],
    file: "manim_parity_point_promotion.py", scene: "PointPromotion",
    factory: "createDirectPointPromotionRenderer",
    // Cleared roots keep their execution rows; each promotion adds one child.
    objectCount: [4, 5, 6, 6][index], duration: 0.75, sampleTime, boundaries: [0.25, 0.5],
    playback: "live", stopAtSourceCompletion: index === 3,
    ...(index < 3 ? { directHeldSampleTime: index * 0.25 } : {}),
  })),
});

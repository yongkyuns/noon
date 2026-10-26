import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_OBJECT_MEMBERSHIP_ARTIFACTS
    ?? "browser-smoke-artifacts/object-membership",
  cases: [0.125, 0.375, 0.5].map((sampleTime, index) => ({
    id: ["initial-members", "replaced-and-copied", "completed-endpoint"][index],
    file: "manim_parity_object_members.py", scene: "ObjectMembers",
    factory: "createDirectObjectMembershipQueriesRenderer",
    objectCount: index === 0 ? 3 : 6,
    duration: 0.5, sampleTime, boundaries: [0.25],
    playback: "live", stopAtSourceCompletion: index === 2,
    ...(index < 2 ? { directHeldSampleTime: index * 0.25 } : {}),
  })),
});

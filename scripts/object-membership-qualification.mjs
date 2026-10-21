import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

// Endpoint proof for the native scene and the completed Python continuation.
// Interior structural history and seek parity remain separate obligations.
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_OBJECT_MEMBERSHIP_ARTIFACTS
    ?? "browser-smoke-artifacts/object-membership",
  cases: [{
    id: "object-membership-copy", file: "manim_parity_object_members.py", scene: "ObjectMembers",
    factory: "createDirectObjectMembershipQueriesRenderer", objectCount: 6,
    duration: 0.5, sampleTime: 0.5, boundaries: [0.25], stopAtSourceCompletion: true,
  }],
});

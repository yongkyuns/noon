import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_FAMILY_QUERY_ARTIFACTS ?? "family-query-artifacts",
  cases: [0.125, 0.375, 0.625].map((sampleTime, index) => ({
    id: ["captured", "current", "own-root"][index],
    file: "manim_parity_family_member_queries.py", scene: "FamilyMemberQueries",
    factory: "createDirectFamilyMemberQueriesSmokeRenderer",
    duration: 0.75, sampleTime, objectCount: index === 2 ? 4 : 2,
    playback: "live", boundaries: [0.25, 0.5], directHeldSampleTime: index * 0.25,
  })),
});

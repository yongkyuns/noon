import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_FAMILY_ROOT_ARTIFACTS ?? "family-root-artifacts",
  cases: [0.125, 0.375, 0.625].map((sampleTime, index) => ({
    id: ["owned", "cleared", "restored"][index],
    file: "manim_parity_family_root_drawing.py", scene: "FamilyRootDrawing",
    factory: "createDirectFamilyRootPointsSmokeRenderer",
    duration: 0.75, sampleTime, objectCount: 4,
  })),
});

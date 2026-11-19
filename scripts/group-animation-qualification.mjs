import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

await qualifyPairedAuthoring({
  cases: [...[0, 1.375, 3.125, 4.875, 6.5].map(sampleTime => ({
    id: `group-animation-${sampleTime}`, file: "showcase_group_animation.py",
    factory: "createDirectGroupAnimationSmokeRenderer", objectCount: sampleTime < 0.75 ? 6 : 16,
    duration: 6.5, sampleTime, playback: "live",
    boundaries: [0.75, 2, 2.5, 3.75, 4.25, 5.5],
    stopAtSourceCompletion: sampleTime === 6.5,
  })), ...[0, 1, 1.75, 2.5, 4].map(sampleTime => ({
    id: `mounted-callout-${sampleTime}`, file: "showcase_mounted_callout.py",
    factory: "createDirectMountedCalloutSmokeRenderer", objectCount: 7,
    duration: 4, sampleTime, playback: "live",
    boundaries: [0.5, 1.5, 2, 3],
    directHeldSampleTime: sampleTime === 1.75 ? 1.5 : undefined,

    stopAtSourceCompletion: sampleTime === 4,
  }))],
  artifactDirectory: "browser-smoke-artifacts/group-animation",
  prepareContext: disableAuthoringJspi,
});

import { MessageChannel } from "node:worker_threads";
import { performance } from "node:perf_hooks";

import {
  validateSceneDocument,
  validateSceneIdentities,
} from "./authoring-client.js";
import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";

const DEFAULT_SIZES = [1_000, 10_000, 100_000];
const DEFAULT_WARMUPS = 2;
const DEFAULT_SAMPLES = 10;

const { config, sizes } = parseArgs(process.argv.slice(2));
console.log(
  `Noon browser scene pipeline benchmark (${config.warmups} warmups, ${config.samples} samples)`,
);
console.log();
console.log("| Objects | Scene payload | Operation | Median | p95 |");
console.log("|---:|---:|---|---:|---:|");

for (const objectCount of sizes) {
  await benchmarkSize(objectCount);
}

async function benchmarkSize(objectCount) {
  const first = buildScene(objectCount, 1);
  const second = buildScene(objectCount, 0.5);
  const payload = {
    channel: "noon.authoring",
    protocolVersion: 4,
    type: "scene_document",
    requestId: 0,
    document: first.document,
    identities: first.identities,
  };
  const payloadBytes = Buffer.byteLength(JSON.stringify(payload));
  const resultJson = JSON.stringify({
    kind: "scene_document",
    document: first.document,
    identities: first.identities,
  });
  const encodedPayload = {
    channel: "noon.authoring",
    protocolVersion: 4,
    type: "result",
    requestId: 0,
    resultJson,
  };

  const clone = await measure(async () => {
    await cloneThroughMessagePort(payload);
  });
  const encodedClone = await measure(async () => {
    await cloneThroughMessagePort(encodedPayload);
  });
  const parse = await measure(() => {
    JSON.parse(resultJson);
  });
  const validation = await measure(() => {
    validateSceneDocument(first.document);
    validateSceneIdentities(first.identities, first.document);
  });

  const identityMap = new SceneIdentityMap();
  identityMap.stabilize(first.document, first.identities);
  const stabilization = await measure(() => {
    identityMap.stabilize(first.document, first.identities);
  });

  const stableFirst = identityMap.stabilize(first.document, first.identities);
  const stableSecond = identityMap.stabilize(second.document, second.identities);
  const diff = await measure(() => {
    const patches = diffSceneDocuments(stableFirst, stableSecond);
    if (patches?.length !== 1) {
      throw new Error("benchmark scene must produce one style patch");
    }
  });
  const patches = diffSceneDocuments(stableFirst, stableSecond);
  const patchSerialization = await measure(() => {
    JSON.stringify({ version: 1, sequence: 0, patches });
  });
  const sceneSerialization = await measure(() => {
    JSON.stringify(stableSecond);
  });

  let current = stableFirst;
  let iteration = 0;
  const rerunResults = [second, first].map((source) =>
    JSON.stringify({
      kind: "scene_document",
      document: source.document,
      identities: source.identities,
    }),
  );
  const pipeline = await measure(() => {
    const source = JSON.parse(rerunResults[iteration % 2]);
    validateSceneDocument(source.document);
    validateSceneIdentities(source.identities, source.document);
    const stable = identityMap.stabilize(source.document, source.identities);
    const nextPatches = diffSceneDocuments(current, stable);
    if (nextPatches === null) {
      throw new Error("benchmark rerun unexpectedly requires replacement");
    }
    JSON.stringify({ version: 1, sequence: iteration, patches: nextPatches });
    current = stable;
    iteration += 1;
  });

  const size = formatMiB(payloadBytes);
  printRow(objectCount, size, "worker message clone", clone);
  printRow(objectCount, size, "encoded message clone", encodedClone);
  printRow(objectCount, size, "parse encoded result", parse);
  printRow(objectCount, size, "validate", validation);
  printRow(objectCount, size, "stabilize identities", stabilization);
  printRow(objectCount, size, "diff one style", diff);
  printRow(objectCount, size, "serialize one patch", patchSerialization);
  printRow(objectCount, size, "serialize full scene", sceneSerialization);
  printRow(objectCount, size, "rerun pipeline", pipeline);
}

function buildScene(objectCount, targetOpacity) {
  const target = Math.floor(objectCount / 2);
  const objects = Array.from({ length: objectCount }, (_, id) => ({
    id,
    geometry: { circle: { radius: 0.5 } },
    transform: {
      translation: { x: id, y: 0 },
      rotation: 0,
      scale: { x: 1, y: 1 },
    },
    style: {
      fill: { red: 1, green: 1, blue: 1, alpha: 1 },
      stroke: null,
      stroke_width: 0,
      opacity: id === target ? targetOpacity : 1,
    },
  }));
  return {
    document: { version: 1, objects, tracks: [] },
    identities: {
      objects: objects.map(({ id }) => ({ id, key: `circle-${id}` })),
      tracks: [],
    },
  };
}

function cloneThroughMessagePort(payload) {
  return new Promise((resolve, reject) => {
    const { port1, port2 } = new MessageChannel();
    port1.once("message", () => {
      port1.close();
      port2.close();
      resolve();
    });
    port1.once("messageerror", reject);
    port2.postMessage(payload);
  });
}

async function measure(operation) {
  for (let iteration = 0; iteration < config.warmups; iteration += 1) {
    await operation(iteration);
  }
  const durations = [];
  for (let sample = 0; sample < config.samples; sample += 1) {
    const started = performance.now();
    await operation(config.warmups + sample);
    durations.push(performance.now() - started);
  }
  durations.sort((left, right) => left - right);
  return {
    median: percentile(durations, 0.5),
    p95: percentile(durations, 0.95),
  };
}

function percentile(sorted, value) {
  const rank = Math.ceil(value * sorted.length);
  return sorted[Math.max(0, Math.min(sorted.length - 1, rank - 1))];
}

function printRow(objectCount, size, operation, timing) {
  console.log(
    `| ${objectCount} | ${size} | ${operation} | ${timing.median.toFixed(3)} ms | ${timing.p95.toFixed(3)} ms |`,
  );
}

function formatMiB(bytes) {
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}

function parseArgs(args) {
  const config = { warmups: DEFAULT_WARMUPS, samples: DEFAULT_SAMPLES };
  const sizes = [];
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--warmups" || argument === "--samples") {
      const name = argument.slice(2);
      config[name] = parsePositive(name, args[(index += 1)]);
    } else {
      sizes.push(parsePositive("object count", argument));
    }
  }
  return { config, sizes: sizes.length > 0 ? sizes : DEFAULT_SIZES };
}

function parsePositive(name, value) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer, got ${value}`);
  }
  return parsed;
}

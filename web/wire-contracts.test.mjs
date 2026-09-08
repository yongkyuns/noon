import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { AUTHORING_CHANNEL, AUTHORING_PROTOCOL_VERSION, parseAuthoringResult } from "./authoring-client.js";

async function fixture(path) {
  return JSON.parse(await readFile(new URL(`../compat/wire/${path}`, import.meta.url), "utf8"));
}

test("wire manifest and JS constants stay synchronized", async () => {
  const manifest = JSON.parse(await readFile(new URL("../compat/wire-contracts-v1.json", import.meta.url), "utf8"));
  assert.equal(manifest.authoring_protocol.channel, AUTHORING_CHANNEL);
  assert.equal(manifest.authoring_protocol.version, AUTHORING_PROTOCOL_VERSION);
});

test("shared authoring result fixture requires a semantic execution descriptor", async () => {
  const result = await fixture("v1/authoring-result-empty-scene.json");
  assert.deepEqual(parseAuthoringResult(JSON.stringify(result)), {
    kind: "semantic_scene", semanticExecution: { contextId: "fixture-scene" }, duration: 0,
  });
  const retired = await fixture("invalid/retired-authoring-export.json");
  assert.throws(() => parseAuthoringResult(JSON.stringify(retired)), /Unknown Python authoring result kind/);
});

test("authoring envelope fixture pins channel and protocol generation", async () => {
  const envelope = await fixture("v1/authoring-envelope-ready.json");
  assert.deepEqual(envelope, {channel: AUTHORING_CHANNEL, protocolVersion: AUTHORING_PROTOCOL_VERSION, type: "ready"});
});
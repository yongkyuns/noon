import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import { FrameArtifactStore } from "./agent-preview-artifacts.mjs";
import { PreviewFrameArtifactSession } from "./agent-preview-capture.mjs";

function png1x1() {
  const png = Buffer.alloc(57);
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]).copy(png, 0);
  png.writeUInt32BE(13, 8); png.write("IHDR", 12, "ascii");
  png.writeUInt32BE(1, 16); png.writeUInt32BE(1, 20);
  png[24] = 8; png[25] = 6;
  png.writeUInt32BE(0, 29);
  png.writeUInt32BE(0, 33); png.write("IDAT", 37, "ascii"); png.writeUInt32BE(0, 41);
  png.writeUInt32BE(0, 45); png.write("IEND", 49, "ascii"); png.writeUInt32BE(0, 53);
  return png;
}

const sample = () => ({
  error: null, presented: true, requestedTime: 1.5, publishedTime: 1.5,
  rendererBackend: "WebGL2", time: 1.5, frameIndex: 45,
});

test("capture binds computed source identity and observed sample metadata", () => {
  const store = new FrameArtifactStore();
  const source = "from noon import *\nclass Demo(Scene): pass\n";
  const session = new PreviewFrameArtifactSession({ store, sessionId: "session-a", source });
  assert.equal(session.sourceSha256, createHash("sha256").update(source).digest("hex"));
  const bytes = png1x1();
  const descriptor = session.retain(sample(), bytes);
  bytes.fill(0);
  const retained = session.read(descriptor.id);
  assert.equal(retained.descriptor.provenance.sourceSha256, session.sourceSha256);
  assert.deepEqual(retained.descriptor.provenance.build, {
    engineRevision: null, wasmSha256: null, workerSha256: null, buildId: null,
  });
  assert.equal(retained.descriptor.provenance.requestedTime, 1.5);
  assert.equal(retained.descriptor.provenance.publishedTime, 1.5);
  assert.equal(retained.descriptor.provenance.backend, "WebGL2");
  assert.equal(retained.descriptor.provenance.sceneRevision, null);
  assert.equal(retained.descriptor.provenance.frameRevision, null);
  assert.equal(retained.png[0], 137);
  assert.equal(session.close(), true);
  assert.throws(() => session.read(descriptor.id), /closed/);
  assert.equal(session.close(), false);
  assert.deepEqual(store.stats(), { scopes: 0, artifacts: 0, encodedBytes: 0, disposed: false });
});

test("failed, unpresented and accessor-backed samples are rejected before storage", () => {
  const store = new FrameArtifactStore();
  const session = new PreviewFrameArtifactSession({ store, sessionId: "session-b", source: "scene" });
  assert.throws(() => session.retain({ ...sample(), error: "lost" }, png1x1()), /failed/);
  assert.throws(() => session.retain({ ...sample(), presented: false }, png1x1()), /unpresented/);
  let invoked = false;
  const hostile = { ...sample() };
  Object.defineProperty(hostile, "requestedTime", { get() { invoked = true; return 1; }, enumerable: true });
  assert.throws(() => session.retain(hostile, png1x1()), /data field/);
  assert.equal(invoked, false);
  assert.equal(store.stats().artifacts, 0);
  session.close();
});

test("source validation happens before opening artifact ownership", () => {
  const store = new FrameArtifactStore();
  assert.throws(() => new PreviewFrameArtifactSession({ store, sessionId: "x", source: "" }), /non-empty/);
  assert.throws(() => new PreviewFrameArtifactSession({ store, sessionId: "x", source: "x".repeat(1_000_001) }), /byte limit/);
  assert.equal(store.stats().scopes, 0);
});

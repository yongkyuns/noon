import assert from "node:assert/strict";
import test from "node:test";

import { GalleryInteractionController } from "./gallery-interaction.js";

function fakePlayer() {
  const calls = [];
  return {
    calls,
    async setPointerFillSelection(value) {
      calls.push(value);
    },
  };
}

const interactive = {
  interaction: { type: "pointer-fill-selection", maxMovement: 4 },
};

test("ordinary examples do not emit a default disable call", async () => {
  const controller = new GalleryInteractionController();
  const player = fakePlayer();
  await controller.apply(player, { interaction: null });
  assert.deepEqual(player.calls, []);
});

test("selection is enabled after adopting an interactive session", async () => {
  const controller = new GalleryInteractionController();
  const player = fakePlayer();
  await controller.apply(player, interactive);
  assert.deepEqual(player.calls, [4]);
});

test("enabled selection is disabled before reconciling into another scene", async () => {
  const controller = new GalleryInteractionController();
  const player = fakePlayer();
  await controller.apply(player, interactive);
  await controller.beforeReconcile(player);
  assert.deepEqual(player.calls, [4, null]);
  await controller.apply(player, { interaction: null });
  assert.deepEqual(player.calls, [4, null]);
});

test("reconciled interactive sessions re-enable because host configuration is not replayed", async () => {
  const controller = new GalleryInteractionController();
  const player = fakePlayer();
  await controller.apply(player, interactive);
  await controller.beforeReconcile(player);
  await controller.apply(player, interactive);
  assert.deepEqual(player.calls, [4, null, 4]);
});

test("runtime replacement forgets prior host configuration without disabling the new default session", async () => {
  const controller = new GalleryInteractionController();
  const first = fakePlayer();
  const replacement = fakePlayer();
  await controller.apply(first, interactive);
  controller.resetForSession(replacement);
  await controller.apply(replacement, { interaction: null });
  assert.deepEqual(first.calls, [4]);
  assert.deepEqual(replacement.calls, []);
});

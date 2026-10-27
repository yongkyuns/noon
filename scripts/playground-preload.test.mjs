import assert from "node:assert/strict";
import test from "node:test";
import { runInNewContext } from "node:vm";

import { holdPlaygroundPreload } from "./playground-browser-support.mjs";

function renderHostEnvironment(t, value) {
  const previous = process.env.NOON_PLAYGROUND_RENDER_HOST;
  if (value === undefined) delete process.env.NOON_PLAYGROUND_RENDER_HOST;
  else process.env.NOON_PLAYGROUND_RENDER_HOST = value;
  t.after(() => {
    if (previous === undefined) delete process.env.NOON_PLAYGROUND_RENDER_HOST;
    else process.env.NOON_PLAYGROUND_RENDER_HOST = previous;
  });
}

function pageDouble(initialGlobals = {}) {
  const realm = { ...initialGlobals };
  const calls = [];
  const routes = [];
  const page = {
    async addInitScript(callback, argument) {
      calls.push(["init", argument]);
      runInNewContext(`(${callback.toString()})(${JSON.stringify(argument)})`, realm);
    },
    async route(pattern, handler) {
      calls.push(["route", pattern]);
      routes.push(handler);
    },
  };
  return { page, realm, calls, routes };
}

async function assertBootstrapGate(state, release) {
  assert.equal(state.routes.length, 1);
  assert.deepEqual(state.calls.at(-1), ["route", "**/live-authoring-bootstrap.js"]);
  let continued = 0;
  const route = { continue: async () => { continued++; } };
  const pending = [state.routes[0](route), state.routes[0](route)];
  await Promise.resolve();
  assert.equal(continued, 0, "bootstrap requests must remain held before release");
  release();
  release();
  await Promise.all(pending);
  assert.equal(continued, 2, "release continues every pending bootstrap request exactly once");
  await state.routes[0](route);
  assert.equal(continued, 3, "later bootstrap requests pass the released gate");
}

test("default preload leaves automatic host selection untouched", async t => {
  renderHostEnvironment(t, undefined);
  const state = pageDouble();
  const release = await holdPlaygroundPreload(state.page);
  assert.equal(Object.hasOwn(state.realm, "__NOON_RENDER_HOST__"), false);
  assert.equal(state.calls.length, 1, "automatic selection adds no init script");
  await assertBootstrapGate(state, release);
});

test("explicit auto overrides the environment without changing an existing page control", async t => {
  renderHostEnvironment(t, "main-thread");
  const state = pageDouble({ __NOON_RENDER_HOST__: "worker" });
  const release = await holdPlaygroundPreload(state.page, { renderHost: "auto" });
  assert.equal(state.realm.__NOON_RENDER_HOST__, "worker");
  assert.equal(state.calls.length, 1);
  await assertBootstrapGate(state, release);
});

for (const renderHost of ["worker", "main-thread"]) {
  test(`explicit ${renderHost} is installed before the unchanged bootstrap gate`, async () => {
    const state = pageDouble();
    const release = await holdPlaygroundPreload(state.page, { renderHost });
    assert.equal(state.realm.__NOON_RENDER_HOST__, renderHost);
    assert.deepEqual(state.calls[0], ["init", renderHost]);
    assert.equal(state.calls.length, 2);
    await assertBootstrapGate(state, release);
  });

  test(`the environment selects ${renderHost} when no option is supplied`, async t => {
    renderHostEnvironment(t, renderHost);
    const state = pageDouble();
    const release = await holdPlaygroundPreload(state.page);
    assert.equal(state.realm.__NOON_RENDER_HOST__, renderHost);
    assert.deepEqual(state.calls[0], ["init", renderHost]);
    release();
  });
}

test("invalid explicit hosts are rejected before any page setup", async () => {
  for (const renderHost of [null, "", "AUTO", "main", "webgl", true, 1]) {
    const state = pageDouble();
    await assert.rejects(holdPlaygroundPreload(state.page, { renderHost }), /unsupported Noon render host/);
    assert.deepEqual(state.calls, []);
  }
});

test("an invalid environment host is rejected before bootstrap", async t => {
  renderHostEnvironment(t, "webgpu");
  const state = pageDouble();
  await assert.rejects(holdPlaygroundPreload(state.page), /unsupported Noon render host/);
  assert.deepEqual(state.calls, []);
});

test("an init-script failure cannot install the bootstrap route", async () => {
  const failure = new Error("init script installation failed");
  const page = {
    addInitScript: async () => { throw failure; },
    route: async () => { assert.fail("bootstrap must wait for successful host setup"); },
  };
  await assert.rejects(holdPlaygroundPreload(page, { renderHost: "main-thread" }),
    error => error === failure);
});

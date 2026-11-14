import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { captureStalledRenderStacks } from './noon-site-worker-stack.mjs';

class FakeCDP extends EventEmitter {
  commands = [];
  async send(method, params = {}) {
    this.commands.push({ method, params });
    if (method === 'Target.getTargets') return { targetInfos: [
      { type: 'worker', targetId: 'render-1', url: 'http://localhost/_site/execution-render-worker.js' },
      { type: 'worker', targetId: 'python', url: 'http://localhost/_site/python-worker.js' },
      { type: 'page', targetId: 'page', url: 'http://localhost/' },
    ] };
    if (method === 'Target.attachToTarget') return { sessionId: params.targetId };
    if (method === 'Target.sendMessageToTarget') {
      const sent = JSON.parse(params.message);
      if (sent.method === 'Debugger.enable') queueMicrotask(() => this.emit('Target.receivedMessageFromTarget', {
        sessionId: params.sessionId, message: JSON.stringify({ method: 'Debugger.scriptParsed',
          params: { scriptId: '5', url: 'http://localhost/_site/authoring-render-controller.js' } }),
      }));
      if (sent.method === 'Debugger.pause' && !this.noPause) queueMicrotask(() => this.emit('Target.receivedMessageFromTarget', {
        sessionId: params.sessionId, message: JSON.stringify({ method: 'Debugger.paused',
          params: { callFrames: [{ functionName: 'frame', location: { scriptId: '5', lineNumber: 1102, columnNumber: 4 } }] } }),
      }));
      return {};
    }
    return {};
  }
  async detach() { this.detached = true; }
}

function fakePage(session) { return { context() { return { newCDPSession: async () => session }; } }; }

test('captures only renderer worker stacks and cleans up inspector on failure', async () => {
  const cdp = new FakeCDP();
  const stack = await captureStalledRenderStacks(fakePage(cdp), { timeoutMs: 100 });
  assert.deepEqual(stack, [{ url: 'http://localhost/_site/execution-render-worker.js', frames: [
    { functionName: 'frame', url: 'http://localhost/_site/authoring-render-controller.js', line: 1103, column: 5 },
  ] }]);
  assert.ok(cdp.commands.some(command => command.method === 'Target.sendMessageToTarget' &&
    JSON.parse(command.params.message).method === 'Debugger.resume'));
  assert.ok(cdp.commands.some(command => command.method === 'Target.detachFromTarget'));
  assert.equal(cdp.listenerCount('Target.receivedMessageFromTarget'), 0);
  assert.equal(cdp.detached, true);
});

test('uninterruptible worker is marked unknown without losing the original error', async () => {
  const cdp = new FakeCDP(); cdp.noPause = true;
  const stack = await captureStalledRenderStacks(fakePage(cdp), { timeoutMs: 20 });
  assert.equal(stack.length, 1);
  assert.match(stack[0].error, /CDP probe timeout/);
  assert.equal(cdp.listenerCount('Target.receivedMessageFromTarget'), 0);
  assert.equal(cdp.detached, true);
  assert.equal(cdp.commands.some(command => command.method === 'Target.sendMessageToTarget' &&
    JSON.parse(command.params.message).method === 'Debugger.resume'), false);
});

test('CDP unavailable returns diagnostic error rather than rejecting the original failure', async () => {
  const stack = await captureStalledRenderStacks({ context() { return {
    newCDPSession: async () => { throw new Error('not supported'); },
  }; } }, { timeoutMs: 20 });
  assert.match(stack[0].error, /not supported/);
});

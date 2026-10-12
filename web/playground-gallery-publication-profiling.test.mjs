import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const source = await readFile(new URL('../scripts/playground-gallery-runtime-smoke.mjs', import.meta.url), 'utf8');
const start = source.indexOf('await context.addInitScript(') + 'await context.addInitScript('.length;
const end = source.indexOf('}, { channel: AUTHORING_CHANNEL', start) + 1;
assert.ok(start > 0 && end > start, 'gallery worker observation hook must exist');
const hook = source.slice(start, end);

function harness(profilePublications) {
  class Worker {
    sent = [];
    listeners = [];
    addEventListener(type, callback) { assert.equal(type, 'message'); this.listeners.push(callback); }
    postMessage(message) { this.sent.push(message); }
    emit(data) {
      let stopped = false;
      const event = { data, stopImmediatePropagation() { stopped = true; } };
      for (const callback of this.listeners) {
        callback(event);
        if (stopped) break;
      }
    }
  }
  const window = { Worker, location: { href: 'http://localhost/web/index.html' } };
  vm.runInNewContext(`(${hook})(options)`, {
    window, URL, performance: { now: () => 10 },
    options: { channel: 'noon.authoring', protocolVersion: 1, profilePublications },
  });
  return window;
}

test('live math capture starts on the render lane before playback initialization', () => {
  const window = harness(true);
  const author = new window.Worker('python-worker.js');
  assert.equal(author.sent.length, 0, 'profiling must not query the Python owner');
  const renderer = new window.Worker('execution-render-worker.js?v=fixture');
  renderer.postMessage({ type: 'init' });
  assert.equal(renderer.sent[0].type, 'metrics');
  assert.equal(renderer.sent[0].requestId, Number.MAX_SAFE_INTEGER);
  assert.equal(renderer.sent[0].profilePublicationStages, true);
  assert.equal(renderer.sent[1].type, 'init');
});

test('initial diagnostic replies cannot settle an ordinary client request', () => {
  const window = harness(true);
  const renderer = new window.Worker('execution-render-worker.js');
  const replies = [];
  renderer.addEventListener('message', event => replies.push(event.data));
  renderer.emit({ channel: 'noon.render', type: 'metrics', requestId: Number.MAX_SAFE_INTEGER,
    metrics: { publicationStageProfiling: true } });
  assert.equal(replies.length, 0);
  renderer.emit({ channel: 'noon.render', type: 'metrics', requestId: 1, metrics: {} });
  assert.equal(replies.length, 1);
  renderer.emit({ channel: 'noon.authoring', type: 'result', protocolVersion: 1, resultJson: 'result' });
  assert.equal(replies.length, 2);
  assert.equal(window.__galleryAuthoringResults[0], 'result');
});

test('cases without live math profiling issue no startup diagnostic request', () => {
  const window = harness(false);
  const renderer = new window.Worker('execution-render-worker.js');
  assert.equal(renderer.sent.length, 0);
});

test('failed profiling initialization remains a test failure', () => {
  const window = harness(true);
  const renderer = new window.Worker('execution-render-worker.js');
  assert.throws(() => renderer.emit({ channel: 'noon.render', type: 'error',
    requestId: Number.MAX_SAFE_INTEGER, message: 'unavailable' }), /profiling did not start/);
});

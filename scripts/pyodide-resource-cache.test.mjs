import assert from 'node:assert/strict';
import test from 'node:test';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';

const source = 'import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";';
const url = 'https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.asm.wasm';
async function attach(cache) {
  let handler;
  await cache.install({ async route(pattern, callback) {
    assert.equal(pattern, 'https://cdn.jsdelivr.net/pyodide/v314.0.5/full/**');
    handler = callback;
  } });
  return handler;
}
function request(fetch, method = 'GET') {
  const result = {};
  return {
    result,
    request: () => ({ url: () => url, method: () => method }),
    fetch,
    async fulfill(value) { result.response = value; },
    async abort(reason) { result.aborted = reason; },
    async continue() { result.continued = true; },
  };
}
function response(body, status = 200) {
  return {
    status: () => status,
    headers: () => ({ 'content-type': 'application/wasm', 'content-encoding': 'gzip', 'content-length': '999', 'set-cookie': 'private=value', 'access-control-allow-origin': '*' }),
    async body() { return body; },
    async dispose() {},
  };
}

test('independent contexts share one in-flight fetch and identical resource bytes', async () => {
  const cache = createPyodideResourceCache(source);
  const first = await attach(cache), second = await attach(cache);
  let fetches = 0;
  const bytes = Buffer.from([0, 97, 115, 109]);
  const upstream = async () => { fetches++; return response(bytes); };
  const a = request(upstream), b = request(upstream);
  await Promise.all([first(a), second(b)]);
  assert.equal(fetches, 1);
  for (const route of [a, b]) {
    assert.deepEqual(route.result.response.body, bytes);
    assert.deepEqual(route.result.response.headers, { 'content-type': 'application/wasm', 'access-control-allow-origin': '*' });
  }
  assert.equal(cache.stats().hits, 1);
  assert.equal(cache.stats().retainedBytes, bytes.length);
});

test('failed fetches still fail their cases and do not poison later requests', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  const failed = request(async () => { throw new Error('network failure'); });
  await handler(failed);
  assert.equal(failed.result.aborted, 'failed');
  assert.equal(failed.result.response, undefined);
  const later = request(async () => response(Buffer.from('ok')));
  await handler(later);
  assert.equal(later.result.response.body.toString(), 'ok');
  assert.equal(cache.stats().upstreamRequests, 2);
  assert.equal(cache.stats().upstreamFailures, 1);
});

test('HTTP errors are delivered unchanged and not cached', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  const failed = request(async () => response(Buffer.from('unavailable'), 503));
  await handler(failed);
  assert.equal(failed.result.response.status, 503);
  const later = request(async () => response(Buffer.from('ok')));
  await handler(later);
  assert.equal(later.result.response.status, 200);
  assert.equal(cache.stats().upstreamRequests, 2);
});

test('retained bytes are bounded and non-GET requests bypass the cache', async () => {
  const cache = createPyodideResourceCache(source, 2);
  const handler = await attach(cache);
  for (let i = 0; i < 2; i++) await handler(request(async () => response(Buffer.from('large'))));
  assert.equal(cache.stats().retainedBytes, 0);
  assert.equal(cache.stats().upstreamRequests, 2);
  const post = request(() => { throw new Error('must not fetch'); }, 'POST');
  await handler(post);
  assert.equal(post.result.continued, true);
});

test('unpinned or different-origin runtimes cannot opt into resource reuse', () => {
  assert.throws(() => createPyodideResourceCache(source.replace('v314.0.5', 'latest')), /pin a Pyodide release/);
  assert.throws(() => createPyodideResourceCache(source.replace('cdn.jsdelivr.net', 'example.com')), /pin a Pyodide release/);
});

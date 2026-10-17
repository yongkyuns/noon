import assert from 'node:assert/strict';
import test from 'node:test';
import { createPyodideResourceCache } from './pyodide-resource-cache.mjs';
import { LATEX_ASSETS } from '../web/latex/assets.js';

const source = 'import { loadPyodide } from "https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.mjs";';
const url = 'https://cdn.jsdelivr.net/pyodide/v314.0.5/full/pyodide.asm.wasm';
const editorModules = [
  'https://esm.sh/codemirror@6.0.2',
  'https://esm.sh/@codemirror/lang-python@6.2.1/es2022/lang-python.mjs',
  'https://esm.sh/@lezer/python@1.1.18/es2022/python.mjs',
  'https://esm.sh/style-mod@4.1.2/es2022/style-mod.mjs',
];
async function attach(cache) {
  let handler;
  await cache.install({ async route(pattern, callback) {
    assert.equal(pattern(new URL(url)), true);
    assert.equal(pattern(new URL(LATEX_ASSETS.bundle)), true);
    assert.equal(pattern(new URL(LATEX_ASSETS.metrics)), true);
    for (const asset of editorModules) assert.equal(pattern(new URL(asset)), true);
    assert.equal(pattern(new URL('https://example.com/python-worker.js')), false);
    assert.equal(pattern(new URL(LATEX_ASSETS.bundle.replace('1.0.5.tgz', '1.0.6.tgz'))), false);
    for (const unpinned of [
      'https://esm.sh/codemirror',
      'https://esm.sh/@codemirror/lang-python@^6.2.1',
      'https://esm.sh/@codemirror/lang-python@6',
      'https://esm.sh/unrelated@6.2.1/module.mjs',
      'https://example.com/@codemirror/lang-python@6.2.1/module.mjs',
    ]) assert.equal(pattern(new URL(unpinned)), false);
    handler = callback;
  } });
  return handler;
}
function request(fetch, method = 'GET', requestUrl = url) {
  const result = {};
  return {
    result,
    request: () => ({ url: () => requestUrl, method: () => method }),
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
  assert.equal(cache.stats().fulfillFailures, 0);
  assert.deepEqual(cache.stats().upstreamFailureDetails, [{ url, error: 'network failure' }]);
});

test('upstream failure diagnostics are bounded snapshots and never retry a case', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  let fetches = 0;
  for (let index = 0; index < 7; index++) {
    const failed = request(async () => { fetches++; throw new Error(`failure-${index}`); });
    await handler(failed);
    assert.equal(failed.result.aborted, 'failed');
  }
  const stats = cache.stats();
  assert.equal(fetches, 7);
  assert.equal(stats.upstreamFailures, 7);
  assert.deepEqual(stats.upstreamFailureDetails.map(detail => detail.error),
    ['failure-2', 'failure-3', 'failure-4', 'failure-5', 'failure-6']);
  stats.upstreamFailureDetails[0].error = 'mutated';
  assert.equal(cache.stats().upstreamFailureDetails[0].error, 'failure-2');
});

test('response cleanup failure does not retain bytes or poison later cases', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  let fetches = 0;
  const failed = request(async () => {
    fetches++;
    const value = response(Buffer.from('bad'));
    value.dispose = async () => { throw new Error('response cleanup failed'); };
    return value;
  });
  await handler(failed);
  assert.equal(failed.result.aborted, 'failed');
  assert.equal(failed.result.response, undefined);
  assert.equal(cache.stats().retainedBytes, 0);
  assert.equal(cache.stats().upstreamFailures, 1);
  assert.deepEqual(cache.stats().upstreamFailureDetails, [{ url, error: 'response cleanup failed' }]);
  const later = request(async () => { fetches++; return response(Buffer.from('ok')); });
  await handler(later);
  assert.equal(fetches, 2, 'only a later independent case may fetch again');
  assert.equal(later.result.response.body.toString(), 'ok');
  assert.equal(cache.stats().hits, 0);
  assert.equal(cache.stats().retainedBytes, 2);
});

test('body failure remains reported when response cleanup also fails', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  let disposals = 0;
  const failed = request(async () => {
    const value = response(Buffer.from('unused'));
    value.body = async () => { throw new Error('body read failed'); };
    value.dispose = async () => { disposals++; throw new Error('secondary cleanup failure'); };
    return value;
  });
  await handler(failed);
  assert.equal(failed.result.aborted, 'failed');
  assert.equal(disposals, 1);
  assert.equal(cache.stats().retainedBytes, 0);
  assert.equal(cache.stats().upstreamFailures, 1);
  assert.deepEqual(cache.stats().upstreamFailureDetails, [{ url, error: 'body read failed' }]);
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

test('rejected fulfill is counted and aborted without reporting a response', async () => {
  const cache = createPyodideResourceCache(source);
  const handler = await attach(cache);
  const failed = request(async () => response(Buffer.from('ok')));
  failed.fulfill = async () => { throw new Error('WebKit route closed'); };

  await handler(failed);

  assert.equal(failed.result.aborted, 'failed');
  assert.equal(failed.result.response, undefined);
  assert.equal(cache.stats().fulfillFailures, 1);
  assert.deepEqual(cache.stats().fulfillFailureDetails, [
    { url, error: 'WebKit route closed' },
  ]);
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


test('pinned optional LaTeX assets share unchanged response bytes across contexts', async () => {
  const cache = createPyodideResourceCache(source);
  const first = await attach(cache), second = await attach(cache);
  let fetches = 0;
  for (const asset of [LATEX_ASSETS.bundle, LATEX_ASSETS.metrics]) {
    const bytes = Buffer.from(asset);
    const upstream = async () => { fetches++; return response(bytes); };
    const a = request(upstream, 'GET', asset), b = request(upstream, 'GET', asset);
    await Promise.all([first(a), second(b)]);
    assert.deepEqual(a.result.response.body, bytes);
    assert.deepEqual(b.result.response.body, bytes);
  }
  assert.equal(fetches, 2);
  assert.equal(cache.stats().hits, 2);
});

test('pinned editor modules reuse their original bytes across fresh gallery contexts', async () => {
  const cache = createPyodideResourceCache(source);
  const first = await attach(cache), second = await attach(cache);
  let fetches = 0;
  for (const asset of editorModules) {
    const bytes = Buffer.from(`// unchanged module: ${asset}`);
    const upstream = async () => { fetches++; return response(bytes); };
    const a = request(upstream, 'GET', asset), b = request(upstream, 'GET', asset);
    await Promise.all([first(a), second(b)]);
    assert.deepEqual(a.result.response.body, bytes);
    assert.deepEqual(b.result.response.body, bytes);
    assert.equal(b.result.response.headers['access-control-allow-origin'], '*');
    assert.equal(b.result.response.headers['set-cookie'], undefined);
  }
  assert.equal(fetches, 4);
  assert.equal(cache.stats().hits, 4);
});

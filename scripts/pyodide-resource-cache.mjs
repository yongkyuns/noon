// Job-local byte reuse for immutable Pyodide assets. Scene/worker sources and
// browser state remain isolated; this does not retry a failed gallery case.
export function createPyodideResourceCache(workerSource, maxBytes = 64 * 1024 * 1024) {
  const moduleUrl = workerSource.match(
    /from\s+["'](https:\/\/cdn\.jsdelivr\.net\/pyodide\/v\d+\.\d+\.\d+\/full\/pyodide\.mjs)["']/,
  )?.[1];
  if (!moduleUrl) throw new Error('Gallery cache requires the worker to pin a Pyodide release');
  const baseUrl = new URL('.', moduleUrl).href;
  const entries = new Map();
  const counts = { requests: 0, hits: 0, upstreamRequests: 0, upstreamFailures: 0, retainedBytes: 0 };

  async function read(route, url) {
    counts.upstreamRequests++;
    let response;
    try {
      response = await route.fetch();
      const body = await response.body();
      const headers = { ...response.headers() };
      // APIResponse exposes decoded bytes. Do not reuse compressed lengths or
      // propagate response cookies between otherwise independent contexts.
      delete headers['content-encoding'];
      delete headers['content-length'];
      delete headers['set-cookie'];
      const value = { status: response.status(), headers, body };
      if (value.status === 200 && counts.retainedBytes + body.length <= maxBytes) {
        counts.retainedBytes += body.length;
      } else {
        entries.delete(url);
      }
      return value;
    } catch (error) {
      counts.upstreamFailures++;
      entries.delete(url);
      throw error;
    } finally {
      await response?.dispose();
    }
  }

  return {
    async install(context) {
      await context.route(`${baseUrl}**`, async route => {
        if (route.request().method() !== 'GET') return route.continue();
        counts.requests++;
        const url = route.request().url();
        let value = entries.get(url);
        if (value) counts.hits++;
        else {
          value = read(route, url);
          entries.set(url, value);
        }
        try {
          await route.fulfill(await value);
        } catch {
          // Preserve the failing request. A subsequent case may make a fresh
          // request, but this case is neither retried nor reported as successful.
          await route.abort('failed').catch(() => {});
        }
      });
    },
    stats() { return { baseUrl, ...counts }; },
  };
}

import { LATEX_ASSETS } from "../web/latex/assets.js";

// Job-local byte reuse for pinned Python and optional LaTeX assets. Scene/worker sources and
// browser state remain isolated; this does not retry a failed gallery case.
export function createPyodideResourceCache(workerSource, maxBytes = 64 * 1024 * 1024) {
  const moduleUrl = workerSource.match(
    /from\s+["'](https:\/\/cdn\.jsdelivr\.net\/pyodide\/v\d+\.\d+\.\d+\/full\/pyodide\.mjs)["']/,
  )?.[1];
  if (!moduleUrl) throw new Error('Gallery cache requires the worker to pin a Pyodide release');
  const baseUrl = new URL('.', moduleUrl).href;
  const latexUrls = new Set([LATEX_ASSETS.bundle, LATEX_ASSETS.metrics]);
  const entries = new Map();
  const counts = {
    requests: 0,
    hits: 0,
    upstreamRequests: 0,
    upstreamFailures: 0,
    upstreamFailureDetails: [],
    fulfillFailures: 0,
    fulfillFailureDetails: [],
    retainedBytes: 0,
  };

  async function read(route, url) {
    counts.upstreamRequests++;
    let response, value, failure;
    let failed = false;
    try {
      response = await route.fetch();
      const body = await response.body();
      const headers = { ...response.headers() };
      // APIResponse exposes decoded bytes. Do not reuse compressed lengths or
      // propagate response cookies between otherwise independent contexts.
      delete headers['content-encoding'];
      delete headers['content-length'];
      delete headers['set-cookie'];
      value = { status: response.status(), headers, body };
    } catch (error) {
      failed = true;
      failure = error;
    }
    try {
      await response?.dispose();
    } catch (error) {
      // Keep the primary read failure when cleanup also fails.
      if (!failed) { failed = true; failure = error; }
    }
    if (failed) {
      counts.upstreamFailures++;
      counts.upstreamFailureDetails.push({
        url: url.slice(0, 500),
        error: String(failure?.message ?? failure).slice(0, 240),
      });
      if (counts.upstreamFailureDetails.length > 5) counts.upstreamFailureDetails.shift();
      entries.delete(url);
      throw failure;
    }
    if (value.status === 200 && counts.retainedBytes + value.body.length <= maxBytes) {
      counts.retainedBytes += value.body.length;
    } else {
      entries.delete(url);
    }
    return value;
  }

  return {
    async install(context) {
      await context.route(url => url.href.startsWith(baseUrl) || latexUrls.has(url.href), async route => {
        if (route.request().method() !== 'GET') return route.continue();
        counts.requests++;
        const url = route.request().url();
        let value = entries.get(url);
        if (value) counts.hits++;
        else {
          value = read(route, url);
          entries.set(url, value);
        }
        let responseValue;
        try {
          responseValue = await value;
        } catch {
          await route.abort('failed').catch(() => {});
          return;
        }
        try {
          await route.fulfill(responseValue);
        } catch (error) {
          counts.fulfillFailures++;
          counts.fulfillFailureDetails.push({
            url: url.slice(0, 500),
            error: String(error?.message ?? error).slice(0, 240),
          });
          if (counts.fulfillFailureDetails.length > 5) counts.fulfillFailureDetails.shift();
          // Preserve the failing request. A subsequent case may make a fresh
          // request, but this case is neither retried nor reported as successful.
          await route.abort('failed').catch(() => {});
        }
      });
    },
    stats() {
      return {
        baseUrl,
        ...counts,
        upstreamFailureDetails: counts.upstreamFailureDetails.map((detail) => ({ ...detail })),
        fulfillFailureDetails: counts.fulfillFailureDetails.map((detail) => ({ ...detail })),
      };
    },
  };
}

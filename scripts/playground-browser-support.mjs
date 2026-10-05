import { lstat, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";

// Shared software graphics configuration for the playground browser gates.
export function playgroundLaunchOptions(browserName) {
  if (browserName === "chromium") {
    return {
      headless: true,
      args: [
        "--disable-features=WebGPU",
        "--enable-unsafe-swiftshader",
        "--ignore-gpu-blocklist",
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--disable-gpu-sandbox",
        "--disable-dev-shm-usage",
      ],
    };
  }
  if (browserName === "firefox") {
    return {
      // Linux headless Firefox does not expose a usable WebGL surface on the
      // CI runner. Xvfb supplies a display while Mesa provides software rendering.
      headless: !(process.platform === "linux" && process.env.DISPLAY),
      firefoxUserPrefs: {
        "webgl.disabled": false,
        "webgl.force-enabled": true,
      },
    };
  }
  return { headless: true };
}

// Playwright's waitForFunction tests a returned Promise for truthiness before
// resolving it. Async worker observations need polling of the resolved value.
export async function waitForBrowserObservation(page, predicate, argument, { timeout = 90_000 } = {}) {
  const deadline = Date.now() + timeout;
  while (!await page.evaluate(predicate, argument)) {
    if (Date.now() >= deadline) throw new Error("Timed out waiting for browser observation");
    await page.waitForTimeout(25);
  }
}

// Import the unchanged, attested worker only after removing JSPI in its realm.
// A mobile viewport alone does not model an iPhone's interpreter capabilities.
export async function disableAuthoringJspi(context, { beforeImport } = {}) {
  await context.addInitScript(() => {
    window.Worker = new Proxy(window.Worker, {
      construct(target, args, newTarget) {
        const workerArgs = [...args];
        const workerUrl = new URL(workerArgs[0], window.location.href);
        if (workerUrl.pathname.endsWith("/python-worker.js")) {
          workerArgs[0] = new URL("./python-worker-no-jspi-test.js", workerUrl);
          window.__noonNoJspiWorkerWrapped = true;
        }
        return Reflect.construct(target, workerArgs, newTarget);
      },
    });
  });
  await context.route("**/python-worker-no-jspi-test.js", async (route) => {
    await beforeImport?.();
    await route.fulfill({
      status: 200,
      contentType: "text/javascript",
      body: [
        "delete WebAssembly.promising;",
        "delete WebAssembly.Suspending;",
        "if ('promising' in WebAssembly || 'Suspending' in WebAssembly) throw new Error('JSPI test precondition failed');",
        "await import('./python-worker.js');",
      ].join("\n"),
    });
  });
}

// CI failure evidence only. Known process IDs avoid guessing executable paths
// that macOS may anonymize. IPS consists of metadata on line one and a JSON
// report in the remaining text:
// https://developer.apple.com/documentation/xcode/interpreting-the-json-format-of-a-crash-report
export async function collectWebKitCrashReports({ directories, pids, startedAtMs,
  endedAtMs, artifacts, prefix }) {
  const reports = [];
  const errors = [];
  const owned = new Set(pids.filter(pid => Number.isSafeInteger(pid) && pid > 0));
  if (!Number.isFinite(startedAtMs) || !Number.isFinite(endedAtMs) || endedAtMs < startedAtMs) {
    return { reports, errors, reason: "invalid crash capture window" };
  }
  if (owned.size === 0) return { reports, errors, reason: "no observed WebKit process IDs" };
  for (const directory of directories) {
    let names;
    try { names = await readdir(directory); }
    catch (error) { errors.push({ directory, code: error.code }); continue; }
    const candidates = [];
    for (const name of names.filter(name => /^(?:com\.apple\.WebKit|Playwright).*\.ips$/.test(name)).sort().slice(-64)) {
      const file = path.join(directory, name);
      try {
        const info = await lstat(file);
        if (info.isFile() && info.size <= 2 * 1024 * 1024 && info.mtimeMs >= startedAtMs) {
          candidates.push({ file, modified: info.mtimeMs });
        }
      } catch { /* A report may still be written or retired by the OS. */ }
    }
    for (const { file } of candidates.sort((a, b) => b.modified - a.modified).slice(0, 32)) {
      if (reports.length >= 3) break;
      try {
        const bytes = await readFile(file);
        if (bytes.length > 2 * 1024 * 1024) continue;
        const text = bytes.toString("utf8");
        const newline = text.indexOf("\n");
        const metadata = JSON.parse(text.slice(0, newline));
        if (metadata.bug_type !== "309") continue;
        const report = JSON.parse(text.slice(newline + 1));
        const capturedAt = Date.parse(report.captureTime);
        if (!owned.has(report.pid) || !Number.isFinite(capturedAt) ||
            capturedAt < startedAtMs || capturedAt > endedAtMs) continue;
        const name = `${prefix}-crash-${reports.length + 1}.ips`;
        await writeFile(path.join(artifacts, name), bytes);
        reports.push({ pid: report.pid, capturedAt, file: name,
          exception: report.exception ?? null, termination: report.termination ?? null });
      } catch { /* Malformed/unavailable reports are not crash evidence. */ }
    }
  }
  return { reports, errors };
}

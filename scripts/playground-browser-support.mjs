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

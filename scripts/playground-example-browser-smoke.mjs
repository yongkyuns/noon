import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? "webkit";
const profileName = process.env.NOON_PLAYGROUND_PROFILE ?? "mobile-dpr2";
const port = Number(process.env.NOON_PLAYGROUND_EXAMPLES_PORT ?? "4176");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    `browser-smoke-artifacts/playground-matrix/${browserName}-${profileName}`,
);

const profiles = {
  "desktop-dpr1": { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 },
  "desktop-dpr2": { viewport: { width: 1100, height: 760 }, deviceScaleFactor: 2 },
  "mobile-dpr2": { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2 },
};

assert.ok(["chromium", "firefox", "webkit"].includes(browserName));
assert.ok(profileName in profiles);

const browserType = playwright[browserName];
const profile = profiles[profileName];
await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Examples smoke server did not start: ${lastError}\n${serverOutput}`);
}

function launchOptions() {
  if (browserName === "chromium") {
    return {
      headless: true,
      args: ["--disable-gpu-sandbox", "--disable-dev-shm-usage"],
    };
  }
  if (browserName === "firefox") return { headless: true };
  return { headless: true };
}

let browser = null;
try {
  await waitForServer();
  browser = await browserType.launch(launchOptions());
  const context = await browser.newContext({
    viewport: profile.viewport,
    deviceScaleFactor: profile.deviceScaleFactor,
  });
  const page = await context.newPage();

  const pageErrors = [];
  const consoleErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);

  const trigger = page.locator("#example-browser-trigger");
  await trigger.waitFor({ state: "visible" });
  await trigger.click();

  const layer = page.locator(".example-browser-layer");
  await layer.waitFor({ state: "visible" });
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );

  const presentation = await page.evaluate(() => {
    const layer = document.querySelector(".example-browser-layer");
    const gallery = document.querySelector(".example-gallery");
    const grid = document.querySelector(".gallery-grid");
    const cards = [...document.querySelectorAll(".example-card")];
    const first = cards[0] ?? null;
    const title = first?.querySelector(".example-card-title") ?? null;
    const rect = (element) => {
      const value = element?.getBoundingClientRect();
      return value
        ? { x: value.x, y: value.y, width: value.width, height: value.height }
        : null;
    };
    const firstRect = first?.getBoundingClientRect() ?? null;
    const hit = firstRect
      ? document.elementFromPoint(
          firstRect.left + Math.min(firstRect.width / 2, 24),
          firstRect.top + Math.min(firstRect.height / 2, 24),
        )
      : null;
    const firstStyle = first ? getComputedStyle(first) : null;
    const gridStyle = grid ? getComputedStyle(grid) : null;
    return {
      viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
      layerHidden: layer?.hidden ?? null,
      layerRect: rect(layer),
      galleryRect: rect(gallery),
      gridRect: rect(grid),
      gridClientHeight: grid?.clientHeight ?? null,
      gridScrollHeight: grid?.scrollHeight ?? null,
      gridTemplateColumns: gridStyle?.gridTemplateColumns ?? null,
      cardCount: cards.length,
      firstCardRect: rect(first),
      firstCardTitle: title?.textContent?.trim() ?? "",
      firstCardDisplay: firstStyle?.display ?? null,
      firstCardVisibility: firstStyle?.visibility ?? null,
      firstCardOpacity: firstStyle?.opacity ?? null,
      firstCardContentVisibility: firstStyle?.contentVisibility ?? null,
      firstCardHitTested: hit?.closest?.(".example-card") === first,
      bodyOverflow: getComputedStyle(document.body).overflow,
      pageErrors: [],
    };
  });
  presentation.pageErrors = pageErrors;
  presentation.consoleErrors = consoleErrors;
  presentation.browserName = browserName;
  presentation.browserVersion = browser.version();
  presentation.profileName = profileName;

  await page.screenshot({
    path: path.join(artifactDir, "examples-browser-open.png"),
    fullPage: false,
  });
  await writeFile(
    path.join(artifactDir, "examples-browser-open.json"),
    `${JSON.stringify(presentation, null, 2)}\n`,
    "utf8",
  );

  assert.equal(presentation.layerHidden, false, `${browserName}/${profileName}: layer stayed hidden`);
  assert.ok(presentation.cardCount >= 2, `${browserName}/${profileName}: example cards are missing`);
  assert.ok(
    presentation.gridRect?.height > 100,
    `${browserName}/${profileName}: examples grid collapsed (${presentation.gridRect?.height}px)`,
  );
  assert.ok(
    presentation.firstCardRect?.width > 80 && presentation.firstCardRect?.height > 60,
    `${browserName}/${profileName}: first example card has no usable geometry`,
  );
  assert.ok(
    presentation.firstCardRect.y < presentation.viewport.height &&
      presentation.firstCardRect.y + presentation.firstCardRect.height > 0,
    `${browserName}/${profileName}: first example card is outside the viewport`,
  );
  assert.ok(presentation.firstCardTitle.length > 0, `${browserName}/${profileName}: first card title is blank`);
  assert.equal(
    presentation.firstCardHitTested,
    true,
    `${browserName}/${profileName}: first card is not hit-testable after opening`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}

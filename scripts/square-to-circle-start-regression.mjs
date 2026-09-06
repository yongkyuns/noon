import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_SQUARE_TO_CIRCLE_PORT ?? "4197");
const baseUrl = `http://127.0.0.1:${port}`;

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`SquareToCircle regression server did not start: ${lastError}\n${serverOutput}`);
}

function visibleObject(frame, label) {
  const visible = frame.objects.filter((object) => object.present && object.bounds !== null);
  assert.equal(visible.length, 1, `${label}: expected exactly one visible geometry object`);
  return visible[0];
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  const source = await readFile(
    path.join(repoRoot, "web/python/examples/manim_parity_square_to_circle.py"),
    "utf8",
  );
  const result = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    source,
  );
  assert.ok(result?.document, "SquareToCircle authoring must return executable geometry");
  assert.equal(result.duration, 3, "SquareToCircle duration drifted");

  const [createFrame, circleFrame] = await page.evaluate(
    ({ sceneJson, createTime, circleTime }) => [
      window.noonManimCompat.semanticFrame(sceneJson, createTime),
      window.noonManimCompat.semanticFrame(sceneJson, circleTime),
    ],
    {
      sceneJson: JSON.stringify(result.document),
      createTime: 0.5,
      circleTime: 2.0,
    },
  );

  const createObject = visibleObject(createFrame, "SquareToCircle create phase");
  const circleObject = visibleObject(circleFrame, "SquareToCircle transform endpoint");
  const createWidth = Number(createObject.bounds.width);
  const createHeight = Number(createObject.bounds.height);
  const circleWidth = Number(circleObject.bounds.width);
  const circleHeight = Number(circleObject.bounds.height);

  assert.ok(Number.isFinite(createWidth) && Number.isFinite(circleWidth));
  assert.ok(Number.isFinite(createHeight) && Number.isFinite(circleHeight));
  assert.ok(
    createWidth > circleWidth * 1.3 && createHeight > circleHeight * 1.3,
    `SquareToCircle must start from the rotated square before morphing to the circle; ` +
      `create bounds=${createWidth.toFixed(4)}×${createHeight.toFixed(4)}, ` +
      `circle bounds=${circleWidth.toFixed(4)}×${circleHeight.toFixed(4)}`,
  );
  assert.deepEqual(errors, [], `SquareToCircle emitted browser errors:\n${errors.join("\n")}`);

  console.log(
    `✓ SquareToCircle starts from rotated square: ` +
      `${createWidth.toFixed(3)}×${createHeight.toFixed(3)} -> ` +
      `${circleWidth.toFixed(3)}×${circleHeight.toFixed(3)}`,
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}

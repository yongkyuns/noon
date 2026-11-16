import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const root = fileURLToPath(new URL("../../../", import.meta.url));
const artifactRoot = path.resolve(process.env.NOON_LIVE_FEEDBACK_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/live-feedback"));
await mkdir(artifactRoot, { recursive: true });
const output = await mkdtemp(path.join(artifactRoot, "run-"));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const source = "# Synthetic host fixture; no Noon scene was evaluated.\nfrom noon import *\n";
const fixture = `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<style>body{background:#07090f;color:#f7f8fb;font:16px system-ui;margin:24px}canvas{max-width:100%;background:#111827}.preview-head{display:flex;gap:24px;margin-bottom:18px}textarea{width:100%}</style>
<section class="preview-pane"><div class="preview-head"><strong>Noon · Review host test</strong></div><canvas id="scene" width="800" height="450"></canvas></section>
<textarea id="editor">This editor draft was never rendered.</textarea>
<script type="module">
import { installPlaygroundFeedback } from '/web/playground-feedback.js';
const canvas=document.querySelector('canvas'), ctx=canvas.getContext('2d');
ctx.fillStyle='#101521';ctx.fillRect(0,0,800,450);ctx.font='30px system-ui';ctx.fillStyle='#f7f8fb';ctx.fillText('Two inputs, one sum',45,60);
ctx.fillStyle='#81a9ff';ctx.fillRect(90,130,220,45);ctx.fillText('+2',90,220);
ctx.fillStyle='#e8b06e';ctx.fillRect(470,130,220,45);ctx.fillText('−2',470,220);
ctx.fillStyle='#b7a9ff';ctx.fillText('2 + (−2) = 0',295,325);
ctx.font='16px system-ui';ctx.fillStyle='#97a6bd';ctx.fillText('Synthetic host fixture — not a Noon render',45,411);
const probe=window.probe={playing:true,current:true,pauses:0,seeks:0,releases:0,sceneClicks:0,runs:0,delayPause:false};
const state=()=>({preparedPlayback:true,playing:probe.playing,time:1.2345678901234567,presentationReceipt:{session:7,sequence:11,time:1.2345678901234567}});
canvas.addEventListener('click',()=>probe.sceneClicks++);
document.addEventListener('keydown',e=>{if(e.ctrlKey && e.key==='Enter')probe.runs++});
const player={rendererBackend:'synthetic-host-fixture',async pause(){probe.pauses++;if(probe.delayPause)await new Promise(resolve=>probe.releasePause=resolve);probe.playing=false;return state()},async seek(){probe.seeks++;return state()},async state(){return state()}};
window.feedback=installPlaygroundFeedback(document.querySelector('.preview-pane'),{getContext(){return {player,canvas,source:${JSON.stringify(source)},runGeneration:3,isCurrent:()=>probe.current,buildIdentity:async()=>({sourceRevision:'a'.repeat(40),files:{wasm:{sha256:'b'.repeat(64)},worker:{sha256:'c'.repeat(64)}},buildId:'synthetic-observed-build'}),release(){probe.releases++}}}});
</script>`;
const server = createServer(async (request, response) => {
  try {
    if (request.url === "/") { response.setHeader("Content-Type", "text/html"); response.end(fixture); return; }
    if (["/web/playground-feedback.js", "/web/playground-feedback-capture.js"].includes(request.url)) {
      response.setHeader("Content-Type", "text/javascript");
      response.end(await readFile(path.join(root, request.url))); return;
    }
    response.writeHead(404); response.end();
  } catch { response.writeHead(500); response.end(); }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const url = `http://127.0.0.1:${server.address().port}`;
let browser;
const report = { fixtureOnly: true, sceneExecuted: false, actualNoonPlaybackQualified: false, audioQualified: false };
try {
  browser = await chromium.launch({ headless: true,
    ...(process.env.NOON_BROWSER_EXECUTABLE ? { executablePath: process.env.NOON_BROWSER_EXECUTABLE } : {}),
  });
  report.browserVersion = browser.version();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, acceptDownloads: true });
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  await page.goto(url);
  await page.locator("#preview-comment").click();
  await page.locator(".noon-feedback-status").filter({ hasText: "Paused snapshot" }).waitFor();
  assert.equal(await page.evaluate(() => probe.playing), false);
  assert.equal(await page.evaluate(() => probe.pauses), 1);
  assert.equal(await page.evaluate(() => probe.seeks), 1);
  const bounds = await page.locator(".noon-feedback-image img").boundingBox();
  // The landscape image is letterboxed within the dialog's fixed-height image.
  const scale = Math.min(bounds.width / 800, bounds.height / 450);
  const imageWidth = 800 * scale, imageHeight = 450 * scale;
  const left = bounds.x + (bounds.width - imageWidth) / 2;
  const top = bounds.y + (bounds.height - imageHeight) / 2;
  await page.mouse.click(left + imageWidth * 0.28, top + imageHeight * 0.35);
  assert.equal(await page.locator("#noon-feedback-comment").evaluate(el => document.activeElement === el), true);
  await page.locator("#noon-feedback-comment").fill("Keep this input visible while showing the sum.");
  assert.equal(await page.locator('[type="submit"]').isDisabled(), false);
  assert.equal(await page.evaluate(() => probe.sceneClicks), 0, "review clicks must not reach the scene");
  await page.keyboard.press("Control+Enter");
  assert.equal(await page.evaluate(() => probe.runs), 0, "comment shortcuts must not invoke Run");
  await page.screenshot({ path: path.join(output, "desktop.png"), fullPage: true });
  const downloadEvent = page.waitForEvent("download");
  await page.locator('[type="submit"]').click();
  const download = await downloadEvent;
  const exportedPath = path.join(output, "feedback.json");
  await download.saveAs(exportedPath);
  const envelope = JSON.parse(await readFile(exportedPath, "utf8"));
  assert.equal(envelope.source, source);
  assert.equal(envelope.feedback.frame.provenance.sourceSha256, hash(source));
  assert.equal(envelope.feedback.status, "pending_review");
  assert.equal(envelope.feedback.feedback.observation, "Keep this input visible while showing the sum.");
  assert.equal(envelope.feedback.frame.provenance.publishedTime, 1.2345678901234567);
  assert.deepEqual(envelope.feedback.feedback.interval, { start: 1.2345678901234567, end: 1.2345678901234567 });
  assert.ok(Math.abs(envelope.feedback.feedback.point.x - 0.28) < 0.005);
  assert.ok(Math.abs(envelope.feedback.feedback.point.y - 0.35) < 0.005);
  const png = Buffer.from(envelope.pngBase64, "base64");
  assert.equal(hash(png), envelope.feedback.frame.sha256);
  assert.equal(png.length, envelope.feedback.frame.byteLength);
  assert.equal(png.readUInt32BE(16), 800); assert.equal(png.readUInt32BE(20), 450);
  await writeFile(path.join(output, "frame.png"), png);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator(".noon-feedback-image").scrollIntoViewIfNeeded();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false);
  const pin = await page.locator(".noon-feedback-pin").boundingBox();
  assert.ok(pin && pin.width > 0);
  await page.screenshot({ path: path.join(output, "mobile.png"), fullPage: true });
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog").isVisible(), false);
  assert.equal(await page.evaluate(() => probe.playing), false, "closing must not resume playback");
  assert.equal(await page.locator(".noon-feedback-image img").getAttribute("src"), null);

  // A cancelled asynchronous capture never queues a seek after its pause returns.
  await page.evaluate(() => { probe.delayPause = true; });
  const seeksBefore = await page.evaluate(() => probe.seeks);
  await page.locator("#preview-comment").click();
  await page.waitForFunction(() => typeof probe.releasePause === "function");
  await page.keyboard.press("Escape");
  await page.evaluate(() => { probe.releasePause(); probe.delayPause = false; });
  await page.waitForFunction(() => probe.releases === 2);
  assert.equal(await page.evaluate(() => probe.seeks), seeksBefore);

  // Rerun/owner changes are surfaced, without a stale picture or enabled export.
  await page.evaluate(() => { probe.current = false; });
  await page.locator("#preview-comment").click();
  await page.locator('.noon-feedback-status[data-error="true"]').waitFor();
  assert.equal(await page.locator('[type="submit"]').isDisabled(), true);
  await page.keyboard.press("Escape");
  await page.evaluate(() => { probe.current = true; });
  await page.locator("#preview-comment").click();
  await page.locator(".noon-feedback-status").filter({ hasText: "Paused snapshot" }).waitFor();
  await page.locator(".noon-feedback-image").focus(); await page.keyboard.press("Enter");
  assert.equal(await page.locator("#noon-feedback-comment").evaluate(el => document.activeElement === el), true);
  await page.locator("#noon-feedback-comment").fill("é".repeat(2001));
  assert.equal(await page.locator('[type="submit"]').isDisabled(), true);
  await page.evaluate(() => feedback.dispose());
  assert.equal(await page.locator("dialog").count(), 0);
  assert.equal(await page.locator("#preview-comment").count(), 0);
  assert.deepEqual(errors, []);
  Object.assign(report, { ok: true, exportedPath, checks: [
    "pause-and-receipt", "click-pin", "focus-comment", "single-comment-export", "source-snapshot",
    "exact-time", "PNG-hash", "pointer-isolation", "keyboard-isolation", "narrow-layout",
    "escape-cleanup", "cancel-before-seek", "stale-run-rejection", "keyboard-pin", "UTF8-limit", "dispose",
  ] });
} catch (error) {
  Object.assign(report, { ok: false, error: String(error?.stack ?? error) });
  process.exitCode = 1;
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
  await writeFile(path.join(output, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ output, ...report }));
}

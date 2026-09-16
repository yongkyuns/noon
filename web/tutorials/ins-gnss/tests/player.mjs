// End-to-end controls and dense motion samples through the real Noon renderer.
import assert from 'node:assert/strict';
import { chromium } from 'playwright';
import { PNG } from 'pngjs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { serveRepository } from '../../../../scripts/browser-test-server.mjs';
import { browserArgs } from '../../../../scripts/manim-raster-support.mjs';

const backend = process.env.NOON_TUTORIAL_BACKEND ?? 'webgpu';
const base = 'web/tutorials/ins-gnss';
const output = `browser-smoke-artifacts/ins-gnss/player-${backend}`;
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const server = await serveRepository(process.cwd(), 4198);
const browser = await chromium.launch({headless: true, args: browserArgs(backend)});
const page = await browser.newPage({viewport: {width: 1360, height: 1000}});
const errors = [], motion = [];
let paused, metrics, completion;
await mkdir(path.join(output, 'motion'), {recursive: true});
page.on('pageerror', error => errors.push(String(error)));
page.on('console', message => {if (message.type() === 'error') errors.push(message.text());});
const ready = () => page.waitForFunction(() => window.tutorial && !window.tutorial.status().busy, null, {timeout: 90000});
const sample = time => page.evaluate(t => window.tutorial.sample(t), time);
const state = () => page.evaluate(() => window.tutorial.status());
const screenshot = name => page.locator('#scene').screenshot({path: path.join(output, name)});

function cursorColumn(bytes) {
  const {width, height, data} = PNG.sync.read(bytes);
  let total = 0, weighted = 0;
  // Plot interior only. Notes/legend use the same color outside this region.
  for (let x = Math.floor(width * .085); x < width * .525; x++) {
    let count = 0;
    for (let y = Math.floor(height * .29); y < height * .69; y++) {
      const i = 4 * (y * width + x);
      const [r, g, b] = data.subarray(i, i + 3);
      if (r > 180 && g > 130 && g < 230 && b < 135 && g > 2 * b) count++;
    }
    if (count > height * .2) {total += count; weighted += x * count;}
  }
  assert.ok(total > 0, 'Gold cursor must be visible in the plot');
  return weighted / total;
}

try {
  await page.goto(`${server.baseUrl}/${base}/`);
  await ready();
  assert.deepEqual((await state()).errors, []);
  await sample(7);
  await page.locator('#play').click();
  await page.waitForTimeout(2200);
  await page.locator('#play').click();
  await ready();
  paused = await state();
  assert.ok(paused.time > 7.5, 'Play must advance the authored timeline');
  assert.equal(paused.playing, false);
  const before = await screenshot('paused.png');
  await page.waitForTimeout(350);
  assert.equal(hash(await page.locator('#scene').screenshot()), hash(before), 'Pause must freeze raster output');
  assert.equal((await state()).time, paused.time, 'Paused lesson time must not drift');
  metrics = await page.evaluate(() => window.tutorial.metrics());

  await page.locator('#restart').click();
  await ready();
  assert.equal((await state()).time, 0);
  await sample(7);
  const first = await screenshot('replay-a.png');
  await page.evaluate(() => window.tutorial.open(1));
  await ready();
  await sample(7);
  assert.equal(hash(await screenshot('replay-b.png')), hash(first), 'Fresh replay must reproduce the same frame');

  for (let i = 0; i <= 90; i++) {
    const time = 7 + i / 30;
    const start = performance.now();
    await sample(time);
    const bytes = await screenshot(`motion/${String(i).padStart(3, '0')}.png`);
    motion.push({time, cursorX: cursorColumn(bytes), sampleAndCaptureMs: performance.now() - start});
  }
  const steps = motion.slice(1).map((p, i) => p.cursorX - motion[i].cursorX);
  assert.ok(steps.every(dx => dx >= -.1), 'Cursor must not reverse during linear time');
  assert.ok(Math.max(...steps) < 4, 'No spatial jumps in dense cursor samples');
  assert.ok(motion.at(-1).cursorX - motion[0].cursorX > 80, 'Cursor must traverse the expected distance');

  await sample(100);
  completion = await state();
  assert.equal(completion.completed, true);
  assert.ok(Math.abs(completion.time - 46.75) < 1e-5, 'Stop at exact chapter completion');
  await page.locator('#chapter').selectOption('6');
  await ready();
  assert.equal((await state()).chapter, 6);
  await sample(33.75);
  await page.screenshot({path: path.join(output, 'desktop.png'), fullPage: true});
  await page.setViewportSize({width: 390, height: 844});
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'No mobile horizontal overflow');
  await page.screenshot({path: path.join(output, 'mobile.png'), fullPage: true});
  assert.deepEqual((await state()).errors, []);
} catch (error) {
  errors.push(String(error));
  await page.screenshot({path: path.join(output, 'failure.png'), fullPage: true}).catch(() => {});
} finally {
  await writeFile(path.join(output, 'report.json'), JSON.stringify({backend, browser: browser.version(),
    sourceSha256: hash(await readFile(path.join(base, 'scene.py'))),
    playerSha256: hash(await readFile(path.join(base, 'player.js'))), paused, metrics, completion, motion, errors}, null, 2));
  await browser.close();
  await server.close();
}
assert.deepEqual(errors, []);

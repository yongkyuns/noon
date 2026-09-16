// Trusted tutorial browser checks. Uses the existing shared Noon preview session.
import { chromium } from 'playwright';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { serveRepository } from '../../../../scripts/browser-test-server.mjs';
import { browserArgs } from '../../../../scripts/manim-raster-support.mjs';

const root = process.cwd();
const sourcePath = process.env.NOON_TUTORIAL_SOURCE ?? 'web/tutorials/ins-gnss/tests/capability_probe.py';
const output = process.env.NOON_TUTORIAL_OUTPUT ?? 'browser-smoke-artifacts/ins-gnss/probe';
const times = (process.env.NOON_TUTORIAL_TIMES ?? '0,0.5,1,2,3,4,4.5,5.5').split(',').map(Number);
const source = await readFile(path.join(root, sourcePath), 'utf8');
const server = await serveRepository(root, 4199);
let browser;
const errors = [];
const frames = [];
try {
  await mkdir(output, {recursive:true});
  browser = await chromium.launch({headless:true, args:browserArgs(process.env.NOON_TUTORIAL_BACKEND ?? 'webgpu')});
  const page = await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:1});
  page.on('pageerror', e => errors.push(String(e)));
  page.on('console', m => { if(m.type()==='error') errors.push(m.text()); });
  await page.goto(`${server.baseUrl}/web/agent-preview-host.html`);
  await page.waitForFunction(() => window.noonAgentPreviewHost !== undefined);
  await page.locator('#scene').evaluate(c => {c.width=1280;c.height=720;c.style.width='1280px';c.style.height='720px';});
  const initial = await page.evaluate(s => window.noonAgentPreviewHost.open(s,600),source);
  for(const time of times){
    const state = time === 0 ? initial : await page.evaluate(t => window.noonAgentPreviewHost.sample(t),time);
    await page.locator('#scene').screenshot({path:path.join(output,`${time.toFixed(3)}.png`)});
    frames.push(state);
    console.log(JSON.stringify({time,frame:state.frame}));
    if(Math.abs(state.frame.publishedTime-time)>1e-6) throw new Error('Published time differs from requested time');
  }
  await page.evaluate(() => window.noonAgentPreviewHost.close());
} catch(error){errors.push(String(error));}
finally {
  await writeFile(path.join(output,'report.json'),JSON.stringify({sourcePath,sourceSha256:createHash('sha256').update(source).digest('hex'),browser:browser?.version(),frames,errors},null,2));
  await browser?.close();await server.close();
}
if(errors.length) throw new Error(errors.join('\n'));

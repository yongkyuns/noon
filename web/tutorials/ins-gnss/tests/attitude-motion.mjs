// Dense raster checks of the actual Noon rotation, not a replacement renderer.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { chromium } from 'playwright';
import { PNG } from 'pngjs';
import { serveRepository } from '../../../../scripts/browser-test-server.mjs';
import { browserArgs } from '../../../../scripts/manim-raster-support.mjs';

const base = 'web/tutorials/ins-gnss';
const source = await readFile(path.join(base, 'scene.py'), 'utf8');
const metadata = JSON.parse(await readFile(path.join(base, 'chapters.json'), 'utf8'));
const spec = metadata.chapters[7].rotation_review;
const config = JSON.parse(execFileSync('python3', ['-c',
  `import sys,json;sys.path.insert(0,'${base}/src');from attitude import AttitudeExample;from dataclasses import asdict;print(json.dumps(asdict(AttitudeExample())))`], {encoding:'utf8'}));
const backend = process.env.NOON_TUTORIAL_BACKEND ?? 'webgpu';
const output = `browser-smoke-artifacts/ins-gnss/attitude-${backend}`;
await mkdir(output, {recursive:true});
const server = await serveRepository(process.cwd(), 4197);
const browser = await chromium.launch({headless:true, args:browserArgs(backend)});
const page = await browser.newPage({viewport:{width:1280,height:720},deviceScaleFactor:1});
const errors = [], samples = [];
page.on('pageerror', e => errors.push(String(e)));
page.on('console', m => {if(m.type()==='error') errors.push(m.text());});

function measuredPitch(bytes) {
  const {width,height,data} = PNG.sync.read(bytes);
  const scale = height/8;
  const cx = width/2 + spec.origin[0]*scale, cy = height/2 - spec.origin[1]*scale;
  let xy=0, xx=0, pixels=0;
  // Interior of the red sensor-forward ray. Exclude the arrowhead and other axes.
  for(let x=Math.ceil(cx+.40*scale);x<cx+1.22*scale;x++) {
    for(let y=Math.floor(cy-.8*scale);y<cy;y++) {
      const i=4*(y*width+x), [r,g,b]=data.subarray(i,i+3);
      if(r>190 && g<150 && b<150 && r>1.5*g) {
        const dx=x+.5-cx, dy=cy-y-.5;
        xy+=dx*dy;xx+=dx*dx;pixels++;
      }
    }
  }
  assert.ok(pixels>60, 'Estimated frame must remain visibly anchored in its panel');
  return Math.atan2(xy,xx)*180/Math.PI;
}

try {
  await page.goto(`${server.baseUrl}/web/agent-preview-host.html`);
  await page.waitForFunction(()=>window.noonAgentPreviewHost!==undefined);
  await page.locator('#scene').evaluate(c=>{c.width=1280;c.height=720;c.style.width='1280px';c.style.height='720px';});
  await page.evaluate(s=>window.noonAgentPreviewHost.open(s,600), `context={'chapter':8}\n${source}`);
  const count=30;
  for(let i=0;i<=count;i++) {
    const fraction=i/count, time=spec.start+fraction*spec.duration;
    const state=await page.evaluate(t=>window.noonAgentPreviewHost.sample(t), time);
    assert.ok(Math.abs(state.frame.publishedTime-time)<1e-6);
    const bytes=await page.locator('#scene').screenshot({path:path.join(output,`${String(i).padStart(2,'0')}.png`)});
    const actual=measuredPitch(bytes);
    const expected=config.prior_pitch_deg+fraction*config.correction_pitch_deg;
    assert.ok(Math.abs(actual-expected)<.35, `Pitch ${actual} versus ${expected} at ${time}`);
    samples.push({time,actualPitchDeg:actual,expectedPitchDeg:expected});
  }
  for(let i=1;i<samples.length;i++) {
    const step=samples[i].actualPitchDeg-samples[i-1].actualPitchDeg;
    assert.ok(step<.05 && step>-.5, 'No reversed or discontinuous rotation');
  }
} catch(error) { errors.push(String(error)); }
finally {
  await page.evaluate(()=>window.noonAgentPreviewHost?.close()).catch(()=>{});
  await writeFile(path.join(output,'report.json'),JSON.stringify({backend,browser:browser.version(),
    sourceSha256:createHash('sha256').update(source).digest('hex'),samples,errors},null,2));
  await browser.close();await server.close();
}
assert.deepEqual(errors,[]);

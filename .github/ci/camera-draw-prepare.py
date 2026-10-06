"""Temporary #1653 experiment. Preserve production bytes; disclose draw ablations."""
import hashlib, json, pathlib, subprocess, sys
root, site, evidence, tools = map(pathlib.Path, sys.argv[1:5])
mode = sys.argv[5]
assert mode in {'normal', 'skip-line', 'skip-path', 'skip-text', 'skip-present', 'clip-all'}
evidence.mkdir(parents=True, exist_ok=True)
def sha(data): return hashlib.sha256(data).hexdigest()
def replace(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new)
identity = json.loads((site/'web/runtime-build-identity.json').read_text())
assert identity['sourceRevision'] == '886ede3e2243d621cda5823b30a7911f4e382417'
for item in identity['files'].values():
    assert sha((site/'web'/item['path']).read_bytes()) == item['sha256']
overrides=[]
for name in ['authoring-render-controller.js', 'authoring-render-worker.js']:
    old=(site/'web'/name).read_text()
    assert (root/'web'/name).read_bytes() == old.encode(), name
    if name == 'authoring-render-worker.js':
        # The probe module is evaluated before controller GPU creation on init.
        new='import "./camera-draw-install.js";\n'+old
    else:
        new=replace(old, 'function currentMetrics() {\n    const base = {',
            'function currentMetrics(includeDrawPrograms = false) {\n    const base = {\n      cameraDrawProbe: globalThis.__noonCameraDrawProbe?.snapshot(includeDrawPrograms),' )
    if name == 'authoring-render-controller.js':
        new=replace(new, 'respond(message.requestId, { type: "metrics", metrics: currentMetrics() });',
            'respond(message.requestId, { type: "metrics", metrics: currentMetrics(message.requestId === Number.MAX_SAFE_INTEGER) });')
    (site/'web'/name).write_text(new)
    overrides.append({'path':name,'originalSha256':sha(old.encode()),'diagnosticSha256':sha(new.encode())})
(site/'web/camera-draw-probe.js').write_bytes((tools/'camera-draw-probe.js').read_bytes())
(site/'web/camera-draw-install.js').write_text('import {installDrawProbe} from "./camera-draw-probe.js";\n'+
    'globalThis.__noonCameraDrawProbe=installDrawProbe(globalThis,'+json.dumps(mode)+');\n')
for name in ['camera-draw-probe.js','camera-draw-install.js']:
    overrides.append({'path':name,'originalSha256':None,'diagnosticSha256':sha((site/'web'/name).read_bytes())})
for item in overrides:
    subprocess.run(['node','--check',str(site/'web'/item['path'])],check=True)
(evidence/'overlay.json').write_text(json.dumps({'diagnosticOnly':True,'mode':mode,'runtimeIdentity':identity,'overrides':overrides},indent=2))
h=(root/'scripts/playground-product-e2e.mjs').read_text()
h=replace(h,'const probe = { worker: null, pending: false, latest: null, metricsReplies: 0,',
    'const probe = { drawPrograms: {}, drawLatest: null, worker: null, pending: false, latest: null, metricsReplies: 0,')
h=replace(h,'frames: metrics?.presentedFrames,','drawCounters: metrics?.cameraDrawProbe?.counters ?? null,\n        frames: metrics?.presentedFrames,')
h=replace(h,'probe.metricsReplies += 1;', '''probe.metricsReplies += 1;
          const draw = message.metrics?.cameraDrawProbe;
          if (draw) {
            for (const program of draw.newPrograms) probe.drawPrograms[program.id] = program;
            probe.drawLatest = draw;
          }''')
h=replace(h,'const cold = await runAndMeasure(page);', '''const cdp = await browser.newBrowserCDPSession();
  await writeFile(path.join(artifactDir, 'gpu-info.json'), JSON.stringify(await cdp.send('SystemInfo.getInfo'), null, 2));
  const cold = await runAndMeasure(page);''')
h=replace(h,'assert.ok(visual.changedPixels > 100, `product frame is effectively blank (${visual.changedPixels} changed pixels)`);',
    '''// Blank endpoints are intentional only for these rendering ablations.
  if (!['clip-all','skip-present'].includes(process.env.NOON_CAMERA_DRAW_MODE))
    assert.ok(visual.changedPixels > 100, `diagnostic frame is unexpectedly blank (${visual.changedPixels})`);''')
h=replace(h,'const report = {', '''const drawProbe = await page.evaluate(() => ({
    programs: Object.values(window.__noonProductRenderProbe.drawPrograms),
    latest: window.__noonProductRenderProbe.drawLatest,
  }));
  await writeFile(path.join(artifactDir, 'draw-programs.json'), JSON.stringify(drawProbe, null, 2));
  assert.ok(drawProbe.latest?.counters.some(p => p.attempted > 0), 'draw probe observed no draws');
  const target = process.env.NOON_CAMERA_DRAW_MODE.replace('skip-','');
  if (process.env.NOON_CAMERA_DRAW_MODE.startsWith('skip-'))
    assert.ok(drawProbe.latest.counters.some(p => p.kind === target && p.skipped > 0), `ablation target ${target} never drawn`);
  if (process.env.NOON_CAMERA_DRAW_MODE === 'clip-all')
    assert.ok(drawProbe.latest.counters.every(p => p.attempted === p.clipped), 'unclipped draw escaped');
  const report = {
    diagnosticOnly: true, cameraDrawMode: process.env.NOON_CAMERA_DRAW_MODE, drawProbe,''')
h=replace(h,'packageSizes: await packageSizes(siteRoot),','packageSizes: null, // Pages diagnostic, not Product Gate qualification.')
(root/'scripts/camera-draw-run.mjs').write_text(h)
subprocess.run(['node','--check',str(root/'scripts/camera-draw-run.mjs')],check=True)

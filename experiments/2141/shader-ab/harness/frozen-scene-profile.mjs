// Temporary, opt-in #2094 profiler. Not a production benchmark or cache gate.
// Runs exact authored source through the ordinary playground editor/Run path.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve, extname, relative, join } from 'node:path';
import { chromium } from 'playwright';
import { endpointAdmission } from './endpoint-admission.mjs';

const expectedScene = '32f2c6ee607bf9670ed47c77dd1f321ace576b5f725a523069bdcad54026bd2e';
const site = resolve(process.env.NOON_BASELINE_SITE ?? 'site');
const scenePath = resolve(process.env.NOON_BASELINE_SCENE ?? 'frozen-scene.py');
const output = resolve(process.env.NOON_BASELINE_OUTPUT ?? 'baseline-evidence');
const revision = process.env.NOON_BASELINE_REVISION;
const maxRunMs = 240_000;
const requestedRuns = Number(process.env.NOON_BASELINE_RUNS ?? '4');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const save = async (name, data) => writeFile(join(output, name), JSON.stringify(data, null, 2) + '\n');
const mime = { '.html':'text/html', '.js':'text/javascript', '.mjs':'text/javascript',
  '.json':'application/json', '.wasm':'application/wasm', '.css':'text/css',
  '.py':'text/plain', '.png':'image/png', '.svg':'image/svg+xml' };

function distribution(values) {
  const v = values.filter(n => Number.isFinite(n)).sort((a,b)=>a-b);
  if (!v.length) return null;
  return {count:v.length,min:v[0],median:v[Math.floor(v.length / 2)],max:v.at(-1)};
}
function summarize(samples) {
  const unique = [];
  const seen = new Set();
  for(const sample of samples) {
    const m=sample.metrics;
    if(!m?.ready || !Number.isFinite(m.time))continue;
    const key=`${m.clockOriginMs}:${m.session}:${m.metricReply}`;
    if(seen.has(key))continue;
    seen.add(key);unique.push(m);
  }
  return Array.from({length:7},(_,i)=>{
    const start=16+i, group=unique.filter(m=>start<=m.time && m.time<start+1);
    const counters={};
    for(const k of ['drawCalls','instancesDrawn','bytesUploaded','geometryCacheMisses','objectCount']){
      counters[k]=distribution(group.map(x=>x.counters[k]));
    }
    const spans = group.filter(m=>Number.isSafeInteger(m.frames) && Number.isFinite(m.rendererAt));
    const startSample=spans[0],endSample=spans.at(-1);
    let sampledCounterRate=null;
    if(startSample && endSample && startSample.clockOriginMs===endSample.clockOriginMs &&
      startSample.session===endSample.session && endSample.rendererAt>startSample.rendererAt &&
      endSample.frames>=startSample.frames){
      sampledCounterRate=(endSample.frames-startSample.frames)*1000/
        (endSample.rendererAt-startSample.rendererAt);
    }
    return {authoredWindow:[start,start+1],snapshotCount:group.length,
      sampledLastFrameCounters:counters,approxFrameCounterRate:sampledCounterRate,
      gpuExecutionMs:null};
  });
}

function installProbe(){
  const NativeWorker=window.Worker, ownId=Number.MAX_SAFE_INTEGER;
  const p={worker:null,pending:false,latest:null,metricsReplies:0,captureStages:false,
    samples:[],events:[],seen:new Set(),errors:[],workers:0,
    request(){if(!p.worker||p.pending)return;p.pending=true;p.worker.postMessage({channel:'noon.render',
      protocolVersion:1,type:'metrics',requestId:ownId,profilePublicationStages:p.captureStages});},
    reset(capture){p.pending=false;p.latest=null;p.captureStages=capture;p.samples=[];p.events=[];
      p.seen.clear();p.errors=[];},
  };
  window.__noon2094Probe=p;
  class ObservedWorker extends NativeWorker{
    constructor(url,opts){super(url,opts);if(!String(url).includes('execution-render-worker.js'))return;
      p.worker=this;p.workers++;p.pending=false;
      this.addEventListener('message',event=>{
        const msg=event.data;
        if(msg?.channel!=='noon.render'||msg.requestId!==ownId)return;
        event.stopImmediatePropagation();if(p.worker!==this)return;
        p.pending=false;
        if(msg.type!=='metrics'||!Number.isFinite(msg.metrics?.presentedFrames)){
          p.errors.push('bad metrics reply '+(msg.message??msg.type));return;
        }
        const m=msg.metrics;p.metricsReplies++;
        p.latest={time:m.time,frames:m.presentedFrames,session:m.presentedSession,
          clockOriginMs:m.performanceTimeOriginMs,rendererAt:m.sampledAtMs,
          ready:m.ready===true,needsPresent:m.needsPresent===true,
          bufferedDeltas:m.bufferedDeltas,metricReply:p.metricsReplies,
          counters:Object.fromEntries(['drawCalls','instancesDrawn','bytesUploaded',
            'geometryCacheMisses','objectCount','rendererRebuilds','modeSwitches']
            .map(k=>[k,m[k]]))};
        if(p.captureStages){for(const v of m.publicationStageSamples??[]){
          const key=`${m.performanceTimeOriginMs}:${v.session}:${v.sequence}`;
          if(p.seen.has(key))continue;
          if(p.seen.size>=2000){p.errors.push('stage sample bound exceeded');break;}
          p.seen.add(key);p.samples.push({...v,clockOriginMs:m.performanceTimeOriginMs});
        }}
      });
    }
  }
  Object.defineProperty(window,'Worker',{configurable:true,writable:true,value:ObservedWorker});
}

await mkdir(output,{recursive:true});
const report={schema:1,kind:'one-off baseline, no cache and not product acceptance',
  sourceSha256:expectedScene,expectedRevision:revision,startedAt:new Date().toISOString(),
  status:'incomplete',runs:[],errors:[],requestsFailed:[],consoleErrors:[]};
let browser,server;
try{
  assert.match(revision??'',/^[a-f0-9]{40}$/);
  assert(Number.isSafeInteger(requestedRuns) && requestedRuns >= 1 && requestedRuns <= 4,
    'NOON_BASELINE_RUNS must be a whole number from 1 to 4');
  const source=await readFile(scenePath);
  assert.equal(sha(source),expectedScene,'do not change the original authored scene');
  const manifest=JSON.parse(await readFile(join(site,'runtime-build-identity.json'),'utf8'));
  assert.equal(manifest.sourceRevision,revision,'loaded WASM source must match the pinned artifact');
  report.runtime={buildId:manifest.buildId,revision:manifest.sourceRevision,files:{}};
  for(const key of ['worker','wasm','glue','verifier']){
    const f=manifest.files[key],file=resolve(site,f.path);
    assert(!relative(site,file).startsWith('..'),'unsafe runtime manifest');
    const actual=sha(await readFile(file));
    assert.equal(actual,f.sha256,`runtime ${key} hash mismatch`);
    report.runtime.files[key]=actual;
  }
  server=createServer(async(req,res)=>{
    try{
      const u=new URL(req.url,'http://localhost'),decoded=decodeURIComponent(u.pathname);
      const file=resolve(site,'.'+decoded+(decoded.endsWith('/')?'index.html':''));
      assert(!relative(site,file).startsWith('..'),'unsafe URL');
      const data=await readFile(file);
      res.writeHead(200,{'Content-Type':mime[extname(file)]??'application/octet-stream',
        'Cache-Control':'no-store'});res.end(data);
    }catch{res.writeHead(404);res.end('Not found');}
  });
  await new Promise((ok,fail)=>server.listen(0,'127.0.0.1',error=>error?fail(error):ok()));
  const url=`http://127.0.0.1:${server.address().port}/index.html`;
  browser=await chromium.launch({executablePath:process.env.NOON_PROFILE_CHROME ?? '/tmp/noon-chrome/opt/google/chrome/chrome',headless:true,args:['--no-sandbox','--ignore-certificate-errors','--disable-features=WebGPU',
    '--enable-unsafe-swiftshader','--ignore-gpu-blocklist','--use-gl=angle',
    '--use-angle=swiftshader','--disable-gpu-sandbox','--disable-dev-shm-usage']});
  report.browserVersion=browser.version();
  const context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:800},deviceScaleFactor:1});
  const page=await context.newPage();
  await page.addInitScript(installProbe);
  page.on('pageerror',e=>report.errors.push(String(e)));
  page.on('console',m=>{if(m.type()==='error')report.consoleErrors.push(m.text())});
  page.on('requestfailed',r=>report.requestsFailed.push({url:r.url(),failure:r.failure()}));
  await page.goto(url,{waitUntil:'load',timeout:30000});
  await page.waitForFunction(()=>window.__noonExampleGallery!==undefined,{timeout:30000});
  const editor=page.locator('#python-scene-source');
  await editor.evaluate((e,s)=>{e.value=s},source.toString('utf8'));
  for(let index=0;index<requestedRuns;index++){
    const run={index,kind:index===0?'cold':'fresh-scene-same-page',status:'incomplete',
      startedAt:new Date().toISOString(),samples:[],stageSamples:[]};report.runs.push(run);
    const authored=await editor.inputValue();assert.equal(sha(Buffer.from(authored)),expectedScene);
    await page.evaluate(c=>window.__noon2094Probe.reset(c),true);
    const started=performance.now();
    await page.locator('#replace-scene').click();
    let sawRun=false,sawProgress=false,epoch=null,progressRebuilds=null,progressFrames=0;
    try{
      while(performance.now()-started < maxRunMs){
        const observed=await page.evaluate(()=>{
          const p=window.__noon2094Probe,canvas=document.querySelector('#scene');
          p.request();const rect=canvas?.getBoundingClientRect();
          return {at:performance.now(),metrics:p.latest,replies:p.metricsReplies,
            runInFlight:window.__noonExampleGallery?.runInFlight??null,
            patchState:document.querySelector('#patch-status')?.dataset.state??null,
            statusText:document.querySelector('#status-text')?.textContent??'',
            backend:document.querySelector('#status')?.dataset.rendererBackend??null,
            errors:p.errors,workers:p.workers,
            backing:canvas?[canvas.width,canvas.height]:null,
            css:rect?[rect.width,rect.height]:null,dpr:devicePixelRatio};
        });
        run.samples.push(observed);sawRun||=observed.runInFlight===true;
        if(observed.errors?.length||report.errors.length)throw Error('runtime error '+JSON.stringify(observed.errors));
        if(observed.patchState==='error')throw Error('Run failed '+observed.statusText);
        const m=observed.metrics;
        if(observed.runInFlight===true&&m?.ready&&m.time>=0&&m.time<24.999){
          if(!sawProgress){
            epoch=[m.session,m.clockOriginMs];
            progressRebuilds=m.counters.rendererRebuilds;
          }
          if(m.session===epoch?.[0]&&m.clockOriginMs===epoch?.[1]){
            sawProgress=true;
            progressFrames=Math.max(progressFrames,m.frames);
          }
        }
        const endpoint=endpointAdmission({observed,sawRun,sawProgress,epoch,
          progressRebuilds,progressFrames});
        if(endpoint){
          run.completion=endpoint;
          run.status='endpoint-observed';break;
        }
        await page.waitForTimeout(50);
      }
      if(run.status!=='endpoint-observed')throw Error(`25 s endpoint not observed in ${maxRunMs} ms`);
    }finally{
      run.elapsedMs=performance.now()-started;
      run.workerProfiles=await Promise.all(page.workers().map(async w=>({url:w.url(),profile:await w.evaluate(()=>globalThis.__noonProfile??null).catch(e=>({error:String(e)}))})));
      run.screenshot='frame-'+index+'.png';
      await page.screenshot({path:join(output,run.screenshot)}).catch(()=>{});
      run.stageSamples=await page.evaluate(()=>window.__noon2094Probe.samples).catch(()=>[]);
      run.exposureIntervals=summarize(run.samples);
      run.finishedAt=new Date().toISOString();
      await save(`run-${index}.json`,run);
    }
  }
  assert.equal(sha(Buffer.from(await editor.inputValue())),expectedScene);
  const systemSession=await browser.newBrowserCDPSession();
  report.browserSystemInfo=await systemSession.send('SystemInfo.getInfo');
  await systemSession.detach();
  report.status=report.errors.length||report.requestsFailed.length||report.consoleErrors.length
    ?'collected-with-environment-errors':'collected-unqualified';
}catch(error){report.error=String(error?.stack??error);}
finally{
  report.finishedAt=new Date().toISOString();
  await save('report.json',report);
  await browser?.close().catch(()=>{});
  if(server?.listening)await new Promise(ok=>server.close(ok));
}
console.log(JSON.stringify({status:report.status,error:report.error,runs:report.runs.length,
  revision,sourceSha:expectedScene}));
if(report.status!=='collected-unqualified')process.exitCode=1;

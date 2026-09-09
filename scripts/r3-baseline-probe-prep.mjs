// Development-only baseline evidence; not a product workflow or mapper.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { chromium } from 'playwright';
const artifacts='r3-baseline-evidence';
await mkdir(artifacts,{recursive:true});
const manifest=JSON.parse(await readFile('web/ci-artifact.json','utf8'));
assert.equal(manifest.source,'d730aa23160e0e551dd9b378f10d22d58645ba15');
for(const [path,sha] of Object.entries(manifest.files))assert.equal(createHash('sha256').update(await readFile(path)).digest('hex'),sha,path);
const source=await readFile('scripts/typed-authoring-errors-smoke.mjs','utf8');
const start=source.indexOf('  await page.evaluate(async () => {')+'  await page.evaluate('.length;
const end=source.indexOf('\n\n  report.javascript =',start);
const fixtures=source.slice(start,end).trim().slice(0,-2);
const worker=await readFile('web/python-worker.source.js','utf8');
const pyodideUrl=worker.match(/import \{ loadPyodide \} from "([^"]+)";/)[1];
const report={source:manifest.source,wasm:manifest.files['web/pkg/noon_web_bg.wasm'],pyodideUrl,fixtureCommit:'862df403c57a9a98c24fec6ef83580c8e2e910e0'};
let logs='';
const server=spawn('python3',['-m','http.server','4198','--bind','127.0.0.1'],{stdio:['ignore','pipe','pipe']});
server.stdout.on('data',x=>logs=(logs+x).slice(-64000));server.stderr.on('data',x=>logs=(logs+x).slice(-64000));
const exited=new Promise(resolve=>server.once('exit',resolve));
let browser;
try {
 for(let i=0;;i++){try{if((await fetch('http://127.0.0.1:4198/')).ok)break;}catch{}if(i>=80)throw new Error('server startup failed');await new Promise(r=>setTimeout(r,100));}
 browser=await chromium.launch({channel:'chromium',headless:true});
 report.browser=browser.version();report.node=process.version;
 const page=await browser.newPage();
 page.on('console',x=>logs=(logs+`\n${x.type()}: ${x.text()}`).slice(-64000));
 await page.goto('http://127.0.0.1:4198/');
 await page.evaluate(`(${fixtures})()`);
 assert.equal(await page.evaluate(()=>typeof window.noonTypedErrorFixtures?.membershipFixture),'function');
 report.python=await page.evaluate(async pyodideUrl=>{
  const {loadPyodide}=await import(pyodideUrl);const py=await loadPyodide();
  window.baselineFixture=(kind,live)=>{
   const f=kind==='ownership'?window.noonTypedErrorFixtures.ownershipFixture(live):window.noonTypedErrorFixtures.membershipFixture(kind,live);
   let rejected;
   return {...f,reject:()=>{try{f.reject();}catch(e){rejected=e;throw e;}},recoverOriginal:()=>f.recover(rejected.takePlayer())};
  };
  return JSON.parse(py.runPython(`
import json
from js import baselineFixture
rows=[]
for kind,live in [("foreign",False),("missing",False),("ambiguous",False),("foreign",True),("missing",True),("pending",True),("ownership",1),("ownership",2)]:
    fixture=baselineFixture(kind,live)
    try:
        try:
            fixture.reject()
        except Exception as error:
            row={"kind":kind,"live":live,"python_type":type(error).__name__,"message":str(error),"category":getattr(error,"category",None),"code":getattr(error,"code",None),"take_player":callable(getattr(error,"takePlayer",None))}
            fixture.assertAtomic()
            row["atomic"]=True
            if kind=="ownership":
                if row["take_player"]:
                    fixture.recover(error.takePlayer())
                    row["recovery_through_python_error"]=True
                else:
                    fixture.recoverOriginal()
                    row["test_only_original_cleanup"]=True
            else:
                fixture.recover()
                row["recovered"]=True
            rows.append(row)
        else:
            raise AssertionError("invalid operation accepted")
    finally:
        fixture.dispose()
json.dumps(rows)
`));
 },pyodideUrl);
 assert.equal(report.python.length,8);
 await writeFile(`${artifacts}/results.json`,JSON.stringify(report,null,2));
 console.log(JSON.stringify(report,null,2));
} catch(error){await writeFile(`${artifacts}/failure.txt`,error.stack??String(error));throw error;}
finally{await browser?.close();server.kill('SIGTERM');await exited;await writeFile(`${artifacts}/browser.log`,logs);}

import { webkit, devices } from 'playwright';
import { mkdir, writeFile } from 'node:fs/promises';
const base='https://yongkyuns.github.io/noon/';
const stringify=(value)=>JSON.stringify(value,(_,v)=>typeof v==='bigint'?String(v):v,2);
await mkdir('mobile-diagnosis',{recursive:true});
const revision=await fetch(`${base}build-info.json?t=${Date.now()}`).then(r=>r.json());
const results=[];
for(const variant of ['upstream-314.0.5','upstream-314.0.6','upstream-314.0.4','promising-314.0.5','async-control','without-jspi']) {
  const browser=await webkit.launch({headless:true});
  const context=await browser.newContext({...devices['iPhone 13']});
  const page=await context.newPage();
  const result={variant,revision,errors:[]};
  page.on('pageerror',e=>result.errors.push(String(e)));
  page.on('console',m=>{if(m.type()==='error')result.errors.push(m.text())});
  try {
    if(variant.startsWith('upstream-')||variant.startsWith('promising-')) {
      await context.route('**/__diagnostic__',r=>r.fulfill({contentType:'text/html',body:'<!doctype html><title>interpreter diagnostic</title>'}));
      await page.goto(`${base}__diagnostic__`);
      result.value=await page.evaluate(async variant=>{
        const version=variant.split('-').at(-1);
        const code='from pyodide.ffi import run_sync\nfrom js import Promise\nrun_sync(Promise.resolve(123))';
        const source=`import {loadPyodide} from 'https://cdn.jsdelivr.net/pyodide/v${version}/full/pyodide.mjs';
          self.onunhandledrejection=e=>postMessage({error:String(e.reason)});
          try {const p=await loadPyodide();
            let result;
            if(${JSON.stringify(variant.startsWith('promising-'))}) {
              const f=p.runPython('def f():\\n    from pyodide.ffi import run_sync\\n    from js import Promise\\n    return run_sync(Promise.resolve(123))\\nf');
              try {result=await f.callPromising()} finally{f.destroy()}
            } else {result=await p.runPythonAsync(${JSON.stringify(code)})}
            postMessage({result});
          } catch(e){postMessage({error:String(e),stack:e.stack})}`;
        const url=URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
        const worker=new Worker(url,{type:'module'});
        try{return await new Promise((resolve)=>{
          const timer=setTimeout(()=>resolve({timeout:true}),35000);
          worker.onmessage=e=>{clearTimeout(timer);resolve(e.data)};
          worker.onerror=e=>{clearTimeout(timer);resolve({error:e.message})};
        })} finally{worker.terminate();URL.revokeObjectURL(url)}
      },variant);
    } else {
      await context.route('**/python-worker.js',async route=>{
        const response=await route.fetch();
        let body=await response.text();
        if(variant==='without-jspi')body='delete WebAssembly.promising;delete WebAssembly.Suspending;\n'+body;
        if(variant==='async-control')body=body.replace('globals.set("__noon_source", source);','globals.set("__noon_source", source.replaceAll("def construct(self):", "async def construct(self):").replaceAll("self.play(", "await self.play(").replaceAll("self.wait(", "await self.wait("));');
        await route.fulfill({response,body});
      });
      await page.goto(`${base}?example=parity-create-circle&renderHost=main-thread`,{waitUntil:'domcontentloaded'});
      await page.waitForFunction(()=>['error','applied'].includes(document.querySelector('#patch-status')?.dataset.state),null,{timeout:45000});
      result.state=await page.evaluate(async()=>({patch:{...document.querySelector('#patch-status')?.dataset},text:document.querySelector('#patch-status')?.value,metrics:document.querySelector('#patch-status')?.dataset.state==='applied'?await window.__noonExampleGallery.executionMetrics():null}));
      await page.locator('#scene').scrollIntoViewIfNeeded();
      await page.screenshot({path:`mobile-diagnosis/${variant}.png`});
    }
  }catch(e){result.exception=String(e)}
  results.push(result);
  console.log('DIAGNOSIS',stringify(result));
  await writeFile('mobile-diagnosis/results.json',stringify(results));
  await browser.close();
}
for(const version of ['314.0.5','314.0.6']){
  const url=`https://cdn.jsdelivr.net/pyodide/v${version}/full/pyodide.asm.mjs`;
  const response=await fetch(url);
  if(response.ok)await writeFile(`mobile-diagnosis/pyodide-${version}.mjs`,await response.text());
}

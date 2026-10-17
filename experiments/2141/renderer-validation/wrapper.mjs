
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {createRequire,registerHooks} from "node:module";
import {pathToFileURL} from "node:url";
import {serveRepository} from "./scripts/browser-test-server.mjs";
const require=createRequire(import.meta.url);
const playwright=require("playwright");
const playwrightUrl=pathToFileURL(require.resolve("playwright")).href;
registerHooks({
  resolve(specifier,context,nextResolve) {
    if (specifier === "playwright") return {url:playwrightUrl,shortCircuit:true};
    return nextResolve(specifier,context);
  }
});
const artifactRoot="/tmp/noon-packed-browser-gallery-direct";
async function identity() {
  const bytes=await readFile("web/runtime-build-identity.json");
  const runtime=JSON.parse(bytes);
  const wasm=await readFile("web/pkg/noon_web_bg.wasm");
  const wasmSha256=createHash("sha256").update(wasm).digest("hex");
  assert.equal(wasmSha256,runtime.files.wasm.sha256,"WASM differs from package identity");
  return {
    head:execFileSync("git",["rev-parse","HEAD"],{encoding:"utf8"}).trim(),
    runtime,identitySha256:createHash("sha256").update(bytes).digest("hex"),wasmSha256,wasmBytes:wasm.length
  };
}
let backend;
const originalLaunch=playwright.chromium.launch.bind(playwright.chromium);

playwright.chromium.launch=async options=>{
  const args=backend === "webgpu" ? [
    "--enable-unsafe-webgpu","--enable-unsafe-swiftshader","--use-webgpu-adapter=swiftshader",
    "--use-gpu-in-tests","--ignore-gpu-blocklist","--enable-features=Vulkan",
    "--use-gl=angle","--use-angle=swiftshader","--use-vulkan=swiftshader",
    "--disable-gpu-sandbox","--disable-dev-shm-usage"
  ] : (options.args ?? []);
  const browser=await originalLaunch({
    ...options,executablePath:"/tmp/noon-chrome151/chrome-linux64/chrome",
    args:[...args,"--ignore-certificate-errors"]
  });
  const originalNewContext=browser.newContext.bind(browser);
  
browser.newContext=async options=>{
    const context=await originalNewContext({...options,ignoreHTTPSErrors:true});
    const originalRoute=context.route.bind(context);
    context.route=(matcher,handler,routeOptions)=>originalRoute(matcher,(route,...args)=>{
      const url=new URL(route.request().url());
      if(url.hostname === "cdn.jsdelivr.net" && url.pathname.startsWith("/pyodide/v314.0.5/full/")) {
        return route.continue();
      }
      return handler(route,...args);
    },routeOptions);
    return context;
  };
  return browser;
};
const summaries=[];
for (backend of ["webgl","webgpu"]) {
  const artifacts=artifactRoot+"/"+backend;
  await mkdir(artifacts,{recursive:true});
  const before=await identity();
  assert.ok(before.head.startsWith("495580e"),"unexpected source HEAD");
  assert.equal(before.runtime.sourceRevision,before.head,"runtime identity not aligned to HEAD");
  assert.equal(before.runtime.buildId,"b0170cc832f3a35996637589a3004cfaa32046028f995392ad5c78775fd890c5");
  console.log("PACKED_GALLERY_START",JSON.stringify({backend,head:before.head,buildId:before.runtime.buildId,wasmSha256:before.wasmSha256}));
  const probe=await serveRepository(process.cwd(),0);
  const port=new URL(probe.baseUrl).port;
  await probe.close();
  delete process.env.NOON_GALLERY_BASE;
  delete process.env.NOON_GALLERY_SHARD_INDEX;
  delete process.env.NOON_GALLERY_SHARD_COUNT;
  process.env.NOON_GALLERY_PORT=port;
  process.env.NOON_GALLERY_CASES="noon-transform-matching-shapes-breadth";
  process.env.NOON_PLAYGROUND_BROWSER="chromium";
  process.env.NOON_PLAYGROUND_PROFILE="desktop-dpr1";
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS=artifacts;
  let failure=null;
  try {
    await import("./scripts/playground-gallery-runtime-smoke.mjs?packed-backend="+backend);
    const cases=JSON.parse(await readFile(artifacts+"/results.json","utf8"));
    assert.equal(cases.length,1);
    assert.equal(cases[0].outcome,"pass");
    assert.equal(cases[0].state.rendererBackend,backend === "webgpu" ? "WebGPU" : "WebGL2");
  } catch(error) {
    failure=error.stack || String(error);
    console.error("PACKED_GALLERY_FAILURE",JSON.stringify({backend,error:failure}));
  }
  const after=await identity();
  assert.equal(after.wasmSha256,before.wasmSha256,"WASM changed during gallery validation");
  assert.equal(after.identitySha256,before.identitySha256,"runtime identity changed during gallery validation");
  const summary={backend,before,after,passed:failure === null,failure,harnessAdaptation:"Pinned Pyodide assets use Chromium networking instead of the optional Node route.fetch byte cache; test assertions and production sources are unchanged."};
  summaries.push(summary);
  await writeFile(artifacts+"/validated-runtime.json",JSON.stringify(summary,null,2)+"\n");
  console.log("PACKED_GALLERY_RESULT",JSON.stringify({backend,passed:summary.passed,artifacts}));
}
await writeFile(artifactRoot+"/summary.json",JSON.stringify(summaries,null,2)+"\n");
assert.ok(summaries.every(result=>result.passed),"one or more production gallery checks failed");


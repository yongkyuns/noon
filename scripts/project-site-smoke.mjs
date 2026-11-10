import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { qualifyDocumentation } from "./project-site-docs-smoke.mjs";
import { installLayoutPlaceholder } from "./project-site-preview-shell.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "browser-smoke-artifacts/project-site");
await mkdir(output, { recursive: true });
const server = await serveRepository(root, 0);
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const base = `${server.baseUrl}/_site/index.html`;
  const selections = JSON.parse(await readFile(path.join(root, "web/site/examples.json"), "utf8"));
  for (const width of [1440, 768, 390, 320]) {
    const context = await browser.newContext({viewport:{width,height:1000}});
    await installLayoutPlaceholder(context);
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    await page.goto(base);
    await page.waitForFunction(count=>document.querySelectorAll('iframe[data-shell-ready=true]').length===count,selections.length);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${width}: horizontal overflow`);
    assert.equal(await page.locator("h1").textContent(), "Animated visuals,written in code.");
    assert.equal(await page.locator('video, .demo-poster, [data-demo-open]').count(),0,'homepage shows live surfaces, not captured clips');
    for (const name of ["Native Python", "Native Rust"]) await page.locator("summary").filter({hasText:name}).click();
    assert.equal(await page.locator("details[open]").count(),2);
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true,`${width}: expanded code overflows`);
    await page.screenshot({path:path.join(output,`homepage-${width}.png`),fullPage:true});
    assert.deepEqual(errors,[]);
    await context.close();
  }
  const context = await browser.newContext();
  await installLayoutPlaceholder(context);
  const page = await context.newPage();
  await page.goto(base);
  for (const asset of await page.locator('[src], [poster], link[rel=stylesheet]').evaluateAll(elements=>
    elements.flatMap(element=>[element.getAttribute('src'),element.getAttribute('poster'),element.getAttribute('href')]).filter(Boolean))) {
    const url = new URL(asset,base);
    if (url.origin===new URL(base).origin) assert.equal((await page.request.get(url.href)).ok(),true,asset);
  }
  // GIFs remain the README's portable fallback, not the website renderer.
  for (const {id} of selections) {
    const decoded = await page.evaluate(async id=>{
      const image = new Image(); image.src=`./site/media/${id}.gif`; await image.decode();
      return {width:image.naturalWidth,height:image.naturalHeight};
    },id);
    assert.deepEqual(decoded,{width:480,height:270},id);
  }
  await page.route('**/playground.html?**',route=>route.fulfill({contentType:'text/html',body:'<title>Playground destination</title>'}));
  await page.goto(`${base}?catalog=showcase&example=showcase-first-scene&backend=webgl#editor`);
  await page.waitForURL('**/playground.html?catalog=showcase&example=showcase-first-scene&backend=webgl#editor');
  await context.close();
  const noJs = await browser.newContext({javaScriptEnabled:false});
  const plain = await noJs.newPage();
  await plain.goto(base);
  assert.equal(await plain.locator('h1').isVisible(),true);
  assert.ok(await plain.locator('a[href*="playground.html"]').count()>0);
  assert.equal(await plain.locator('iframe').count(),0);
  await noJs.close();
  await qualifyDocumentation(browser,base,output);
  console.log('Project site layout/navigation passed; actual autoplay and hot reload are qualified separately.');
} finally { await browser?.close(); await server.close(); }

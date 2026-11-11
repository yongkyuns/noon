import assert from 'node:assert/strict';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {mkdir} from 'node:fs/promises';
import {chromium} from 'playwright';
import {serveRepository} from './browser-test-server.mjs';
import {installLayoutPlaceholder} from './project-site-preview-shell.mjs';

// Design/interaction gate. Runtime is explicitly stubbed here, never in the
// separate real multi-browser autoplay/hot-reload qualification.
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.join(root,'browser-smoke-artifacts/project-site/design');
await mkdir(output,{recursive:true});
const server = await serveRepository(root,0);
const base = server.baseUrl + '/_site/';
let browser;
try {
 browser = await chromium.launch({headless:true});
 const context = await browser.newContext();
 await installLayoutPlaceholder(context);
 await context.addInitScript(()=>{
   window.__copied = null;
   Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.__copied=text;}}});
 });
 const page = await context.newPage();
 const errors=[]; page.on('pageerror',error=>errors.push(String(error)));
 await page.goto(base+'site/docs/getting-started.html');
 await page.locator('[data-copy-code]').first().waitFor({state:'visible'});
 const source=await page.locator('.code-block code').first().textContent();
 await page.locator('[data-copy-code]').first().click();
 await page.waitForFunction(text=>window.__copied===text,source);
 await page.evaluate(()=>Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw Error('denied')}}}));
 await page.locator('[data-copy-code]').first().click();
 assert.equal(await page.evaluate(()=>getSelection().toString()),source,'denied clipboard selects exact source');
 await page.goto(base+'site/docs/api.html');
 await page.locator('[data-api-filter]').fill('Circle');
 assert.ok(await page.locator('[data-api-entry]:visible').count()>0);
 assert.ok(await page.locator('[data-api-entry][hidden]').count()>0);
 await page.locator('[data-api-filter]').fill('__no_matching_symbol__');
 assert.equal(await page.locator('[data-api-entry]:visible').count(),0);
 assert.match(await page.locator('[data-api-filter-status]').textContent(),/0 exported/);
 await page.goto(base+'site/docs/search.html?q=Scene');
 await page.waitForFunction(()=>document.querySelector('[data-search-status]').textContent.includes('matching'));
 assert.equal((await page.locator('[data-search-item]:visible a').first().textContent()).trim(),'Scene','exact symbol first');
 await page.locator('#search-query').fill('not-a-real-topic-987');
 assert.match(await page.locator('[data-search-status]').textContent(),/No matches/);
 await page.locator('#search-query').fill('<img onerror=alert(1)>');
 assert.equal(await page.locator('.topic-index img').count(),0,'query is text, not markup');
 for(const width of [1440,390,320]) {
   await page.setViewportSize({width,height:1000});
   await page.goto(base+'site/docs/index.html');
   await page.waitForFunction(()=>document.documentElement.dataset.theme);
   assert.equal(await page.locator('.docs-navigation').getAttribute('open')!==null,width>900);
   await page.keyboard.press('Control+k');
   assert.equal(await page.locator('#docs-search').evaluate(el=>document.activeElement===el),true);
   if(width<=900) {
     await page.locator('#docs-search').press('Escape');
     assert.equal(await page.locator('.docs-navigation').getAttribute('open'),null);
   }
   await page.screenshot({path:path.join(output,`navigation-${width}.png`)});
 }
 assert.deepEqual(errors,[]);
 await context.close();
 // With JS disabled, the disclosure itself remains operable at every size.
 const plain=await browser.newContext({javaScriptEnabled:false});const nojs=await plain.newPage();
 for(const width of [1440,390]) {
  await nojs.setViewportSize({width,height:1000});await nojs.goto(base+'site/docs/index.html');
  assert.equal(await nojs.locator('.docs-navigation>summary').isVisible(),true);
  await nojs.locator('.docs-navigation>summary').click();
  assert.equal(await nojs.locator('.docs-sidebar a[href="getting-started.html"]').isVisible(),true);
 }
 await plain.close();
 // Include the actual late-injected showcase stylesheet, not a weaker CSS mock.
 const shell=await browser.newContext({viewport:{width:390,height:240}});
 await shell.route('**/python-editor-bootstrap.js',r=>r.fulfill({contentType:'text/javascript',body:'// runtime intentionally omitted in presentation test'}));
 await shell.route('**/main.js',r=>r.fulfill({contentType:'text/javascript',body:`
   import {installShowcasePresentation} from './showcase-presentation.js';
   const summary=document.createElement('div');summary.className='selected-example';summary.textContent='Gallery description must stay out of the embed';
   document.querySelector('.workspace').before(summary);
   installShowcasePresentation(document,true);
   document.querySelector('#replace-scene').disabled=false;
   document.querySelector('#patch-status').dataset.state='applied';
 `}));
 const view=await shell.newPage();await view.goto(base+'embed.html?catalog=showcase&example=showcase-first-scene');
 await view.waitForFunction(()=>document.documentElement.dataset.noonCatalog==='showcase');
 assert.equal(await view.locator('.selected-example').isVisible(),false);
 assert.equal(await view.locator('.topbar').isVisible(),false);
 const bounds=await view.locator('#scene').boundingBox();
 assert.ok(bounds.y>=0 && bounds.y+bounds.height<=241,'late gallery chrome does not move canvas offscreen');
 assert.equal(await view.locator('button:visible').count(),2);
 await shell.close();
 console.log('Documentation design: search, copy, filters, keyboard, no-JS navigation and late showcase CSS passed. No runtime claim.');
} finally {await browser?.close();await server.close();}

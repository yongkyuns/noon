import assert from 'node:assert/strict';
import path from 'node:path';

// Navigation-only placeholder. This does not qualify execution or pixels.
export async function installLayoutPlaceholder(context) {
  await context.route('**/embed.html?**', route => route.fulfill({contentType:'text/html', body:
    `<!doctype html><title>Layout-only live scene placeholder</title><script>
      parent.postMessage({type:'noon-site-embed-state',state:'ready',view:'view'},location.origin);
    </script>`}));
}

// Actual shell/CSS/tabs, with the gallery runtime explicitly stubbed. The separate
// three-engine gate uses the untouched runtime and actual rendered canvas pixels.
export async function qualifyPreviewShell(browser, base, output) {
  const root = new URL('./', base);
  const context = await browser.newContext();
  await context.route('**/python-editor-bootstrap.js', route => route.fulfill({
    contentType:'text/javascript',body:'// Layout-only fixture: do not start WASM or Python.',
  }));
  await context.route('**/main.js', route => route.fulfill({contentType:'text/javascript',body:`
    document.querySelector('#python-scene-source').value = '# Layout-only fixture\\nfrom noon import *';
    document.querySelector('#replace-scene').disabled = false;
    document.querySelector('#patch-status').dataset.state = 'applied';
  `}));
  try {
    const page = await context.newPage();
    for (const colorScheme of ['light','dark']) for (const width of [1440,768,390,320]) {
      await page.emulateMedia({colorScheme});
      await page.setViewportSize({width,height:1000});
      await page.goto(new URL('site/docs/examples.html',root).href);
      await page.waitForFunction(() => document.querySelectorAll('iframe[data-shell-ready=true]').length === 4);
      const cards = page.locator('[data-demo]');
      for (let index=0;index<await cards.count();index++) {
        const card = cards.nth(index);
        const element = await card.locator('iframe').elementHandle();
        const frame = await element.contentFrame();
        const bounds = await frame.locator('#scene').boundingBox();
        const iframe = await card.locator('iframe').boundingBox();
        assert.ok(bounds && iframe && bounds.height >= 80,`visible canvas at ${width}`);
        assert.ok(bounds.y >= iframe.y && bounds.y+bounds.height <= iframe.y+iframe.height+1,'canvas fits without nested scrolling');
        assert.equal(await frame.locator('.editor-pane').isVisible(),false);
        assert.equal(await frame.locator('button:visible').count(),2,'only View/Code in the normal view');
        await frame.getByRole('tab',{name:'Code',exact:true}).click();
        await frame.locator('#python-scene-source').fill(`# retained draft ${index}`);
        assert.equal(await frame.locator('.preview-pane').isVisible(),false);
        assert.ok((await card.locator('iframe').boundingBox()).height>=350,'code has useful editing height');
        await frame.getByRole('tab',{name:'View',exact:true}).click();
        await frame.getByRole('tab',{name:'Code',exact:true}).click();
        assert.equal(await frame.locator('#python-scene-source').inputValue(),`# retained draft ${index}`);
        assert.equal(await element.evaluate(node=>node.isConnected),true,'tab changes keep the same frame');
        // Arrow navigation, selection state, and focus are part of the switch.
        await frame.getByRole('tab',{name:'Code',exact:true}).press('ArrowLeft');
        assert.equal(await frame.getByRole('tab',{name:'View',exact:true}).getAttribute('aria-selected'),'true');
      }
      assert.equal(await page.locator('iframe').count(),4,'examples do not evict one another');
      assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
      await page.screenshot({path:path.join(output,`live-shell-${colorScheme}-${width}.png`),fullPage:true});
    }
    await context.route('**/embed.html?**',route=>route.abort('failed'));
    await page.clock.install();
    await page.goto(new URL('site/docs/examples.html',root).href);
    await page.clock.fastForward(31000);
    assert.equal(await page.locator('iframe').count(),0);
    assert.match(await page.locator('[data-demo-status]').first().textContent(),/could not load|did not respond/);
    assert.equal(await page.locator('[data-demo-retry]:visible').count(),4);
  } finally { await context.close(); }
}

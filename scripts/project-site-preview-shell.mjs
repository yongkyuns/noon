import assert from 'node:assert/strict';
import path from 'node:path';

// Deliberately no engine execution here. This isolates preview-first layout,
// activation, context replacement/close, and failure behavior from rendering.
export async function qualifyPreviewShell(browser, base, output) {
  const root = new URL('./', base);
  const context = await browser.newContext();
  await context.route('**/main.js', route => route.fulfill({
    contentType:'text/javascript',body:`document.querySelector('#replace-scene').disabled = false;`,
  }));
  try {
    const page = await context.newPage();
    for (const width of [1440,768,390,320]) {
      await page.setViewportSize({width,height:1000});
      await page.goto(new URL('site/docs/examples.html',root).href);
      assert.equal(await page.locator('iframe').count(),0);
      const cards = page.locator('[data-demo]');
      for (let index=0;index<await cards.count();index++) {
        await cards.nth(index).locator('[data-demo-open]').click();
        await page.waitForFunction(() => document.querySelector('iframe')?.dataset.shellReady === 'true');
        assert.equal(await page.locator('iframe').count(),1,'one active preview across all cards');
        const frame = await page.locator('iframe').elementHandle().then(handle=>handle.contentFrame());
        const bounds = await frame.locator('#scene').boundingBox();
        const iframe = await page.locator('iframe').boundingBox();
        assert.ok(bounds && iframe && bounds.height >= 80,`visible canvas at ${width}`);
        assert.ok(bounds.y >= iframe.y && bounds.y+bounds.height <= iframe.y+iframe.height+1,`canvas must fit the frame at ${width}`);
        assert.equal(await frame.locator('.editor-pane').isVisible(),false);
      }
      const last = cards.last();
      await last.locator('[data-demo-close]').click();
      assert.equal(await page.locator('iframe').count(),0);
      assert.equal(await last.locator('[data-demo-open]').evaluate(element=>element===document.activeElement),true);
      await page.screenshot({path:path.join(output,`preview-shell-${width}.png`)});
    }
    // A transport failure cannot be presented as a successful load.
    await context.unroute('**/main.js');
    await context.route('**/embed.html?**',route=>route.abort('failed'));
    await page.goto(new URL('site/docs/examples.html',root).href);
    await page.clock.install();
    await page.locator('[data-demo-open]').first().click();
    await page.clock.fastForward(31000);
    assert.equal(await page.locator('iframe').count(),0);
    assert.match(await page.locator('[data-demo-status]').first().textContent(),/could not load|did not respond/);
  } finally { await context.close(); }
}

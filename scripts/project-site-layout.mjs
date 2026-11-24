import assert from 'node:assert/strict';
import path from 'node:path';
import { writeFile } from 'node:fs/promises';

// Inspect text fragments as well as element boxes: an unbroken word can escape
// a paragraph whose own bounding box still fits. Ignore deliberate scroll areas
// such as code blocks, rather than hiding document overflow to pass the check.
export async function assertDocumentWidth(page, label, output) {
  const width = await page.evaluate(() => ({ viewport: innerWidth, document: document.documentElement.scrollWidth }));
  if (width.document <= width.viewport) return;
  const evidence = await page.evaluate(() => {
    const fragments = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode()) && fragments.length < 12) {
      if (!node.textContent.trim()) continue;
      let parent = node.parentElement;
      let contained = false;
      while (parent && parent !== document.body) {
        if (['auto', 'scroll', 'hidden', 'clip'].includes(getComputedStyle(parent).overflowX)) {
          contained = true;
          break;
        }
        parent = parent.parentElement;
      }
      if (contained) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const bounds = [...range.getClientRects()].find(rect => rect.right > innerWidth + 0.5 || rect.left < -0.5);
      if (bounds) fragments.push({
        tag: node.parentElement.tagName, id: node.parentElement.id,
        text: node.textContent.slice(0, 400), bounds: bounds.toJSON(),
      });
    }
    return { url: location.href, viewport: innerWidth, document: document.documentElement.scrollWidth, fragments };
  });
  if (output) {
    const name = label.replace(/[^a-z0-9_-]+/gi, '-');
    await writeFile(path.join(output, `${name}-overflow.json`), `${JSON.stringify(evidence, null, 2)}\n`);
    await page.screenshot({ path: path.join(output, `${name}-overflow.png`) }).catch(() => {});
  }
  assert.fail(`${label}: horizontal overflow (${width.document} > ${width.viewport}); ${JSON.stringify(evidence.fragments)}`);
}

// Exercise the actual documentation stylesheet at the current viewport/theme.
// The fixture is removed afterwards; it neither changes canonical text nor
// substitutes for checking the real generated pages above/below this call.
export async function qualifyLongProse(page, label, output) {
  const prose = 'transform/style/bounds/resource/timeline/reactive';
  const link = 'https://example.invalid/' + 'unbrokentopic'.repeat(12);
  const code = 'retain_source_spacing(' + 'argument,'.repeat(100) + ');';
  await page.evaluate(({ prose, link, code }) => {
    const fixture = document.createElement('section');
    fixture.dataset.layoutRegression = 'true';
    for (const text of [prose, link]) {
      const paragraph = document.createElement('p');
      if (text === link) {
        const anchor = document.createElement('a');
        anchor.href = text; anchor.textContent = text;
        paragraph.append(anchor);
      } else paragraph.textContent = text;
      fixture.append(paragraph);
    }
    const pre = document.createElement('pre');
    pre.tabIndex = 0;
    const source = document.createElement('code');
    source.textContent = code;
    pre.append(source); fixture.append(pre);
    document.querySelector('.docs-content').append(fixture);
  }, { prose, link, code });
  try {
    await assertDocumentWidth(page, label, output);
    const result = await page.locator('[data-layout-regression]').evaluate(fixture => {
      const paragraphs = [...fixture.querySelectorAll('p')];
      const pre = fixture.querySelector('pre');
      return {
        text: paragraphs.map(p => p.textContent), code: pre.textContent,
        visibleProse: paragraphs.every(p => ['visible'].includes(getComputedStyle(p).overflowX)),
        codeWhiteSpace: getComputedStyle(pre).whiteSpace,
        codeScrollable: ['auto', 'scroll'].includes(getComputedStyle(pre).overflowX) && pre.scrollWidth > pre.clientWidth,
        documentVisible: [document.body, document.documentElement, fixture.closest('.docs-content')]
          .every(node => !['hidden', 'clip'].includes(getComputedStyle(node).overflowX)),
      };
    });
    assert.deepEqual(result.text, [prose, link], `${label}: prose unchanged`);
    assert.equal(result.code, code, `${label}: code unchanged`);
    assert.equal(result.visibleProse && result.documentVisible, true, `${label}: do not clip prose to mask overflow`);
    assert.equal(result.codeWhiteSpace, 'pre', `${label}: preserve source line layout`);
    assert.equal(result.codeScrollable, true, `${label}: long code scrolls inside its own panel`);
  } finally {
    await page.locator('[data-layout-regression]').evaluate(fixture => fixture.remove());
  }
}

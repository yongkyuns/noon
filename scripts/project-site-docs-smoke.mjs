import assert from "node:assert/strict";
import path from "node:path";
import { readFile } from "node:fs/promises";
import { qualifyPreviewShell } from "./project-site-preview-shell.mjs";

const pages = JSON.parse(await readFile(new URL("./site/navigation.json", import.meta.url), "utf8")).map(page => page.slug);

// This suite validates documentation presentation and navigation. Playground
// documents are deliberately stubbed; it is NOT an embedded-renderer proof.
export async function qualifyDocumentation(browser, base, output) {
  const root = new URL("./", base);
  const validateContrast = async (page) => {
    const contrast = (values) => {
      const luminance = (rgb) => {
        const channels = rgb.match(/[\d.]+/g).slice(0, 3).map(Number).map(value => value / 255);
        return channels.reduce((sum, value, index) => sum + [0.2126, 0.7152, 0.0722][index]
          * (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4), 0);
      };
      const [low, high] = [luminance(values.color), luminance(values.background)].sort((a, b) => a - b);
      return (high + 0.05) / (low + 0.05);
    };
    const colors = element => ({
      color: getComputedStyle(element).color,
      background: getComputedStyle(element).backgroundColor,
    });
    await page.locator(".skip").focus();
    assert.ok(contrast(await page.locator(".skip").evaluate(colors)) >= 4.5, "skip link contrast");
    await page.locator(".button").first().hover();
    assert.ok(contrast(await page.locator(".button").first().evaluate(colors)) >= 4.5, "button hover contrast");
  };

  for (const colorScheme of ["light", "dark"]) {
    for (const width of [1440, 768, 390, 320]) {
      const context = await browser.newContext({ colorScheme, viewport: { width, height: 1000 } });
      // Full-editor links are tested as destinations, not additional runtime owners.
      await context.route("**/playground.html?**", route => route.fulfill({
        contentType: "text/html", body: "<!doctype html><title>Layout test placeholder</title>",
      }));
      try {
        const page = await context.newPage();
        const errors = [], missing = [];
        page.on("pageerror", error => errors.push(String(error)));
        page.on("response", response => {
          if (response.status() >= 400 && new URL(response.url()).origin === root.origin) missing.push(response.url());
        });
        await page.goto(base);
        await page.waitForFunction(() => { return document.documentElement.dataset.theme && document.querySelector("[data-theme-choice]"); });
        await validateContrast(page);

        for (const id of [...pages, "python-Scene", "search"]) {
          const url = new URL(`site/docs/${id}.html`, root);
          await page.goto(url.href);
          await page.waitForFunction(() => { return document.documentElement.dataset.theme && document.querySelector("[data-theme-choice]"); });
          assert.equal(await page.locator("h1").count(), 1, `${id}: one title`);
          assert.equal(await page.locator("iframe").count(), 0, `${id}: no runtime context before activation`);
          assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true,
            `${id}: ${colorScheme}/${width} horizontal overflow`);
          if (id !== "search") assert.equal(await page.locator('.docs-sidebar a[aria-current="page"]').getAttribute("href"), `${id === "python-Scene" ? "api" : id}.html`);
          for (const destination of pages) {
            assert.equal(await page.locator(`.docs-sidebar a[href="${destination}.html"]`).count(), 1);
          }
          // Validate local link destinations and anchors in the staged bundle,
          // not just substring matches in the source templates.
          if (width === 1440 && colorScheme === "light") {
            const links = await page.locator("a[href], [src], link[rel=stylesheet]").evaluateAll(elements =>
              elements.map(element => element.getAttribute("href") ?? element.getAttribute("src")));
            for (const href of new Set(links)) {
              const target = new URL(href, url);
              if (target.origin !== root.origin) continue;
              if (target.pathname.endsWith("/")) target.pathname += "index.html";
              const response = await page.request.get(target.href);
              assert.equal(response.ok(), true, `${id}: ${href}`);
              if (target.hash && target.pathname.endsWith(".html")) {
                const markup = await response.text();
                const anchor = decodeURIComponent(target.hash.slice(1));
                assert.ok(markup.includes(`id="${anchor}"`) || markup.includes(`id='${anchor}'`), `${id}: missing ${href}`);
              }
            }
          }
          if (width === 1440 || width === 390) {
            await page.screenshot({ path: path.join(output, `docs-${id}-${colorScheme}-${width}.png`), fullPage: false });
          }
        }

        const opposite = colorScheme === "dark" ? "light" : "dark";
        await page.locator("[data-theme-choice]").selectOption(opposite);
        assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), opposite);
        const queryUrl = new URL("site/docs/getting-started.html?q=Scene#browser", root);
        await page.goto(queryUrl.href);
        await page.waitForFunction(theme => document.documentElement.dataset.theme === theme, opposite);
        assert.equal(page.url(), queryUrl.href, "docs query must not redirect to the playground");
        await page.reload();
        await page.waitForFunction(theme => document.documentElement.dataset.theme === theme, opposite);
        await page.goto(base);
        await page.waitForFunction(theme => document.documentElement.dataset.theme === theme, opposite);
        await validateContrast(page);
        await page.goto(new URL("site/docs/search.html?q=Scene", root).href);
        assert.ok(await page.locator("[data-search-item]:visible").count() > 0, "query finds topics and symbols");
        assert.ok(await page.locator("[data-search-item][hidden]").count() > 0, "query filters the index");
        await page.locator("[data-theme-choice]").selectOption("system");
        assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), colorScheme);
        assert.deepEqual(errors, [], `${colorScheme}/${width}: page errors`);
        assert.deepEqual(missing, [], `${colorScheme}/${width}: missing resources`);
      } finally {
        await context.close();
      }
    }
  }

  const noJs = await browser.newContext({ javaScriptEnabled: false });
  await noJs.route("**/playground.html?**", route => route.fulfill({
    contentType: "text/html", body: "<!doctype html><title>Layout test placeholder</title>",
  }));
  try {
    const page = await noJs.newPage();
    for (const id of pages) {
      await page.goto(new URL(`site/docs/${id}.html`, root).href);
      assert.equal(await page.locator("h1").isVisible(), true);
      assert.equal(await page.locator(".docs-sidebar a").count(), pages.length);
    }
  } finally {
    await noJs.close();
  }
  await qualifyPreviewShell(browser, base, output);
  console.log(`Documentation: ${pages.length} routes, four widths, system/manual themes, links, and no-JS navigation passed. Preview-shell tests are layout/lifecycle only.`);
}

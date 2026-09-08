from pathlib import Path
p=Path('scripts/playground-main-thread-mobile-smoke.mjs')
s=p.read_text()
s=s.replace('const intermediate = await page.locator("#scene").screenshot();','const intermediate = await page.locator("#scene").screenshot();\n    await writeFile(path.join(artifactDir, `${variant.name}-intermediate.png`), intermediate);\n    await writeFile(path.join(artifactDir, `${variant.name}-capture.json`), JSON.stringify(await page.evaluate(async()=>({metrics:await window.__noonExampleGallery.executionMetrics(), rect:document.querySelector("#scene").getBoundingClientRect().toJSON(), style:{border:getComputedStyle(document.querySelector("#scene")).border, background:getComputedStyle(document.querySelector("#scene")).background}, viewport:{width:innerWidth,height:innerHeight,scale:visualViewport.scale,offsetLeft:visualViewport.offsetLeft,offsetTop:visualViewport.offsetTop}})), (_,v)=>typeof v === "bigint" ? String(v) : v, 2));')
s=s.replace('const final = await page.locator("#scene").screenshot();','const final = await page.locator("#scene").screenshot();\n    await writeFile(path.join(artifactDir, `${variant.name}-final.png`), final);')
p.write_text(s)

"""Add observations to the retained navigation test; keep its assertions intact."""
from pathlib import Path
import sys

source = Path(sys.argv[1])
text = source.read_text()
anchor = "  const page = await context.newPage();"
assert text.count(anchor) == 1
observations = """
  const navigationRequests = new Map();
  const navigationEvents = [];
  page.on("request", request => navigationRequests.set(request, {
    url: request.url(), type: request.resourceType(), startedAt: performance.now(), status: "pending"
  }));
  page.on("requestfinished", request => {
    const record = navigationRequests.get(request);
    if (record) Object.assign(record, {status: "finished", completedAt: performance.now()});
  });
  page.on("requestfailed", request => {
    const record = navigationRequests.get(request);
    if (record) Object.assign(record, {status: "failed", completedAt: performance.now(), failure: request.failure()});
  });
  page.on("domcontentloaded", () => navigationEvents.push({event: "domcontentloaded", at: performance.now()}));
  page.on("load", () => navigationEvents.push({event: "load", at: performance.now()}));
"""
text = text.replace(anchor, anchor + observations)
navigation = """  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });"""
assert text.count(navigation) == 1
wrapped = """  try {
    await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
      waitUntil: "load",
    });
  } finally {
    await writeFile(path.join(artifactDir, "navigation-trace.json"), JSON.stringify({
      browser: browserName, profile: profileName, browserVersion: browser.version(),
      events: navigationEvents, requests: [...navigationRequests.values()], pageErrors, consoleErrors,
      url: page.url(), serverOutput
    }, null, 2));
  }"""
text = text.replace(navigation, wrapped)
source.write_text(text)

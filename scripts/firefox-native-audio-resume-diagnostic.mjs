import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { playgroundLaunchOptions } from "./playground-browser-support.mjs";
import { serveRepository } from "./browser-test-server.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_FIREFOX_AUDIO_PORT ?? "4198");
assert.ok(Number.isSafeInteger(port) && port >= 1024 && port <= 65535, "invalid diagnostic port");
const artifactDir = path.resolve(process.env.NOON_FIREFOX_AUDIO_ARTIFACTS ??
  path.join(repoRoot, "browser-smoke-artifacts", "firefox-audio-native"));
const runtimeAssetIdentity = JSON.parse(await readFile(path.join(repoRoot, "web/runtime-build-identity.json"), "utf8"));
const probeUrl = `http://127.0.0.1:${port}/firefox-native-audio-probe.html`;
const probeHtml = `<!doctype html><meta charset="utf-8"><title>Native Web Audio probe</title>
  <button id="resume" type="button">Test native resume</button>`;
const variants = [
  { name: "context-before-gesture", createBeforeGesture: true, startSilentSource: false },
  { name: "context-in-gesture", createBeforeGesture: false, startSilentSource: false },
  { name: "silent-source-before-resume", createBeforeGesture: true, startSilentSource: true },
];

await mkdir(artifactDir, { recursive: true });
let server;
let browser;
let browserVersion = null;
let failure = null;
const results = [];
try {
  server = await serveRepository(repoRoot, port);
  browser = await playwright.firefox.launch(playgroundLaunchOptions("firefox"));
  browserVersion = browser.version();
  for (const variant of variants) {
    const context = await browser.newContext();
    await context.route(probeUrl, route => route.fulfill({
      status: 200, contentType: "text/html", body: probeHtml,
    }));
    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    await page.goto(probeUrl, { waitUntil: "load" });
    await page.evaluate(({ createBeforeGesture, startSilentSource }) => {
      const events = [];
      const log = event => events.push({ at: performance.now(), epochMs: Date.now(),
        timeOrigin: performance.timeOrigin, ...event });
      const activation = () => ({ isActive: navigator.userActivation?.isActive ?? null,
        hasBeenActive: navigator.userActivation?.hasBeenActive ?? null });
      let audio = null;
      const observe = context => {
        audio = context;
        window.__nativeAudioContext = context;
        window.__nativeAudioEvents = events;
        const gain = context.createGain();
        gain.gain.value = 1;
        gain.connect(context.destination);
        window.__nativeAudioGain = gain;
        log({ type: "context-and-master-gain-created", state: context.state, sampleRate: context.sampleRate,
          baseLatency: context.baseLatency ?? null, outputLatency: context.outputLatency ?? null });
        context.addEventListener("statechange", () => log({ type: "statechange", state: context.state,
          currentTime: context.currentTime }));
      };
      if (createBeforeGesture) observe(new AudioContext());
      let finish;
      window.__nativeAudioResult = new Promise(resolve => { finish = resolve; });
      document.querySelector("#resume").addEventListener("click", event => {
        log({ type: "trusted-click", isTrusted: event.isTrusted, activation: activation() });
        if (audio === null) observe(new AudioContext());
        if (startSilentSource) {
          const source = audio.createBufferSource();
          source.buffer = audio.createBuffer(1, 1, audio.sampleRate);
          source.connect(window.__nativeAudioGain);
          source.start();
          log({ type: "silent-source-started", state: audio.state, currentTime: audio.currentTime });
        }
        log({ type: "resume-call", stateBefore: audio.state, currentTime: audio.currentTime,
          activation: activation() });
        let resumePromise;
        try {
          resumePromise = audio.resume();
          log({ type: "resume-returned", isPromise: resumePromise instanceof Promise,
            stateAfterCall: audio.state, currentTime: audio.currentTime });
          resumePromise.then(
            () => log({ type: "resume-fulfilled", state: audio.state, currentTime: audio.currentTime }),
            error => log({ type: "resume-rejected", state: audio.state, currentTime: audio.currentTime,
              error: { name: error?.name ?? null, message: error?.message ?? String(error) } }),
          );
        } catch (error) {
          log({ type: "resume-threw", error: { name: error?.name ?? null, message: error?.message ?? String(error) } });
        }
        setTimeout(() => finish({
          secureContext: isSecureContext,
          final: { state: audio.state, currentTime: audio.currentTime, sampleRate: audio.sampleRate,
            baseLatency: audio.baseLatency ?? null, outputLatency: audio.outputLatency ?? null },
          resumeSettled: events.some(item => item.type === "resume-fulfilled" || item.type === "resume-rejected"),
          events: events.map(item => ({ ...item })),
        }), 2000);
      }, { once: true });
    }, { createBeforeGesture: variant.createBeforeGesture, startSilentSource: variant.startSilentSource });
    await page.locator("#resume").click();
    const observation = await page.evaluate(() => window.__nativeAudioResult);
    const cleanup = await page.evaluate(async () => {
      const audio = window.__nativeAudioContext;
      if (!audio) return { state: "unavailable", settled: false };
      let closePromise;
      try { closePromise = audio.close(); }
      catch (error) { return { state: audio.state, settled: false, error: String(error) }; }
      const status = await Promise.race([
        closePromise.then(() => "fulfilled", error => `rejected: ${String(error)}`),
        new Promise(resolve => setTimeout(() => resolve("pending after 500ms"), 500)),
      ]);
      return { status, state: audio.state,
        events: window.__nativeAudioEvents.map(event => ({ ...event })) };
    });
    results.push({ ...variant, observation, cleanup });
    await page.close();
    await context.close();
  }
} catch (error) {
  failure = { name: error?.name ?? "Error", message: error?.message ?? String(error),
    stack: typeof error?.stack === "string" ? error.stack : null };
  throw error;
} finally {
  try { await browser?.close(); } catch (error) { failure ??= { cleanup: String(error) }; }
  try { await server?.close(); } catch (error) { failure ??= { cleanup: String(error) }; }
  const report = { status: failure ? "failed" : results.length === variants.length ? "complete" : "partial",
    nodeVersion: process.version, browser: "firefox", browserVersion, port, runtimeAssetIdentity,
    probe: "native AudioContext plus a 1x gain master; no Noon modules or playback worker",
    variants: results, ...(failure === null ? {} : { error: failure }) };
  await writeFile(path.join(artifactDir, "native-audio-resume.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify(report, null, 2));
}

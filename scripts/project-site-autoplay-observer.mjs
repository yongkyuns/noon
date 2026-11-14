// Test-only passive observation, installed before iframe scripts on every
// browser. Observe insertion and attributes, with a read-only fallback poll:
// Firefox can coalesce attribute mutations while an expensive WASM startup
// occupies its event loop. The source renderer/playback remain untouched.
export function observeUnpromptedPlayback(win = window, doc = document, Observer = MutationObserver) {
  win.__noonSitePlayingObserved = false;
  let retired = false;
  let timer = null;
  let observer = null;
  const retire = () => {
    if (retired) return;
    retired = true;
    observer?.disconnect();
    if (timer !== null) win.clearInterval(timer);
  };
  const sample = () => {
    if (retired) return;
    if (doc.querySelector('#status')?.dataset.playbackPlaying === 'true') {
      win.__noonSitePlayingObserved = true;
      retire();
    }
  };
  observer = new Observer(sample);
  timer = win.setInterval(sample, 100);
  observer.observe(doc, {
    subtree: true, childList: true, attributes: true,
    attributeFilter: ['data-playback-playing'],
  });
  win.addEventListener('pagehide', retire, {once: true});
  sample();
}

// Stronger no-gesture autoplay proof than a historical MutationObserver flag:
// the same real scene generation must advance both presented frame count and
// playback time while its canonical controls say it is actively playing.
// Playwright serializes this closure-free predicate into each real iframe.
export function observedUnpromptedProgress(start, win = window, doc = document) {
  const gallery = win.__noonExampleGallery;
  const status = doc.querySelector('#status');
  const frames = Number(status?.dataset?.presentedFrames);
  const time = Number.parseFloat(doc.querySelector('#metric-time')?.value);
  return gallery?.generationDiagnostics?.runGeneration === start.generation &&
    gallery.runInFlight === false && status?.dataset?.playbackPlaying === 'true' &&
    Number.isFinite(frames) && Number.isFinite(time) &&
    frames > start.frames && Math.abs(time - start.time) > 0.01;
}

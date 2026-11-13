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

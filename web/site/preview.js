// Documentation-level resource ownership only. Playback semantics stay in main.js
// and the canonical WASM runtime. Exactly one iframe can be active per page.
export function installPreviews(documentLike = document, windowLike = window) {
  let active = null;
  const cleanups = [];
  const setStatus = (card, text) => { card.querySelector('[data-demo-status]').textContent = text; };
  const close = (reason = 'Preview closed. Open it to start again.', returnFocus = false) => {
    if (!active) return;
    const { card, frame, timer } = active;
    active = null;
    windowLike.clearTimeout(timer);
    // Retiring the browsing context invokes the canonical playground pagehide
    // cleanup; detached contexts cannot remain hidden background previews.
    frame.remove();
    card.querySelector('[data-demo-close]').hidden = true;
    card.querySelector('[data-demo-open]').hidden = false;
    card.querySelector('.demo-poster').hidden = false;
    card.removeAttribute('data-preview-active');
    setStatus(card, reason);
    if (returnFocus) card.querySelector('[data-demo-open]').focus();
  };
  const onMessage = event => {
    if (!active || event.origin !== windowLike.location.origin || event.source !== active.frame.contentWindow) return;
    if (event.data?.type !== 'noon-docs-preview-shell-ready') return;
    windowLike.clearTimeout(active.timer);
    setStatus(active.card, 'Preview opened. Press Run to execute this scene.');
    active.frame.dataset.shellReady = 'true';
  };
  for (const card of documentLike.querySelectorAll('[data-demo]')) {
    const openButton = card.querySelector('[data-demo-open]');
    const closeButton = card.querySelector('[data-demo-close]');
    const open = () => {
      close('Closed because another preview was opened.');
      const url = new URL(card.dataset.frameSrc, windowLike.location.href);
      const siteRoot = new URL('../../', windowLike.location.href);
      const expectedPath = new URL('embed.html', siteRoot).pathname;
      if (url.origin !== windowLike.location.origin || url.pathname !== expectedPath ||
          url.searchParams.get('example') !== card.dataset.demo) {
        setStatus(card, 'This preview URL is not valid. Use the full editor link.');
        return;
      }
      const frame = documentLike.createElement('iframe');
      frame.title = `${card.dataset.demo} — interactive Noon preview`;
      frame.referrerPolicy = 'same-origin';
      frame.setAttribute('allow', 'fullscreen');
      frame.src = url.href;
      frame.addEventListener('error', () => {
        if (active?.frame === frame) close('The preview could not load. Retry or open the full editor.');
      });
      openButton.hidden = true;
      closeButton.hidden = false;
      card.querySelector('.demo-poster').hidden = true;
      card.dataset.previewActive = 'true';
      setStatus(card, 'Opening the preview interface. The runtime starts only after Run.');
      const timer = windowLike.setTimeout(() => {
        if (active?.frame === frame && frame.dataset.shellReady !== 'true') {
          close('The preview interface did not respond. Retry or open the full editor.');
        }
      }, 30000);
      active = { card, frame, timer };
      card.querySelector('[data-demo-frame]').append(frame);
      closeButton.focus();
    };
    const closeSelected = () => { if (active?.card === card) close(undefined, true); };
    openButton.addEventListener('click', open);
    closeButton.addEventListener('click', closeSelected);
    cleanups.push(() => { openButton.removeEventListener('click', open); closeButton.removeEventListener('click', closeSelected); });
  }
  const visibility = () => { if (documentLike.hidden) close('Preview released while the page was hidden. Reopen to start again.'); };
  const pagehide = () => close('Preview released during navigation.');
  windowLike.addEventListener('message', onMessage);
  windowLike.addEventListener('pagehide', pagehide);
  documentLike.addEventListener('visibilitychange', visibility);
  return () => {
    close(); cleanups.forEach(fn => fn());
    windowLike.removeEventListener('message', onMessage);
    windowLike.removeEventListener('pagehide', pagehide);
    documentLike.removeEventListener('visibilitychange', visibility);
  };
}

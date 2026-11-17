import { installEmbeddedRuntimeBuild } from '../runtime-build-verifier.js';

const touchSceneLimit = 2;

export function installPreviews(documentLike = document, windowLike = window,
  embedUrl = new URL('../embed.html', import.meta.url)) {
  const records = new Set();
  const playOnDemand = windowLike.matchMedia('(pointer: coarse)').matches;
  const releaseRuntimeBuild = installEmbeddedRuntimeBuild(documentLike);
  let disposed = false;
  const unmount = record => {
    windowLike.clearTimeout(record.timer);
    record.frame?.remove();
    record.frame = null;
    if (record.closeButton) record.closeButton.hidden = true;
  };
  const liveCount = () => [...records].filter(record => record.frame !== null).length;
  const showPreview = record => {
    record.waitingForSlot = false;
    record.card.dataset.liveState = 'idle';
    record.card.dataset.embedView = 'view';
    record.thumbnail.hidden = false;
    record.status.hidden = true;
    record.retry.textContent = 'Play scene';
    record.retry.hidden = false;
  };
  const releaseSlot = record => {
    unmount(record);
    if (liveCount() === 0) releaseRuntimeBuild();
    for (const waiting of records) {
      if (waiting.waitingForSlot) showPreview(waiting);
    }
  };
  const fail = (record, text) => {
    releaseSlot(record);
    if (record.thumbnail) record.thumbnail.hidden = false;
    record.card.dataset.liveState = 'error';
    record.status.textContent = text;
    record.status.hidden = false;
    record.retry.textContent = 'Retry';
    record.retry.hidden = false;
  };
  const mount = record => {
    if (disposed || record.frame !== null) return;
    if (playOnDemand && liveCount() >= touchSceneLimit) {
      record.waitingForSlot = true;
      record.status.textContent = 'Close a scene to play another.';
      record.status.hidden = false;
      return;
    }
    let url;
    try { url = new URL(record.card.dataset.frameSrc, windowLike.location.href); }
    catch { fail(record, 'Invalid scene address.'); return; }
    if (url.origin !== windowLike.location.origin || url.pathname !== embedUrl.pathname ||
        url.searchParams.get('example') !== record.card.dataset.demo ||
        url.searchParams.get('catalog') !== 'showcase') {
      fail(record, 'Invalid scene address.');
      return;
    }
    if (playOnDemand) {
      record.waitingForSlot = false;
      record.thumbnail.hidden = true;
      record.closeButton.hidden = false;
    }
    const frame = documentLike.createElement('iframe');
    frame.title = `${record.card.dataset.demo} — live scene and editable code`;
    frame.referrerPolicy = 'same-origin';
    frame.loading = 'eager';
    frame.setAttribute('allow', 'fullscreen');
    frame.src = url.href;
    frame.addEventListener('error', () => {
      if (record.frame === frame) fail(record, 'The live scene could not load.');
    });
    record.frame = frame;
    record.card.dataset.liveState = 'loading';
    record.card.dataset.embedView = 'view';
    record.retry.hidden = true;
    record.status.hidden = false;
    record.status.textContent = 'Loading live scene…';
    record.timer = windowLike.setTimeout(() => {
      if (record.frame === frame && frame.dataset.shellReady !== 'true') {
        fail(record, 'The live scene did not respond.');
      }
    }, 30000);
    record.surface.append(frame);
    if (playOnDemand && documentLike.activeElement === record.retry) record.closeButton.focus();
  };
  const onMessage = event => {
    if (event.origin !== windowLike.location.origin || event.data?.type !== 'noon-site-embed-state') return;
    const record = [...records].find(item => item.frame?.contentWindow === event.source);
    if (!record || !['view', 'code'].includes(event.data.view) ||
        !['loading', 'ready', 'error'].includes(event.data.state)) return;
    windowLike.clearTimeout(record.timer);
    record.frame.dataset.shellReady = 'true';
    record.card.dataset.embedView = event.data.view;
    record.card.dataset.liveState = event.data.state;
    // The child shows runtime progress/errors, including editable Python errors.
    // The parent's message is only for a failure to load the child document.
    record.status.hidden = true;
    record.retry.hidden = true;
  };
  const dispose = () => {
    disposed = true;
    for (const record of records) {
      unmount(record);
      record.retry.removeEventListener('click', record.onRetry);
      record.closeButton?.removeEventListener('click', record.onClose);
    }
    records.clear();
    releaseRuntimeBuild();
    windowLike.removeEventListener('message', onMessage);
    windowLike.removeEventListener('pagehide', onPageHide);
  };
  const onPageHide = event => {
    // Preserve drafts and the editor DOM through back/forward cache. The child
    // gallery retires its workers on pagehide and resumes on pageshow.
    if (event.persisted) releaseRuntimeBuild();
    else dispose();
  };
  windowLike.addEventListener('message', onMessage);
  windowLike.addEventListener('pagehide', onPageHide);
  for (const card of documentLike.querySelectorAll('[data-demo]')) {
    const record = { card, frame: null, timer: null, closeButton: null, waitingForSlot: false,
      surface: card.querySelector('[data-demo-frame]'), thumbnail: null,
      status: card.querySelector('[data-demo-status]'), retry: card.querySelector('[data-demo-retry]') };
    record.onRetry = () => mount(record);
    record.retry.addEventListener('click', record.onRetry);
    records.add(record);
    if (playOnDemand) {
      const thumbnail = documentLike.createElement('img');
      thumbnail.alt = '';
      thumbnail.loading = 'lazy';
      thumbnail.src = new URL(`../thumbnails/showcase/${encodeURIComponent(card.dataset.demo)}.png`, import.meta.url).href;
      record.thumbnail = thumbnail;
      const closeButton = documentLike.createElement('button');
      closeButton.type = 'button';
      closeButton.className = 'demo-close';
      closeButton.textContent = 'Close';
      closeButton.setAttribute('aria-label', 'Close scene');
      closeButton.hidden = true;
      record.closeButton = closeButton;
      record.onClose = () => {
        releaseSlot(record);
        showPreview(record);
        record.retry.focus();
      };
      closeButton.addEventListener('click', record.onClose);
      record.surface.append(thumbnail);
      card.append(closeButton);
      showPreview(record);
    } else {
      mount(record);
    }
  }
  return dispose;
}

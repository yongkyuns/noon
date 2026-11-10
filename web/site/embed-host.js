// Presentation-only adapter over the unchanged canonical playground DOM/runtime.
// No renderer, authoring clock, player, or worker is created in this file.
(() => {
  document.documentElement.dataset.noonEmbed = 'true';
  const preview = document.querySelector('.preview-pane');
  const header = document.querySelector('.preview-head');
  const run = document.querySelector('#replace-scene');
  const status = document.querySelector('#status');
  const diagnostics = document.querySelector('#python-diagnostics');
  if (!preview || !header || !run || !status || !diagnostics) {
    throw new Error('The canonical playground shell changed: update and qualify the embed adapter');
  }
  // Retain required hidden elements for main.js, but surface only the controls
  // needed by this preview. The real player still supplies playback/seek UI.
  header.prepend(run);
  preview.insertBefore(status, header.nextSibling);
  preview.append(diagnostics);
  document.querySelector('.editor-pane').setAttribute('aria-hidden', 'true');
  const ready = () => {
    if (run.disabled) return;
    observer.disconnect();
    window.parent.postMessage({ type: 'noon-docs-preview-shell-ready' }, window.location.origin);
  };
  const observer = new MutationObserver(ready);
  observer.observe(run, { attributes: true, attributeFilter: ['disabled'] });
  window.addEventListener('pagehide', () => observer.disconnect(), { once: true });
  ready();
})();

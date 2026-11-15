// Presentation only. Retain the gallery's editor bootstrap, initial live run,
// debounced/IME-safe edits, failure recovery and worker teardown unchanged.
export function installEmbedHost(documentLike = document, windowLike = window) {
  const root = documentLike.documentElement;
  const workspace = documentLike.querySelector('.workspace');
  const view = documentLike.querySelector('.preview-pane');
  const code = documentLike.querySelector('.editor-pane');
  const run = documentLike.querySelector('#replace-scene');
  const autoRun = documentLike.querySelector('#auto-run');
  const status = documentLike.querySelector('#status');
  const patch = documentLike.querySelector('#patch-status');
  const recover = documentLike.querySelector('#preview-recover');
  if (![workspace, view, code, run, autoRun, status, patch, recover].every(Boolean)) {
    throw new Error('Canonical playground shell changed: requalify the live embed');
  }
  root.dataset.noonEmbed = 'true';
  root.dataset.embedView = 'view';
  root.dataset.embedState = 'loading';
  // Set the existing preference before the gallery/initial run initializes.
  // There is no second input listener, debounce timer, source buffer or runner.
  autoRun.checked = true;

  const tabs = documentLike.createElement('div');
  tabs.className = 'embed-tabs';
  tabs.setAttribute('role', 'tablist');
  tabs.setAttribute('aria-label', 'Scene display');
  const buttons = ['view', 'code'].map((name, index) => {
    const button = documentLike.createElement('button');
    button.type = 'button';
    button.id = `embed-${name}-tab`;
    button.textContent = name === 'view' ? 'View' : 'Code';
    button.setAttribute('role', 'tab');
    button.setAttribute('aria-controls', `embed-${name}-panel`);
    button.addEventListener('click', () => select(name));
    button.addEventListener('keydown', event => {
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? 1 :
        ['ArrowRight', 'ArrowLeft'].includes(event.key) ? 1 - index : null;
      if (next === null) return;
      event.preventDefault();
      select(next === 0 ? 'view' : 'code');
      buttons[next].focus();
    });
    tabs.append(button);
    return button;
  });
  for (const [name, panel] of [['view', view], ['code', code]]) {
    panel.id = `embed-${name}-panel`;
    panel.setAttribute('role', 'tabpanel');
    panel.setAttribute('aria-labelledby', `embed-${name}-tab`);
  }
  // Keep pause/restart/seek accessible without showing the full toolbar.
  // Relay keyboard input to the existing playback control owner, not a new clock.
  view.tabIndex = 0;
  view.setAttribute('aria-keyshortcuts', 'Space Home ArrowLeft ArrowRight');
  view.setAttribute('aria-description', 'Live scene. Space pauses or plays; Home restarts when supported.');
  const relayKeyboard = event => {
    if (event.target !== view || !['Space','Home','ArrowLeft','ArrowRight'].includes(event.code)) return;
    const controls = view.querySelector('.playback-controls');
    if (!controls) return;
    const forwarded = new windowLike.KeyboardEvent(event.type, {key:event.key,code:event.code,
      repeat:event.repeat,isComposing:event.isComposing,altKey:event.altKey,ctrlKey:event.ctrlKey,
      metaKey:event.metaKey,shiftKey:event.shiftKey,bubbles:true,cancelable:true});
    controls.dispatchEvent(forwarded);
    event.stopPropagation();
    if (forwarded.defaultPrevented) event.preventDefault();
  };
  view.addEventListener('keydown', relayKeyboard);
  view.addEventListener('keyup', relayKeyboard);
  const message = documentLike.createElement('div');
  message.className = 'embed-message';
  message.setAttribute('role', 'status');
  const text = documentLike.createElement('span');
  const retry = documentLike.createElement('button');
  retry.type = 'button'; retry.textContent = 'Retry'; retry.hidden = true;
  retry.addEventListener('click', () => {
    if (!recover.hidden && !recover.disabled) recover.click();
    else if (!run.disabled) run.click();
    else windowLike.location.reload();
  });
  message.append(text, retry);
  workspace.append(tabs, message);
  const publish = () => windowLike.parent.postMessage({type: 'noon-site-embed-state',
    view: root.dataset.embedView, state: root.dataset.embedState}, windowLike.location.origin);
  function select(name) {
    root.dataset.embedView = name;
    for (const [index, panel] of [view, code].entries()) {
      const selected = (index === 0 ? 'view' : 'code') === name;
      panel.inert = !selected;
      panel.setAttribute('aria-hidden', String(!selected));
      buttons[index].tabIndex = selected ? 0 : -1;
      buttons[index].setAttribute('aria-selected', String(selected));
    }
    // Both panels and their source/runtime stay mounted. Changing tabs is not a Run.
    publish();
  }
  const update = () => {
    const presented = status.dataset.playbackPlaying === 'true' || patch.dataset.state === 'applied';
    const failed = patch.dataset.state === 'error' || status.dataset.state === 'error' ||
      (!presented && status.dataset.liveAuthoring === 'error');
    const state = failed ? 'error' : presented ? 'ready' : 'loading';
    root.dataset.embedState = state;
    message.hidden = state === 'ready';
    retry.hidden = !failed;
    // Full diagnostics remain in the canonical editor, not a second error model.
    text.textContent = failed ? 'Scene could not run. Open Code for details.' : 'Loading live scene…';
    publish();
  };
  let restorePending = false;
  const resume = () => {
    if (!restorePending || run.disabled) return;
    restorePending = false;
    run.click();
  };
  const observer = new windowLike.MutationObserver(() => { update(); resume(); });
  for (const element of [status, patch]) observer.observe(element, { attributes: true,
    attributeFilter: ['data-state', 'data-live-authoring', 'data-playback-playing'] });
  observer.observe(run, { attributes: true, attributeFilter: ['disabled'] });
  // main.js restores a deferred host after BFCache; reuse its ordinary Run action
  // after all pageshow listeners finish. This does not recreate/reset the editor.
  const restored = event => {
    if (!event.persisted) return;
    restorePending = true;
    windowLike.queueMicrotask(resume);
  };
  windowLike.addEventListener('pageshow', restored);
  const retire = event => {
    restorePending = false;
    if (!event.persisted) { observer.disconnect(); windowLike.removeEventListener('pageshow', restored); }
  };
  windowLike.addEventListener('pagehide', retire);
  select('view');
  update();
}
if (typeof document !== 'undefined') installEmbedHost();

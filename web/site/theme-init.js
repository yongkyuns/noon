// Classic, blocking head script: apply the saved preference before first paint.
// The CSS media query remains the no-JavaScript/system fallback.
(() => {
  let preference = 'system';
  try {
    const saved = localStorage.getItem('noon-site-theme');
    if (saved === 'light' || saved === 'dark') preference = saved;
  } catch { /* Storage may be unavailable in a private/embedded context. */ }
  const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
  document.documentElement.dataset.themePreference = preference;
  document.documentElement.dataset.theme = preference === 'system' ? (dark ? 'dark' : 'light') : preference;
})();

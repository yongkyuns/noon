export function effectiveTheme(preference, systemDark) {
  return preference === 'light' || preference === 'dark' ? preference : (systemDark ? 'dark' : 'light');
}

export function installTheme(documentLike = document, windowLike = window) {
  const root = documentLike.documentElement;
  const media = windowLike.matchMedia('(prefers-color-scheme: dark)');
  let preference = 'system';
  const read = () => {
    try {
      const value = windowLike.localStorage.getItem('noon-site-theme');
      return value === 'light' || value === 'dark' ? value : 'system';
    } catch { return 'system'; }
  };
  const apply = () => {
    root.dataset.themePreference = preference;
    root.dataset.theme = effectiveTheme(preference, media.matches);
    documentLike.querySelectorAll('[data-theme-choice]').forEach(select => { select.value = preference; });
  };
  preference = read();
  apply();
  const removals = [];
  for (const select of documentLike.querySelectorAll('[data-theme-choice]')) {
    const change = () => {
      preference = ['light', 'dark'].includes(select.value) ? select.value : 'system';
      try {
        if (preference === 'system') windowLike.localStorage.removeItem('noon-site-theme');
        else windowLike.localStorage.setItem('noon-site-theme', preference);
      } catch { /* Keep the choice for this document even without storage. */ }
      apply();
    };
    select.addEventListener('change', change);
    removals.push(() => select.removeEventListener('change', change));
  }
  const storage = event => {
    if (event.key === 'noon-site-theme' || event.key === null) { preference = read(); apply(); }
  };
  windowLike.addEventListener('storage', storage);
  media.addEventListener('change', apply);
  return () => {
    removals.forEach(remove => remove());
    windowLike.removeEventListener('storage', storage);
    media.removeEventListener('change', apply);
  };
}

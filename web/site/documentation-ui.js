// Progressive enhancements only. Links, forms, code and navigation work without JS.
export function searchScore(title, text, query) {
  const q = query.trim().toLocaleLowerCase();
  if (!q) return 1;
  const t = title.toLocaleLowerCase(), body = text.toLocaleLowerCase();
  if (!q.split(/\s+/).every(term => (t + ' ' + body).includes(term))) return 0;
  return 1 + (t === q ? 100 : t.startsWith(q) ? 50 : t.includes(q) ? 20 : 0)
    + q.split(/\s+/).filter(term => t.includes(term)).length * 5;
}

export function installDocumentationUI(doc = document, win = window) {
  const cleanup = [];
  const listen = (node, name, fn) => { node.addEventListener(name, fn); cleanup.push(() => node.removeEventListener(name, fn)); };
  const navigation = doc.querySelector('.docs-navigation');
  const desktop = win.matchMedia('(min-width: 901px)');
  if (navigation) {
    const adapt = () => { navigation.open = desktop.matches; };
    adapt(); listen(desktop, 'change', adapt);
    listen(navigation, 'keydown', event => {
      if (event.key === 'Escape' && !desktop.matches) { navigation.open = false; navigation.querySelector('summary').focus(); }
    });
  }
  listen(win, 'keydown', event => {
    if (!(event.key.toLowerCase() === 'k' && (event.ctrlKey || event.metaKey))) return;
    if (event.target.closest?.('input,textarea,[contenteditable=true]')) return;
    const field = doc.querySelector('#search-query') || doc.querySelector('#docs-search');
    if (!field) {
      const destination = doc.querySelector('[data-search-shortcut]');
      if (destination) { event.preventDefault(); win.location.assign(destination.href); }
      return;
    }
    event.preventDefault();
    if (navigation && field.id === 'docs-search') navigation.open = true;
    field.focus();
  });
  for (const button of doc.querySelectorAll('[data-copy-code]')) {
    button.hidden = false;
    listen(button, 'click', async () => {
      const block = button.closest('.code-block'), code = block.querySelector('code');
      const status = block.querySelector('[data-copy-status]');
      try {
        await win.navigator.clipboard.writeText(code.textContent);
        status.textContent = 'Code copied.'; button.textContent = 'Copied';
      } catch {
        const source = code.textContent;
        const range = doc.createRange(); range.selectNodeContents(code);
        const selection = win.getSelection(); selection.removeAllRanges(); selection.addRange(range);
        // Chromium omits a terminal newline when a selection ends inside <pre>.
        // Extend only a clipped selection to a nonselected boundary after <pre>.
        if (selection.toString() !== source && source.endsWith('\\n')) {
          let boundary = block.querySelector('[data-copy-selection-end]');
          if (!boundary) {
            boundary = doc.createElement('span');
            boundary.dataset.copySelectionEnd = '';
            boundary.setAttribute('aria-hidden', 'true');
            boundary.style.cssText = 'position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%)';
            boundary.textContent = '.';
            block.querySelector('pre').after(boundary);
          }
          range.setEndBefore(boundary);
          selection.removeAllRanges(); selection.addRange(range);
        }
        status.textContent = selection.toString() === source
          ? 'Clipboard unavailable. Code selected; use your copy shortcut.'
          : 'Clipboard unavailable. Select the code manually to copy.';
        button.textContent = 'Select to copy';
      }
    });
  }
  const query = new URL(win.location.href).searchParams.get('q') || '';
  const headerSearch = doc.querySelector('#docs-search');
  if (headerSearch) headerSearch.value = query;
  const entries = [...doc.querySelectorAll('[data-search-item]')];
  const input = doc.querySelector('#search-query');
  if (entries.length) {
    const status = doc.querySelector('[data-search-status]');
    const apply = value => {
      const ranked = entries.map((item,index) => ({item,index,score:searchScore(item.querySelector('a').textContent,item.dataset.searchText,value)}))
        .sort((a,b) => b.score-a.score || a.index-b.index);
      let count = 0;
      const fragment = doc.createDocumentFragment();
      for (const {item,score} of ranked) { item.hidden = score === 0; if(score) count++; fragment.append(item); }
      doc.querySelector('.topic-index').append(fragment);
      status.textContent = value.trim() ? (count ? `${count} matching topics and symbols.` : 'No matches. Try a class name such as Scene, or a task such as export.') : `${count} topics and symbols. Search to narrow the list.`;
    };
    if(input) {
      input.value = query;
      listen(input,'input',() => {
        apply(input.value);
        const url = new URL(win.location.href);
        if(input.value) url.searchParams.set('q',input.value); else url.searchParams.delete('q');
        win.history.replaceState(null,'',url);
      });
      listen(win,'popstate',()=>{input.value=new URL(win.location.href).searchParams.get('q')||'';apply(input.value);});
    }
    apply(query);
  }
  const apiInput = doc.querySelector('[data-api-filter]');
  if(apiInput) {
    const apply = () => {
      const value = apiInput.value.trim().toLocaleLowerCase();
      let count = 0;
      for(const row of doc.querySelectorAll('[data-api-entry]')) { row.hidden = !row.textContent.toLocaleLowerCase().includes(value); if(!row.hidden) count++; }
      doc.querySelector('[data-api-filter-status]').textContent = `${count} exported symbols shown.`;
    };
    listen(apiInput,'input',apply); apply();
  }
  // Scroll tracking changes only link emphasis, never focus, history, or scroll.
  if(win.IntersectionObserver) {
    const observer = new win.IntersectionObserver(changes => {
      const visible = changes.filter(entry=>entry.isIntersecting).sort((a,b)=>a.boundingClientRect.top-b.boundingClientRect.top)[0];
      if(!visible) return;
      for(const link of doc.querySelectorAll('.page-contents a,.contents-mobile a')) {
        if(link.hash === '#' + visible.target.id) link.setAttribute('aria-current','location');
        else link.removeAttribute('aria-current');
      }
    },{rootMargin:'-100px 0px -65% 0px'});
    for(const heading of doc.querySelectorAll('.docs-content h2[id],.docs-content h3[id]')) observer.observe(heading);
    cleanup.push(()=>observer.disconnect());
  }
  return () => cleanup.forEach(fn=>fn());
}

import { installTheme } from './theme.js';
import { installPreviews } from './preview.js';

installTheme();
installPreviews();
if (window.matchMedia("(max-width: 900px)").matches) {
  const navigation = document.querySelector(".docs-navigation");
  if (navigation) navigation.open = false;
}


const query = new URL(window.location.href).searchParams.get('q')?.trim().toLocaleLowerCase() ?? '';
const search = document.querySelector('#docs-search');
if (search) search.value = query;
const entries = [...document.querySelectorAll('[data-search-item]')];
if (entries.length) {
  const terms = query.split(/\s+/).filter(Boolean);
  let count = 0;
  for (const item of entries) {
    const text = item.dataset.searchText.toLocaleLowerCase();
    item.hidden = !terms.every(term => text.includes(term));
    if (!item.hidden) count++;
  }
  const status = document.querySelector('[data-search-status]');
  if (status) status.textContent = terms.length ? `${count} matching topics and symbols for “${query}”.` : `${count} topics and symbols.`;
}

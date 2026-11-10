import assert from 'node:assert/strict';
import test from 'node:test';
import { effectiveTheme, installTheme } from './site/theme.js';
import { installPreviews } from './site/preview.js';

class Element extends EventTarget {
  constructor() { super(); this.dataset = {}; this.hidden = false; this.children = []; this.attributes = new Map(); this.textContent = ''; this.focused = false; this.contentWindow = {}; }
  setAttribute(name, value) { this.attributes.set(name, value); }
  removeAttribute(name) { this.attributes.delete(name); if (name === 'data-preview-active') delete this.dataset.previewActive; }
  append(child) { this.children.push(child); child.parent = this; }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); this.removed = true; }
  focus() { this.focused = true; }
}
function click(element) { element.dispatchEvent(new Event('click')); }
function event(type, props) { const result = new Event(type); Object.assign(result, props); return result; }
function fixture(count = 4) {
  const doc = new EventTarget(); doc.hidden = false; doc.documentElement = new Element();
  const win = new EventTarget(); win.location = { href:'https://example.test/noon/site/docs/examples.html', origin:'https://example.test' };
  const timers = new Map(); let serial = 0;
  win.setTimeout = fn => { timers.set(++serial, fn); return serial; };
  win.clearTimeout = id => timers.delete(id);
  const cards = Array.from({length:count}, (_,i) => {
    const card = new Element(); card.dataset = { demo:`showcase-case-${i}`, frameSrc:`../../embed.html?catalog=showcase&example=showcase-case-${i}` };
    card.parts = Object.fromEntries(['[data-demo-open]','[data-demo-close]','[data-demo-status]','[data-demo-frame]','.demo-poster'].map(key => [key,new Element()]));
    card.querySelector = name => card.parts[name];
    card.parts['[data-demo-close]'].hidden = true;
    return card;
  });
  doc.querySelectorAll = selector => selector === '[data-demo]' ? cards : [];
  doc.createElement = tag => { assert.equal(tag,'iframe'); return new Element(); };
  return {doc,win,cards,timers,frames:() => cards.flatMap(card => card.parts['[data-demo-frame]'].children)};
}

test('preview requires an explicit action and bounds live contexts to one', () => {
  const f = fixture(); const dispose = installPreviews(f.doc,f.win);
  assert.equal(f.frames().length,0);
  for (const card of f.cards) {
    click(card.parts['[data-demo-open]']);
    assert.equal(f.frames().length,1);
    assert.equal(card.parts['.demo-poster'].hidden,true);
  }
  dispose(); assert.equal(f.frames().length,0); assert.equal(f.timers.size,0);
});

test('closing restores poster and keyboard focus', () => {
  const f = fixture(1); const dispose = installPreviews(f.doc,f.win); const card = f.cards[0];
  click(card.parts['[data-demo-open]']); click(card.parts['[data-demo-close]']);
  assert.equal(f.frames().length,0); assert.equal(card.parts['.demo-poster'].hidden,false);
  assert.equal(card.parts['[data-demo-open]'].focused,true); dispose();
});

test('hidden document and pagehide retire active contexts', () => {
  const f = fixture(1); const dispose = installPreviews(f.doc,f.win); const open = f.cards[0].parts['[data-demo-open]'];
  click(open); f.doc.hidden = true; f.doc.dispatchEvent(new Event('visibilitychange')); assert.equal(f.frames().length,0);
  f.doc.hidden = false; click(open); f.win.dispatchEvent(new Event('pagehide')); assert.equal(f.frames().length,0); dispose();
});

test('foreign, stale and wrong-origin messages cannot mark a preview ready', () => {
  const f = fixture(2); const dispose = installPreviews(f.doc,f.win);
  click(f.cards[0].parts['[data-demo-open]']); const old = f.frames()[0];
  click(f.cards[1].parts['[data-demo-open]']); const current = f.frames()[0];
  const data = {type:'noon-docs-preview-shell-ready'};
  f.win.dispatchEvent(event('message',{origin:f.win.location.origin,source:old.contentWindow,data}));
  f.win.dispatchEvent(event('message',{origin:'https://other.test',source:current.contentWindow,data}));
  assert.equal(current.dataset.shellReady,undefined);
  f.win.dispatchEvent(event('message',{origin:f.win.location.origin,source:current.contentWindow,data}));
  assert.equal(current.dataset.shellReady,'true'); assert.equal(f.timers.size,0); dispose();
});

test('failed or unresponsive preview returns to a retryable poster', () => {
  const f = fixture(1); const dispose = installPreviews(f.doc,f.win); const card = f.cards[0];
  click(card.parts['[data-demo-open]']); f.frames()[0].dispatchEvent(new Event('error'));
  assert.equal(f.frames().length,0); assert.match(card.parts['[data-demo-status]'].textContent,/could not load/);
  click(card.parts['[data-demo-open]']); [...f.timers.values()][0]();
  assert.equal(f.frames().length,0); assert.match(card.parts['[data-demo-status]'].textContent,/did not respond/); dispose();
});

test('preview rejects off-origin, mismatched, or noncanonical targets', () => {
  for (const src of ['https://other.test/embed.html?example=showcase-case-0','../../playground.html?example=showcase-case-0','../../embed.html?example=other']) {
    const f = fixture(1); f.cards[0].dataset.frameSrc = src; const dispose = installPreviews(f.doc,f.win);
    click(f.cards[0].parts['[data-demo-open]']); assert.equal(f.frames().length,0); dispose();
  }
});

function themeFixture(saved = null) {
  const doc = {documentElement:new Element()}; const picker = new Element(); picker.value = 'system';
  doc.querySelectorAll = () => [picker];
  const media = new EventTarget(); media.matches = true;
  const win = new EventTarget(); win.matchMedia = () => media;
  let value = saved;
  win.localStorage = {getItem:()=>value,setItem:(key,next)=>{value=next;},removeItem:()=>{value=null;}};
  return {doc,win,media,picker,value:()=>value};
}

test('theme supports system default and explicit light/dark', () => {
  assert.equal(effectiveTheme('system',true),'dark'); assert.equal(effectiveTheme('system',false),'light');
  assert.equal(effectiveTheme('light',true),'light'); assert.equal(effectiveTheme('dark',false),'dark');
});

test('theme persists, returns to system, and observes system changes', () => {
  const f = themeFixture('light'); const dispose = installTheme(f.doc,f.win);
  assert.equal(f.doc.documentElement.dataset.theme,'light');
  f.picker.value = 'dark'; f.picker.dispatchEvent(new Event('change')); assert.equal(f.value(),'dark');
  f.picker.value = 'system'; f.picker.dispatchEvent(new Event('change')); assert.equal(f.value(),null);
  f.media.matches = false; f.media.dispatchEvent(new Event('change')); assert.equal(f.doc.documentElement.dataset.theme,'light'); dispose();
});

test('theme synchronizes external changes without overriding explicit choice', () => {
  const f = themeFixture('light'); const dispose = installTheme(f.doc,f.win);
  f.media.dispatchEvent(new Event('change')); assert.equal(f.doc.documentElement.dataset.theme,'light');
  f.win.localStorage.setItem('noon-site-theme','dark');
  f.win.dispatchEvent(event('storage',{key:'noon-site-theme'})); assert.equal(f.picker.value,'dark'); dispose();
});

test('theme is usable when storage is denied', () => {
  const f = themeFixture(); f.win.localStorage = {getItem(){throw Error('denied')},setItem(){throw Error('denied')},removeItem(){throw Error('denied')}};
  const dispose = installTheme(f.doc,f.win); f.picker.value = 'light'; f.picker.dispatchEvent(new Event('change'));
  assert.equal(f.doc.documentElement.dataset.theme,'light'); dispose();
});

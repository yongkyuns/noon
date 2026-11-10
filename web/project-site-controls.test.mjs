import assert from 'node:assert/strict';
import test from 'node:test';
import { effectiveTheme, installTheme } from './site/theme.js';
import { installPreviews } from './site/preview.js';
import { installEmbedHost } from './site/embed-host.js';

class Element extends EventTarget {
  constructor() { super(); this.dataset = {}; this.hidden = false; this.children = []; this.attributes = new Map(); this.textContent = ''; this.contentWindow = {}; }
  setAttribute(name, value) { this.attributes.set(name, value); }
  removeAttribute(name) { this.attributes.delete(name); }
  append(...children) { for (const child of children) { this.children.push(child); child.parent = this; } }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter(x => x !== this); this.removed = true; }
  focus() { this.focused = true; }
}
function click(element) { element.dispatchEvent(new Event('click')); }
function event(type, props) { const result = new Event(type); Object.assign(result, props); return result; }
function fixture(count = 4, pathname = '/noon/site/docs/examples.html') {
  const doc = new EventTarget(); doc.hidden = false; doc.documentElement = new Element();
  const win = new EventTarget(); win.location = { href:'https://example.test' + pathname, origin:'https://example.test' };
  const timers = new Map(); let serial = 0;
  win.setTimeout = fn => { timers.set(++serial, fn); return serial; };
  win.clearTimeout = id => timers.delete(id);
  const relative = pathname.includes('/site/docs/') ? '../../' : './';
  const cards = Array.from({length:count}, (_,i) => {
    const card = new Element(); card.dataset = { demo:`showcase-case-${i}`, frameSrc:`${relative}embed.html?catalog=showcase&example=showcase-case-${i}` };
    card.parts = Object.fromEntries(['[data-demo-retry]','[data-demo-status]','[data-demo-frame]'].map(key => [key,new Element()]));
    card.querySelector = name => card.parts[name];
    return card;
  });
  doc.querySelectorAll = selector => selector === '[data-demo]' ? cards : [];
  doc.createElement = tag => { assert.equal(tag,'iframe'); return new Element(); };
  return {doc,win,cards,timers,frames:() => cards.flatMap(card => card.parts['[data-demo-frame]'].children),
    install:()=>installPreviews(doc,win,new URL('https://example.test/noon/embed.html'))};
}
function message(f, frame, state = 'ready', view = 'view', origin = f.win.location.origin) {
  f.win.dispatchEvent(event('message', {origin,source:frame.contentWindow,data:{type:'noon-site-embed-state',state,view}}));
}

test('all scenes mount eagerly without an activation click on homepage and docs', () => {
  for (const pathname of ['/noon/index.html', '/noon/site/docs/examples.html']) {
    const f = fixture(4,pathname); const dispose = f.install();
    assert.equal(f.frames().length,4);
    for (const frame of f.frames()) assert.equal(frame.loading,'eager');
    assert.equal(f.timers.size,4);
    dispose(); assert.equal(f.frames().length,0); assert.equal(f.timers.size,0);
  }
});

test('View/Code messages preserve all contexts and independent drafts', () => {
  const f = fixture(); const dispose = f.install(); const frames = [...f.frames()];
  frames.forEach((frame,index)=>{ frame.contentWindow.draft = `source ${index}`; message(f,frame); });
  for (const view of ['code','view','code','view']) message(f,frames[1],'ready',view);
  assert.deepEqual(f.frames(),frames);
  assert.equal(f.cards[1].dataset.embedView,'view');
  assert.equal(frames[1].contentWindow.draft,'source 1');
  f.doc.hidden=true; f.doc.dispatchEvent(new Event('visibilitychange'));
  assert.deepEqual(f.frames(),frames,'hiding the tab must not discard edits');
  dispose();
});

test('navigation retires all contexts while BFCache preserves them for gallery restore', () => {
  const f = fixture(); const dispose = f.install(); const frames = [...f.frames()];
  f.win.dispatchEvent(event('pagehide',{persisted:true})); assert.deepEqual(f.frames(),frames);
  f.win.dispatchEvent(event('pagehide',{persisted:false})); assert.equal(f.frames().length,0);
  assert.ok(frames.every(frame=>frame.removed)); assert.equal(f.timers.size,0);
  dispose();
});

test('foreign, stale, unknown and wrong-origin messages cannot change a scene', () => {
  const f = fixture(2); const dispose = f.install(); const old = f.frames()[0];
  old.dispatchEvent(new Event('error')); click(f.cards[0].parts['[data-demo-retry]']);
  const current = f.frames()[0];
  message(f,old); message(f,current,'ready','view','https://other.test');
  message(f,current,'nonsense'); message(f,current,'ready','other');
  assert.equal(current.dataset.shellReady,undefined);
  message(f,current); assert.equal(current.dataset.shellReady,'true');
  assert.equal(f.timers.size,1,'the other frame retains its independent timer');
  dispose();
});

test('one load failure is retryable without disturbing other live scenes', () => {
  const f = fixture(); const dispose = f.install(); const other = f.frames()[1];
  f.frames()[0].dispatchEvent(new Event('error'));
  assert.equal(f.frames().length,3); assert.equal(f.cards[0].parts['[data-demo-retry]'].hidden,false);
  click(f.cards[0].parts['[data-demo-retry]']); assert.equal(f.frames().length,4);
  assert.equal(f.frames()[1],other);
  [...f.timers.values()][0](); assert.equal(f.frames().length,3);
  dispose();
});

test('parent admits only its canonical embed document and selected example', () => {
  for (const src of ['https://other.test/embed.html?example=showcase-case-0','../../playground.html?example=showcase-case-0','../../embed.html?catalog=showcase&example=other','bad://[']) {
    const f = fixture(1); f.cards[0].dataset.frameSrc = src; const dispose = f.install();
    assert.equal(f.frames().length,0); assert.equal(f.cards[0].dataset.liveState,'error'); dispose();
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

function embedFixture() {
  const selectors = ['.workspace','.preview-pane','.editor-pane','#replace-scene','#auto-run','#status','#patch-status','#preview-recover'];
  const nodes = Object.fromEntries(selectors.map(key=>[key,new Element()]));
  const doc = {documentElement:new Element(),querySelector:key=>nodes[key],createElement:()=>new Element()};
  const callbacks=[],messages=[],microtasks=[];
  const win = new EventTarget();
  win.location={origin:'https://example.test',reload:()=>{throw Error('unexpected reload');}};
  win.parent={postMessage:(data)=>messages.push(data)};
  win.queueMicrotask=fn=>microtasks.push(fn);
  win.MutationObserver=class {
    constructor(callback){callbacks.push(callback);} observe(){} disconnect(){this.disconnected=true;}
  };
  let runs=0;
  nodes['#replace-scene'].click=()=>runs++;
  installEmbedHost(doc,win);
  return {doc,win,nodes,messages,changed:()=>callbacks.forEach(fn=>fn()),
    flush:()=>{while(microtasks.length)microtasks.shift()();},runs:()=>runs,
    tabs:()=>nodes['.workspace'].children[0].children};
}

test('embed enables canonical auto-run without starting a second initial run', () => {
  const f=embedFixture();
  assert.equal(f.nodes['#auto-run'].checked,true);
  assert.equal(f.runs(),0,'the retained gallery bootstrap owns the initial run');
  assert.equal(f.doc.documentElement.dataset.embedView,'view');
  assert.equal(f.nodes['.editor-pane'].inert,true);
  assert.deepEqual(f.tabs().map(tab=>tab.textContent),['View','Code']);
});

test('embed tabs keep the same panels and do not execute or replace source', () => {
  const f=embedFixture(), code=f.nodes['.editor-pane']; code.draft='my source';
  click(f.tabs()[1]); assert.equal(code.inert,false); assert.equal(f.nodes['.preview-pane'].inert,true);
  click(f.tabs()[0]); click(f.tabs()[1]);
  assert.equal(code.draft,'my source'); assert.equal(f.runs(),0);
  assert.equal(f.messages.at(-1).view,'code');
});

test('corrected code clears an initial bootstrap error without hiding active errors', () => {
  const f=embedFixture(); f.nodes['#status'].dataset.liveAuthoring='error'; f.changed();
  assert.equal(f.doc.documentElement.dataset.embedState,'error');
  f.nodes['#patch-status'].dataset.state='applied'; f.changed();
  assert.equal(f.doc.documentElement.dataset.embedState,'ready');
  f.nodes['#patch-status'].dataset.state='error'; f.changed();
  assert.equal(f.doc.documentElement.dataset.embedState,'error');
});

test('BFCache resumes once through canonical Run after controls become available', () => {
  const f=embedFixture(); const run=f.nodes['#replace-scene']; run.disabled=true;
  f.win.dispatchEvent(event('pageshow',{persisted:true})); f.flush(); assert.equal(f.runs(),0);
  run.disabled=false; f.changed(); f.changed(); assert.equal(f.runs(),1);
  f.win.dispatchEvent(event('pageshow',{persisted:true}));
  f.win.dispatchEvent(event('pagehide',{persisted:false})); f.flush(); assert.equal(f.runs(),1);
});

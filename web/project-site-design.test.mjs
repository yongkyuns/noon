import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import test from 'node:test';
import {searchScore} from './site/documentation-ui.js';
const read = file=>readFile(new URL(file,import.meta.url),'utf8');
test('search ranks exact symbols before incidental full-text matches',()=>{
 assert.ok(searchScore('Scene','class documentation','scene')>searchScore('Scene authoring','create a Scene','scene'));
 assert.ok(searchScore('Scene authoring','control flow','scene')>searchScore('Introduction','A Scene can move','scene'));
 assert.equal(searchScore('Scene','object','not-present'),0);
 assert.ok(searchScore('Native export','video output','native video')>0);
 assert.equal(searchScore('Scene','source','   '),1);
});
test('homepage and documentation share theme tokens and a navigation model',async()=>{
 for(const stylesheet of ['./site/site.css','./site/docs.css']) assert.match(await read(stylesheet),/foundation\.css/);
 const home=await read('./site/index.html');
 for(const route of ['site/docs/index.html','site/docs/getting-started.html','site/docs/examples.html','site/docs/api.html']) assert.ok(home.includes(route),route);
 assert.match(home,/data-search-shortcut/);
 assert.equal((home.match(/data-demo="/g)||[]).length,4);
 assert.doesNotMatch(home,/<video\b|demo-poster|data-demo-open/);
});
test('late showcase CSS cannot reintroduce full gallery chrome in the embed',async()=>{
 const css=await read('./site/embed.css');
 assert.match(css,/html\[data-noon-embed="true"\]\[data-noon-catalog\] \.selected-example\{display:none!important\}/);
});

import assert from 'node:assert/strict';
import test from 'node:test';
import { observeRendererCalls } from './project-site-native-observation.mjs';

test('native observation preserves receiver, arguments, result and cleanup', () => {
  const events = [], argument = {}, result = {};
  class Renderer { render(arg) { assert.equal(this, instance); assert.equal(arg, argument); return result; } }
  const instance = new Renderer(), original = Renderer.prototype.render;
  const restore = observeRendererCalls(Renderer, event => events.push(event));
  assert.equal(instance.render(argument), result);
  assert.deepEqual(events, [{name:'render',phase:'enter'}, {name:'render',phase:'return'}]);
  restore(); assert.equal(Renderer.prototype.render, original);
});

test('native observation preserves the exact Promise and its rejection', async () => {
  const events = [], failure = new Error('native rejection'), promise = Promise.reject(failure);
  class Renderer { static create() { return promise; } }
  const restore = observeRendererCalls(Renderer, event => events.push(event));
  assert.equal(Renderer.create(), promise);
  await assert.rejects(promise, error => error === failure);
  assert.deepEqual(events.map(event => event.phase), ['enter','return','rejected']); restore();
});

test('native exception is recorded and rethrown unchanged', () => {
  const events = [], failure = new Error('GPU call');
  class Renderer { free() { throw failure; } }
  const restore = observeRendererCalls(Renderer, event => events.push(event));
  assert.throws(() => new Renderer().free(), error => error === failure);
  assert.deepEqual(events.map(event => event.phase), ['enter','threw']); restore();
});

test('successful Promise remains identical and inactive frames stay silent', async () => {
  const events = [], promise = Promise.resolve(3);
  class Renderer { static create() { return promise; } render() { return true; } }
  const restore = observeRendererCalls(Renderer, event => events.push(event), () => false);
  assert.equal(new Renderer().render(), true); assert.equal(events.length, 0);
  assert.equal(Renderer.create(), promise); await promise;
  assert.deepEqual(events.map(event => event.phase), ['enter','return','resolved']); restore();
});

test('a failing observer never changes the native operation', () => {
  class Renderer { render() { return true; } }
  const restore = observeRendererCalls(Renderer, () => { throw new Error('observer'); });
  assert.equal(new Renderer().render(), true); restore();
});

test('replay records retained bundle replacement without altering native behavior', () => {
  const events = [], bundle = new Uint8Array([1, 2, 3]), result = {};
  class Renderer { replaceResourceBundle(bytes) { assert.equal(this, instance); assert.equal(bytes, bundle); return result; } }
  const instance = new Renderer(), original = Renderer.prototype.replaceResourceBundle;
  const restore = observeRendererCalls(Renderer, event => events.push(event), () => false);
  assert.equal(instance.replaceResourceBundle(bundle), result);
  assert.deepEqual(events, [
    {name:'replaceResourceBundle',phase:'enter'},
    {name:'replaceResourceBundle',phase:'return'},
  ]);
  restore();
  assert.equal(Renderer.prototype.replaceResourceBundle, original);
});

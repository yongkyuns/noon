// Temporary #1653 WebGL draw-cost experiment. Never merge into the product.
export function classifyFragment(source) {
  const main = source.slice(source.lastIndexOf('void main'));
  if (/sample_glyph/.test(main)) return 'text';
  if (/revealed_path_color/.test(main)) return 'path';
  if (/half_length/.test(main)) return 'line';
  if (/half_size/.test(main)) return 'rectangle';
  if (/head_angle|atan\(/.test(main)) return 'circle';
  if (/srgb_to_linear/.test(main)) return 'present';
  if (/point_lit/.test(main)) return 'mesh';
  return 'unknown';
}

export function installDrawProbe(global, mode) {
  const modes = ['normal', 'skip-line', 'skip-path', 'skip-text', 'skip-present', 'clip-all'];
  if (!modes.includes(mode)) throw new Error(`invalid draw probe mode ${mode}`);
  const proto = global.WebGL2RenderingContext?.prototype;
  if (!proto) throw new Error('draw probe requires WebGL2');
  const originals = {};
  const contexts = new WeakMap();
  const programs = [];
  const unseen = [];
  let nextContext = 0;
  const state = gl => {
    let value = contexts.get(gl);
    if (!value) {
      value = { id: ++nextContext, shaders: new WeakMap(), programs: new WeakMap(), current: null,
        scissorEnabled: false, scissor: null };
      contexts.set(gl, value);
    }
    return value;
  };
  function hook(name, operation) {
    const original = proto[name];
    if (typeof original !== 'function') throw new Error(`missing WebGL2 method ${name}`);
    originals[name] = original;
    proto[name] = function (...args) { return operation.call(this, original, ...args); };
  }
  hook('shaderSource', function (call, shader, source) {
    state(this).shaders.set(shader, {type: this.getShaderParameter(shader, this.SHADER_TYPE), source});
    return call.call(this, shader, source);
  });
  hook('attachShader', function (call, program, shader) {
    const s = state(this);
    let p = s.programs.get(program);
    if (!p) {
      if (programs.length >= 128) throw new Error('draw probe exceeded program bound');
      p = {id: programs.length + 1, context: s.id, shaders: [], kind: 'unknown',
        attempted: 0, submitted: 0, skipped: 0, clipped: 0, methods: {}};
      s.programs.set(program, p); programs.push(p);
    }
    p.shaders.push(s.shaders.get(shader));
    return call.call(this, program, shader);
  });
  hook('linkProgram', function (call, program) {
    const result = call.call(this, program);
    const p = state(this).programs.get(program);
    if (!p) throw new Error('unobserved shader program');
    p.fragment = p.shaders.find(s => s.type === this.FRAGMENT_SHADER)?.source ?? '';
    p.vertex = p.shaders.find(s => s.type === this.VERTEX_SHADER)?.source ?? '';
    p.kind = classifyFragment(p.fragment);
    unseen.push({id: p.id, context: p.context, kind: p.kind, fragment: p.fragment, vertex: p.vertex});
    return result;
  });
  hook('useProgram', function (call, program) {
    state(this).current = program === null ? null : state(this).programs.get(program);
    return call.call(this, program);
  });
  hook('enable', function (call, cap) {
    if (cap === this.SCISSOR_TEST) state(this).scissorEnabled = true;
    return call.call(this, cap);
  });
  hook('disable', function (call, cap) {
    if (cap === this.SCISSOR_TEST) state(this).scissorEnabled = false;
    return call.call(this, cap);
  });
  hook('scissor', function (call, x, y, w, h) {
    state(this).scissor = [x, y, w, h];
    return call.call(this, x, y, w, h);
  });
  for (const name of ['drawArrays', 'drawElements', 'drawArraysInstanced', 'drawElementsInstanced', 'drawRangeElements']) {
    hook(name, function (call, ...args) {
      const s = state(this), p = s.current;
      if (!p) throw new Error('draw without an observed program');
      p.attempted += 1; p.methods[name] = (p.methods[name] ?? 0) + 1;
      p.firstArguments ??= args;
      if (mode === `skip-${p.kind}`) { p.skipped += 1; return; }
      p.submitted += 1;
      if (mode !== 'clip-all') return call.apply(this, args);
      p.clipped += 1;
      // Suppress raster coverage while preserving the draw call. No synchronous
      // per-draw GL state query: restore state observed through original API calls.
      originals.enable.call(this, this.SCISSOR_TEST);
      originals.scissor.call(this, 0, 0, 0, 0);
      try { return call.apply(this, args); }
      finally {
        originals.scissor.apply(this, s.scissor ?? [0, 0, this.drawingBufferWidth, this.drawingBufferHeight]);
        if (!s.scissorEnabled) originals.disable.call(this, this.SCISSOR_TEST);
      }
    });
  }
  return Object.freeze({snapshot(includePrograms = false) {
    return {mode, diagnosticOnly: true, contexts: nextContext, newPrograms: includePrograms ? unseen.splice(0) : [],
      counters: programs.map(p => ({id: p.id, context: p.context, kind: p.kind,
        attempted: p.attempted, submitted: p.submitted, skipped: p.skipped,
        clipped: p.clipped, methods: {...p.methods}, firstArguments: p.firstArguments}))};
  }});
}

if (typeof globalThis.WebGL2RenderingContext === 'function' && globalThis.__NOON_CAMERA_DRAW_MODE) {
  globalThis.__noonCameraDrawProbe = installDrawProbe(globalThis, globalThis.__NOON_CAMERA_DRAW_MODE);
}

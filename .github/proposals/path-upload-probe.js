// Disposable WebGPU write/draw observation. Original methods receive their
// arguments unchanged. Captured bytes never become engine or rendering inputs.
(() => {
  const buffers = new WeakMap(), passes = new WeakMap();
  const log = console.log.bind(console);
  let checkpoint = null;
  console.log = (...args) => {
    const first = args[0];
    if (typeof first === 'string' && first.startsWith('NOON_CHECKPOINT_DELTA ')) {
      try {
        const packet = JSON.parse(first.slice('NOON_CHECKPOINT_DELTA '.length));
        if (packet.kind === 'prepared') checkpoint = Math.abs(packet.time - 3.1) < 1e-12 && packet.objects.some(x => x.object === 19) ? packet.time : null;
      } catch {}
    }
    log(...args);
  };
  const emit = packet => log('NOON_CHECKPOINT_DELTA ' + JSON.stringify(packet));
  const device = globalThis.GPUDevice?.prototype;
  const queue = globalThis.GPUQueue?.prototype;
  const pass = globalThis.GPURenderPassEncoder?.prototype;
  if (!device || !queue || !pass) throw new Error('WebGPU diagnostic interfaces unavailable');
  const createBuffer = device.createBuffer;
  device.createBuffer = function(descriptor) {
    const result = Reflect.apply(createBuffer, this, arguments);
    buffers.set(result, {label:descriptor.label ?? '', bytes:new Uint8Array(descriptor.size), captured:true});
    return result;
  };
  const writeBuffer = queue.writeBuffer;
  queue.writeBuffer = function(buffer, offset, data, dataOffset = 0, size) {
    const result = Reflect.apply(writeBuffer, this, arguments);
    const record = buffers.get(buffer);
    if (!record || !/path/i.test(record.label)) return result;
    const elementBytes = data.BYTES_PER_ELEMENT ?? 1;
    const source = ArrayBuffer.isView(data)
      ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
      : new Uint8Array(data);
    const start = Number(dataOffset) * elementBytes;
    const count = size === undefined ? source.byteLength - start : Number(size) * elementBytes;
    if (start < 0 || count < 0 || start + count > source.byteLength || Number(offset) + count > record.bytes.length) {
      record.captured = false;
    } else {
      record.bytes.set(source.subarray(start, start + count), Number(offset));
    }
    return result;
  };
  function state(object) {
    let result = passes.get(object);
    if (!result) { result = {pipeline:null, vertices:new Map(), index:null}; passes.set(object, result); }
    return result;
  }
  const setPipeline = pass.setPipeline;
  pass.setPipeline = function(pipeline) {
    const result = Reflect.apply(setPipeline, this, arguments);
    state(this).pipeline = pipeline.label;
    return result;
  };
  const setVertexBuffer = pass.setVertexBuffer;
  pass.setVertexBuffer = function(slot, buffer, offset = 0, size) {
    const result = Reflect.apply(setVertexBuffer, this, arguments);
    state(this).vertices.set(slot, {buffer, offset, size});
    return result;
  };
  const setIndexBuffer = pass.setIndexBuffer;
  pass.setIndexBuffer = function(buffer, format, offset = 0, size) {
    const result = Reflect.apply(setIndexBuffer, this, arguments);
    state(this).index = {buffer, format, offset, size};
    return result;
  };
  function record(binding) {
    if (!binding) return null;
    const saved = buffers.get(binding.buffer);
    if (!saved) return {missing:true};
    return {label:saved.label, offset:binding.offset, size:binding.size ?? null, format:binding.format ?? null,
      captured:saved.captured, bytes:Array.from(saved.bytes)};
  }
  const drawIndexed = pass.drawIndexed;
  pass.drawIndexed = function() {
    const result = Reflect.apply(drawIndexed, this, arguments);
    if (checkpoint !== null) {
      const current = state(this);
      emit({kind:'gpu-draw', time:checkpoint, pipeline:current.pipeline, args:Array.from(arguments),
        vertices:Array.from(current.vertices, ([slot, binding]) => ({slot, ...record(binding)})),
        index:record(current.index)});
    }
    return result;
  };
  const submit = queue.submit;
  queue.submit = function() {
    const result = Reflect.apply(submit, this, arguments);
    checkpoint = null;
    return result;
  };
})();

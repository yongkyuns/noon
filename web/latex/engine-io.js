// Host I/O for the pinned e-TeX WebAssembly ABI. This module neither interprets
// TeX nor owns scene state: the real engine produces DVI for Rust normalization.
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const MAX_OUTPUT_BYTES = 16 * 1024 * 1024;
const MAX_LOG_BYTES = 64 * 1024;
const MAX_IO_CALLS = 2_000_000;

export function createTexIo(memory, assets, source) {
  const bytes = new Uint8Array(memory.buffer);
  const words = new DataView(memory.buffer);
  const files = [];
  const output = new Map();
  const input = encoder.encode(" input.tex\n\\end\n");
  let log = "";
  let calls = 0;
  let writtenBytes = 0;
  let finished = false;
  const deadline = performance.now() + 15_000;

  function budget() {
    if (++calls > MAX_IO_CALLS || performance.now() > deadline) {
      throw new Error("LaTeX compilation exceeded its I/O budget");
    }
  }
  function filename(length, pointer) {
    return decoder.decode(bytes.subarray(pointer, pointer + length))
      .replace(/\0+$/, "").replace(/^\"([^\"]*)\".*$/, "$1")
      .trimEnd().replace(/^\*/, "").replace(/^TeXfonts:/, "")
      .replace(/^TeXformats:TEX.POOL$/, "tex.pool");
  }
  function open(name, writing) {
    budget();
    if (files.length >= 4096) throw new Error("LaTeX opened too many files");
    let data;
    if (name === "TTY:") {
      data = writing ? new Uint8Array() : input;
    } else if (writing) {
      data = new Uint8Array();
    } else if (name === "input.tex") {
      data = source;
    } else {
      const prior = output.get(name);
      data = prior ? prior.data.subarray(0, prior.position) : assets.get(name);
    }
    const file = {
      name, data: data ?? new Uint8Array(), position: 0, linePosition: 0,
      missing: data === undefined, eof: false, eoln: false,
      stdin: name === "TTY:" && !writing, stdout: name === "TTY:" && writing,
    };
    if (writing && !file.stdout) output.set(name, file);
    files.push(file);
    return files.length - 1;
  }
  function appendLog(value) {
    if (log.length < MAX_LOG_BYTES) log += value.slice(0, MAX_LOG_BYTES - log.length);
  }
  function write(descriptor, data) {
    budget();
    const file = descriptor < 0 ? undefined : files[descriptor];
    if (!file || file.stdout) {
      appendLog(decoder.decode(data));
      return;
    }
    const needed = file.position + data.length;
    writtenBytes += data.length;
    if (writtenBytes > MAX_OUTPUT_BYTES) throw new Error("LaTeX output exceeds 16 MiB");
    if (needed > file.data.length) {
      const expanded = new Uint8Array(Math.min(MAX_OUTPUT_BYTES,
        Math.max(needed, 1024, file.data.length * 2)));
      expanded.set(file.data);
      file.data = expanded;
    }
    file.data.set(data, file.position);
    file.position = needed;
  }
  function print(descriptor, value) { write(descriptor, encoder.encode(String(value))); }

  const library = {
    printInteger: print,
    printChar: (fd, value) => write(fd, Uint8Array.of(value)),
    printString: (fd, pointer) => write(fd, bytes.subarray(pointer + 1, pointer + 1 + bytes[pointer])),
    printNewline: (fd) => print(fd, "\n"),
    reset: (length, pointer) => open(filename(length, pointer), false),
    rewrite: (length, pointer) => open(filename(length, pointer), true),
    close: () => { budget(); },
    eof: (fd) => Number(files[fd].eof),
    eoln: (fd) => Number(files[fd].eoln),
    erstat: (fd) => Number(files[fd].missing),
    getCurrentMinutes: () => 0,
    getCurrentDay: () => 1,
    getCurrentMonth: () => 1,
    getCurrentYear: () => 2022,
    tex_final_end: () => { finished = true; },
    inputln(fd, bypass, buffer, firstPointer, lastPointer, _maximumPointer, size) {
      budget();
      const file = files[fd];
      if (bypass && !file.eof && file.eoln) file.linePosition++;
      const first = words.getUint32(firstPointer, true);
      words.setUint32(lastPointer, first, true);
      if (file.linePosition >= file.data.length) {
        file.eof = true;
        if (file.stdin && !finished) throw new Error(`LaTeX requested interactive input\n${log}`);
        return 0;
      }
      let end = file.data.indexOf(10, file.linePosition);
      if (end < 0) end = file.data.length;
      const length = end - file.linePosition;
      if (first + length > size) throw new Error("LaTeX input line exceeds engine buffer");
      bytes.set(file.data.subarray(file.linePosition, end), buffer + first);
      let last = first + length;
      while (last > first && bytes[buffer + last - 1] === 32) last--;
      words.setUint32(lastPointer, last, true);
      file.linePosition = end;
      file.eoln = true;
      return 1;
    },
    get(fd, pointer, length) {
      budget();
      const file = files[fd];
      if (file.position >= file.data.length) {
        bytes[pointer] = file.stdin ? 13 : 0;
        file.eof = file.eoln = true;
        if (file.stdin && !finished) throw new Error(`LaTeX requested interactive input\n${log}`);
        return;
      }
      const end = Math.min(file.position + length, file.data.length);
      bytes.set(file.data.subarray(file.position, end), pointer);
      file.position += length;
      file.eoln = bytes[pointer] === 10 || bytes[pointer] === 13;
    },
    put: (fd, pointer, length) => write(fd, bytes.subarray(pointer, pointer + length)),
  };
  return {
    library,
    result() {
      const dvi = output.get("input.dvi");
      if (!finished || !dvi?.position || /^! /m.test(log)) {
        throw new Error(`LaTeX compilation failed\n${log}`);
      }
      return { dvi: dvi.data.slice(0, dvi.position), log };
    },
  };
}

// The format image is mostly zeroes. Keep only populated pages, rather than a
// second 72-MiB memory image throughout the optional compiler's lifetime.
export function sparseFormatSnapshot(format) {
  const formatLength = format.length;
  const pages = [];
  for (let offset = 0; offset < format.length; offset += 16_384) {
    const page = format.subarray(offset, offset + 16_384);
    if (page.some(byte => byte !== 0)) pages.push({ offset, bytes: page.slice() });
  }
  return {
    byteLength: pages.reduce((total, page) => total + page.bytes.length, 0),
    reset(memory) {
      const target = new Uint8Array(memory.buffer);
      if (target.length !== formatLength) throw new Error("LaTeX format memory size mismatch");
      target.fill(0);
      for (const page of pages) target.set(page.bytes, page.offset);
    },
  };
}

export function createTexEngine(module, format, assets) {
  const memory = new WebAssembly.Memory({ initial: 1100, maximum: 1100 });
  const snapshot = sparseFormatSnapshot(format);
  return {
    snapshotBytes: snapshot.byteLength,
    compile(document) {
      const source = encoder.encode(document);
      if (source.length > 1024 * 1024) throw new Error("LaTeX source exceeds 1 MiB");
      snapshot.reset(memory);
      const io = createTexIo(memory, assets, source);
      const instance = new WebAssembly.Instance(module, { library: io.library, env: { memory } });
      instance.exports.main();
      return io.result();
    },
  };
}


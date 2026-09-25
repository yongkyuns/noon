// Explicit native authoring subprocess. Only this boundary uses a wire format;
// compiled resources enter the same Rust semantic store as browser authoring.
import { once } from "node:events";
import { prepareLatexBackend } from "./backend.js";

const MAX_REQUEST = 6 * 1024 * 1024; // JSON escaping of a 1 MiB document.
const MAX_PAYLOAD = 16 * 1024 * 1024;

async function respond(metadata, payloads = []) {
  const payload = Buffer.concat(payloads);
  if (payload.length > MAX_PAYLOAD) throw new Error("LaTeX response exceeds limit");
  const header = Buffer.from(JSON.stringify({ ...metadata, bytes: payload.length }));
  if (header.length > 4096) throw new Error("LaTeX response header exceeds limit");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(header.length);
  if (!process.stdout.write(Buffer.concat([length, header, payload]))) await once(process.stdout, "drain");
}

async function* requests() {
  let pending = Buffer.alloc(0);
  for await (const chunk of process.stdin) {
    pending = Buffer.concat([pending, chunk]);
    let end;
    while ((end = pending.indexOf(10)) >= 0) {
      if (end > MAX_REQUEST) throw new Error("LaTeX request exceeds limit");
      const line = pending.subarray(0, end);
      pending = pending.subarray(end + 1);
      yield JSON.parse(line.toString("utf8"));
    }
    if (pending.length > MAX_REQUEST) throw new Error("LaTeX request exceeds limit");
  }
  if (pending.length) throw new Error("Truncated LaTeX request");
}

try {
  const backend = await prepareLatexBackend();
  await respond({ protocol: 1, identity: backend.identity });
  for await (const request of requests()) {
    try {
      if (request.op === "compile" && typeof request.document === "string") {
        const { dvi } = backend.compile(request.document);
        await respond({ kind: "dvi" }, [dvi]);
      } else if (request.op === "font" && typeof request.name === "string" && /^[\w-]{1,128}$/.test(request.name)) {
        const { tfm, ttf, faceKey } = backend.font(request.name);
        await respond({ kind: "font", tfmBytes: tfm.length, faceKey }, [tfm, ttf]);
      } else throw new Error("Invalid LaTeX host request");
    } catch (error) {
      await respond({ error: String(error.message ?? error).slice(0, 2048) });
    }
  }
} catch (error) {
  await respond({ error: String(error.message ?? error).slice(0, 2048) });
  process.exitCode = 1;
}


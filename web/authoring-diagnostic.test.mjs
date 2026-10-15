import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  authoringDiagnosticSourceRange, parseAuthoringDiagnostic, presentAuthoringDiagnostic,
} from "./authoring-diagnostic.js";

// Exercise the actual Python producer, not a separately hand-maintained wire
// fixture. No browser, WASM engine, network, image oracle or extra CI workflow.
const python = spawnSync("python3", ["-c", String.raw`
import json
from _noon_diagnostics import capture_exception
from _noon_errors import NoonCallbackError, NoonErrorCause, NoonUnsupportedError
filename, revision = "scene.py", "run-17"
def error(source):
    try:
        exec(compile(source, filename, "exec"), {})
    except BaseException as failure:
        return failure
def capture(failure, source=""):
    return capture_exception(failure, source=source, filename=filename,
                             revision=revision, phase="construct")
source = "label = '😀한글'; value = missing\n"
crlf = "label = '😀'\r\nvalue = missing\r\n"
chain = "try:\n    missing\nexcept Exception as e:\n    raise ValueError('outer') from e\n"
callback = NoonCallbackError(NoonErrorCause("callback_failure", "python.callback_failed", "callback"),
                             object(), "Scene.continuation")
callback.python_cause = error(source)
engine = NoonUnsupportedError(NoonErrorCause("unsupported_operation", "matching.key_map", "key_map unsupported",
                              NoonErrorCause("invalid_input", "argument.name", "key_map")),
                              object(), "TransformMatchingShapes")
print(json.dumps({
    "source": source, "crlfSource": crlf, "chainSource": chain,
    "ordinary": capture(error(source), source),
    "syntax": capture(error("label = '😀'; value = (1 + )\n"), "label = '😀'; value = (1 + )\n"),
    "engine": capture(engine), "callback": capture(callback, source),
    "chain": capture(error(chain), chain), "crlf": capture(error(crlf), crlf),
    "group": capture(ExceptionGroup("group", [ValueError("x"), TypeError("y")])),
    "unknown": capture(TypeError("unsupported_operation: invalid_input")),
    "long": capture(ValueError("😀" * 5000)),
}))
`], {
  encoding: "utf8", timeout: 15000, maxBuffer: 1024 * 1024,
  env: { ...process.env, PYTHONPATH: fileURLToPath(new URL("./python/", import.meta.url)),
    PYTHONDONTWRITEBYTECODE: "1" },
});
assert.equal(python.error, undefined, String(python.error));
assert.equal(python.status, 0, python.stderr);
const fixtures = JSON.parse(python.stdout);
const document = { filename: "scene.py", revision: "run-17" };
const copy = (value = fixtures.ordinary) => structuredClone(value);

for (const name of ["ordinary", "syntax", "engine", "callback", "chain", "crlf", "group", "unknown", "long"]) {
  test(`accepts real Python ${name} snapshot`, () => {
    const parsed = parseAuthoringDiagnostic(fixtures[name]);
    assert.notEqual(parsed, null);
    assert.deepEqual(parsed, fixtures[name]);
  });
}

test("typed metadata supplies support explanation without claiming browser-only scope", () => {
  const view = presentAuthoringDiagnostic(fixtures.engine);
  assert.equal(view.title, "Operation not supported");
  assert.equal(view.operation, "TransformMatchingShapes");
  assert.equal(view.code, "matching.key_map");
  assert.equal(view.message, "key_map unsupported");
  assert.doesNotMatch(view.hint, /browser|remove|ignore/i);
});

test("ordinary error wording never supplies engine classification", () => {
  const view = presentAuthoringDiagnostic(fixtures.unknown);
  assert.equal(view.title, "Python raised TypeError");
  assert.equal(view.code, null);
  assert.match(view.hint, /identify the cause/);
});

test("UTF-16 source range selects actual Python failing expression", () => {
  const range = authoringDiagnosticSourceRange(fixtures.ordinary, fixtures.source, document);
  assert.deepEqual(range, { from: fixtures.source.indexOf("missing"), to: fixtures.source.indexOf("missing") + 7 });
  assert.equal(fixtures.source.slice(range.from, range.to), "missing");
});

test("CRLF source range includes actual newline width", () => {
  const range = authoringDiagnosticSourceRange(fixtures.crlf, fixtures.crlfSource, document);
  assert.equal(fixtures.crlfSource.slice(range.from, range.to), "missing");
});

test("new source revision cannot receive an old diagnostic range", () => {
  assert.equal(authoringDiagnosticSourceRange(fixtures.ordinary, fixtures.source,
    { ...document, revision: "run-18" }), null);
});

test("different filename cannot receive range even with same revision", () => {
  assert.equal(authoringDiagnosticSourceRange(fixtures.ordinary, fixtures.source,
    { ...document, filename: "/other/scene.py" }), null);
});

test("missing document identity has no source navigation", () => {
  assert.equal(authoringDiagnosticSourceRange(fixtures.ordinary, fixtures.source), null);
});

test("callback navigation uses preserved Python cause rather than transport wrapper", () => {
  const range = authoringDiagnosticSourceRange(fixtures.callback, fixtures.source, document);
  assert.equal(fixtures.source.slice(range.from, range.to), "missing");
  assert.equal(presentAuthoringDiagnostic(fixtures.callback).title, "A callback failed");
});

test("explicit cause navigation selects original failure", () => {
  const range = authoringDiagnosticSourceRange(fixtures.chain, fixtures.chainSource, document);
  assert.equal(fixtures.chainSource.slice(range.from, range.to), "missing");
});

test("does not choose an arbitrary exception-group child for source navigation", () => {
  const value = copy(fixtures.group);
  value.exception.children[0].location = fixtures.ordinary.exception.location;
  assert.equal(authoringDiagnosticSourceRange(value, fixtures.source, document), null);
});

test("line-only location remains a line-only range", () => {
  const value = copy();
  value.exception.location = { line: 1, column: null, endLine: null, endColumn: null };
  assert.deepEqual(authoringDiagnosticSourceRange(value, fixtures.source, document),
    { from: 0, to: fixtures.source.length - 1 });
});

test("multiline range supports exclusive ends", () => {
  const value = copy();
  value.exception.location = { line: 1, column: 2, endLine: 2, endColumn: 3 };
  assert.deepEqual(authoringDiagnosticSourceRange(value, "hello\r\nworld", document), { from: 2, to: 10 });
});

test("invalid source positions are not clamped onto different code", () => {
  for (const position of [
    { line: 90, column: 0, endLine: null, endColumn: null },
    { line: 1, column: 400, endLine: null, endColumn: null },
    { line: 1, column: 0, endLine: 90, endColumn: 1 },
  ]) {
    const value = copy();
    value.exception.location = position;
    assert.equal(authoringDiagnosticSourceRange(value, fixtures.source, document), null);
  }
});

test("unavailable location has no fabricated source link", () => {
  assert.equal(authoringDiagnosticSourceRange(fixtures.engine, fixtures.source, document), null);
});

for (const [label, mutate] of [
  ["version", (d) => { d.version = 2; }],
  ["phase", (d) => { d.phase = "guess"; }],
  ["document", (d) => { d.document = null; }],
  ["revision", (d) => { d.document.revision = ""; }],
  ["message", (d) => { d.exception.message = { toString() { return "fake"; } }; }],
  ["huge text", (d) => { d.exception.message = "a".repeat(2049); }],
  ["negative column", (d) => { d.exception.location.column = -1; }],
  ["boolean line", (d) => { d.exception.location.line = true; }],
  ["unsafe line", (d) => { d.exception.location.line = Number.MAX_SAFE_INTEGER + 1; }],
  ["reverse range", (d) => { d.exception.location.endColumn = 0; }],
  ["partial end", (d) => { d.exception.location.endLine = null; }],
  ["too many frames", (d) => { d.exception.frames = Array(17).fill(d.exception.frames[0]); }],
  ["too many notes", (d) => { d.exception.notes = Array(5).fill("note"); }],
  ["missing cause relation", (d) => { d.exception.causeRelation = "cause"; }],
  ["cycle", (d) => { d.exception.cause = d.exception; d.exception.causeRelation = "cause"; }],
  ["truncated flag", (d) => { d.truncated = "true"; }],
]) {
  test(`rejects malformed ${label} without throwing`, () => {
    const value = copy();
    mutate(value);
    assert.equal(parseAuthoringDiagnostic(value), null);
    assert.equal(presentAuthoringDiagnostic(value), null);
    assert.equal(authoringDiagnosticSourceRange(value, fixtures.source, document), null);
  });
}

test("null, arrays and throwing getters retain generic fallback", () => {
  for (const value of [null, [], "Traceback ...", { get version() { throw new Error("getter"); } }]) {
    assert.equal(parseAuthoringDiagnostic(value), null);
  }
});

test("exception budget applies across the entire tree", () => {
  const value = copy(fixtures.group);
  value.exception.children = Array.from({ length: 8 }, () => copy(fixtures.unknown).exception);
  assert.equal(parseAuthoringDiagnostic(value), null);
});

test("engine metadata is bounded and validated too", () => {
  const value = copy(fixtures.engine);
  value.exception.engine.causes = Array(9).fill(value.exception.engine.causes[0]);
  assert.equal(parseAuthoringDiagnostic(value), null);
  value.exception.engine.causes = [];
  value.exception.engine.code = 123;
  assert.equal(parseAuthoringDiagnostic(value), null);
});

test("snapshot copying and deep freezing prevent later transport mutation", () => {
  const value = copy(fixtures.engine);
  const parsed = parseAuthoringDiagnostic(value);
  value.exception.engine.causes[0].message = "changed later";
  assert.equal(parsed.exception.engine.causes[0].message, "key_map");
  assert.ok(Object.isFrozen(parsed));
  assert.ok(Object.isFrozen(parsed.document));
  assert.ok(Object.isFrozen(parsed.exception.engine.causes[0]));
  assert.throws(() => { parsed.exception.message = "overwrite"; }, TypeError);
});

test("unknown object keys are not copied or used for prototype mutation", () => {
  const value = copy();
  Object.defineProperty(value, "__proto__", { value: { polluted: true }, enumerable: true });
  const parsed = parseAuthoringDiagnostic(value);
  assert.equal(Object.hasOwn(parsed, "__proto__"), false);
  assert.equal({}.polluted, undefined);
});

test("truncation remains explicit and non-BMP text satisfies consumer limits", () => {
  const view = presentAuthoringDiagnostic(fixtures.long);
  assert.equal(view.truncated, true);
  assert.equal(view.message.length, 2048);
  assert.equal([...view.message].length, 1024);
});

test("messages are literal text, not markup or actionable code", () => {
  const value = copy();
  value.exception.message = '<img src=x onerror="run()">';
  assert.equal(presentAuthoringDiagnostic(value).message, value.exception.message);
});

for (const [column, endColumn] of [[1, 2], [0, 1]]) {
  test(`does not split surrogate pairs (${column}, ${endColumn})`, () => {
    const value = copy();
    value.exception.location = { line: 1, column, endLine: 1, endColumn };
    assert.equal(authoringDiagnosticSourceRange(value, "😀missing", document), null);
  });
}

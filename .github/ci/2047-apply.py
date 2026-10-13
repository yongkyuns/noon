from pathlib import Path
import hashlib
import json
import os
import textwrap

ROOT = Path.cwd()
EXPECTED = {
    'web/python-worker.source.js': '57fd9f31e20ae0abbc625931580cba77bf8cc119',
    'web/authoring-client.js': '93330020bab63a05d38374e31f2df3c77b0d0f11',
    'web/main.js': '0745084ec9473e4ee65d596e5fd3b1f1086e3fb5',
    'web/python-compat-modules.js': '3adbe752018af74d1c5b9af3668d24265a86663b',
    'web/index.html': '7cfe4056e5bf4795bb09d0b9c248a47a0f6f27f8',
    'web/authoring-client.test.mjs': 'e3b89af868c95baad15db74d902dc11af57cf17f',
    'web/authoring-diagnostic.js': '4942f587c03cd338b9d478232639c50be9fd397d',
    'scripts/playground-authoring-error-recovery-smoke.mjs': '57f0e08d569f0c40e203c7498a9c080d5265744e',
}
for name, expected in EXPECTED.items():
    data = (ROOT/name).read_bytes()
    actual = hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
    assert actual == expected, f'{name} changed: {actual}; refusing to overwrite'

def replace(source, before, after, count=1):
    actual = source.count(before)
    assert actual == count, f'anchor count {actual}, expected {count}: {before[:120]}'
    return source.replace(before, after)

def edit(name, transform):
    path=ROOT/name
    path.write_text(transform(path.read_text()))

def worker(s):
    s = 'import { attachAuthoringDiagnostic } from "./authoring-error.js";\n'+s
    s = replace(s, '          request.context,\n        );', '          request.context,\n          request.sourceDocument ?? { filename: "scene.py", revision: `request-${requestId}` },\n        );')
    s = replace(s, 'async function runAuthoringSource(pyodide, source, context) {\n  await prepareAuthoringDependencies(pyodide, source);\n', 'async function runAuthoringSource(pyodide, source, context, sourceDocument) {\n')
    s = replace(s, '  globals.set("__noon_context_json", JSON.stringify(context));', '''  globals.set("__noon_context_json", JSON.stringify(context));
  globals.set("__noon_source_document_json", JSON.stringify(sourceDocument));
  // Compile the actual user source before import discovery. A syntax error
  // belongs to that source, not to the package scanner's internal Python code.
  // The existing dependency loader runs while this coroutine is suspended;
  // scene source still executes exactly once after its dependencies are ready.
  globals.set("__noon_prepare_dependencies", () => prepareAuthoringDependencies(pyodide, source));''')
    start = s.index('async function runAuthoringSource')
    body_start = s.index('`\n', s.index('const resultJson = await pyodide.runPythonAsync(', start))+2
    body_end = s.index('\n`,', body_start)
    body=s[body_start:body_end]
    body=replace(body, 'import json\n', '')
    body=replace(body, '    __noon_source\n)', '    __noon_source, filename=__noon_source_document["filename"]\n)')
    body=replace(body, 'with authoring_source_scope():', '''__noon_phase = "dependencies"
await __noon_prepare_dependencies()
__noon_phase = "construct"
with authoring_source_scope():''')
    body=replace(body, 'if isinstance(__noon_result, Scene):', '__noon_phase = "runtime"\nif isinstance(__noon_result, Scene):')
    body=replace(body, '\njson.dumps(\n', '\n__noon_result_json = json.dumps(\n')
    body='''import json
__noon_diagnostic_json = None
__noon_source_document = json.loads(__noon_source_document_json)
__noon_phase = "compile"
try:
'''+textwrap.indent(body,'    ')+'''
except BaseException as __noon_error:
    try:
        # Lazy failure-only import. Do not retain the exception in a global
        # mailbox, replace it, or inspect interpreter-global last-error state.
        from _noon_diagnostics import capture_exception
        __noon_diagnostic_json = json.dumps(capture_exception(
            __noon_error, source=__noon_source,
            filename=__noon_source_document["filename"],
            revision=__noon_source_document["revision"], phase=__noon_phase,
        ))
    except BaseException:
        pass
    raise
__noon_result_json'''
    s=s[:body_start]+body+s[body_end:]
    start=s.index('async function runAuthoringSource')
    end=s.index('async function runCanonicalCallbackPhase',start)
    part=s[start:end]
    part=replace(part,'    return resultJson;\n  } finally {','    return resultJson;\n  } catch (error) {\n    throw attachAuthoringDiagnostic(error, globals);\n  } finally {')
    s=s[:start]+part+s[end:]
    s=replace(s,'    diagnostic: error instanceof Error && error.stack ? { stack: error.stack } : null,','    diagnostic: error instanceof Error && error.stack ? { stack: error.stack } : null,\n    authoringDiagnostic: error?.authoringDiagnostic ?? null,')
    anchor='''    if (!isRecord(request.context)) {
      throw new Error("Python authoring context must be an object");
    }
    return;'''
    s=replace(s,anchor,'''    if (!isRecord(request.context)) {
      throw new Error("Python authoring context must be an object");
    }
    const document = request.sourceDocument;
    if (document !== undefined && (!isRecord(document) ||
        typeof document.filename !== "string" || !document.filename || document.filename.length > 512 ||
        typeof document.revision !== "string" || !document.revision || document.revision.length > 128)) {
      throw new Error("Python authoring source document identity is invalid");
    }
    return;''')
    return s
edit('web/python-worker.source.js',worker)

def client(s):
    s='import { authoringErrorFromMessage } from "./authoring-error.js";\n\nlet nextDiagnosticClient = 0;\n'+s
    s=replace(s,'  #worker;','  #worker;\n  #diagnosticClient = nextDiagnosticClient++;')
    s=replace(s,'    const result = this.#resultFor(requestId, { onSemanticContinuation });','''    const sourceDocument = Object.freeze({ filename: "scene.py", revision: `${this.#diagnosticClient}/${requestId}` });
    const result = this.#resultFor(requestId, { onSemanticContinuation, source, sourceDocument });''')
    s=replace(s,'        requestId,\n        source,\n        context,','        requestId,\n        source,\n        context,\n        sourceDocument,')
    old='''        const details = [
          message.message,
          message.diagnostic?.stack,
        ].filter(Boolean);
        const error = new Error(String(details.join("\\n") || "Python authoring failed"));
        if (message.requestId === null) {
          this.#fail(error);
          return;
        }
        this.#settle(message.requestId, ({ reject }) => reject(error));'''
    new='''        if (message.requestId === null) {
          this.#fail(authoringErrorFromMessage(message));
          return;
        }
        this.#settle(message.requestId, (pending) => {
          pending.reject(authoringErrorFromMessage(message, pending));
        });'''
    return replace(s,old,new)
edit('web/authoring-client.js',client)

edit('web/authoring-client.test.mjs',lambda s: replace(s,'    context: { example: "scene" },\n  });','    context: { example: "scene" },\n    sourceDocument: worker.messages[0].sourceDocument,\n  });\n  assert.equal(worker.messages[0].sourceDocument.filename, "scene.py");\n  assert.equal(typeof worker.messages[0].sourceDocument.revision, "string");'))

edit('web/python-compat-modules.js',lambda s: replace(s,'  { sourcePath: "python/_noon_errors.py", runtimePath: "/tmp/_noon_errors.py", label: "Noon shared error projection" },','  { sourcePath: "python/_noon_errors.py", runtimePath: "/tmp/_noon_errors.py", label: "Noon shared error projection" },\n  { sourcePath: "python/_noon_diagnostics.py", runtimePath: "/tmp/_noon_diagnostics.py", label: "Noon authoring exception diagnostics" },'))

edit('web/index.html',lambda s: replace(s,'    <link rel="icon" href="data:," />','    <link rel="icon" href="data:," />\n    <link rel="stylesheet" href="./playground-diagnostics.css" />'))

def main(s):
    s='import { PlaygroundDiagnostics } from "./playground-diagnostics.js";\n'+s
    s=replace(s,'const sceneSourceEditor = document.querySelector("#python-scene-source");','''const sceneSourceEditor = document.querySelector("#python-scene-source");
const diagnosticPanel = new PlaygroundDiagnostics(
  sceneSourceEditor, document.querySelector("#python-diagnostics"), document.querySelector("#python-traceback"),
);''')
    s=replace(s,'  setRuntimeStatus(`Error: ${error}`, "error");\n  patchStatus.value = "Runtime failed";','  setRuntimeStatus("Preview unavailable · see error details", "error");\n  patchStatus.value = diagnosticPanel.show(error, "Preview unavailable");')
    start=s.index('function showSceneError(error) {'); end=s.index('\nfunction showRecoverableSceneError',start)
    s=s[:start]+'''function showSceneError(error) {
  resetPresentationRate();
  console.error(error);
  patchStatus.value = diagnosticPanel.show(error, "Python execution failed");
  patchStatus.dataset.state = "error";
}
'''+s[end:]
    s=replace(s,'  patchStatus.value = `Python callback failed: ${error}`;','  patchStatus.value = diagnosticPanel.show(error, "A callback failed");')
    s=replace(s,'  patchStatus.value = `Playback failed: ${error}`;','  patchStatus.value = diagnosticPanel.show(error, "Playback failed");')
    s=replace(s,'document.querySelector("#python-diagnostics").hidden = true;','diagnosticPanel.clear();',count=2)
    s=replace(s,'setRuntimeStatus("Python error · edit the source to retry", "ready");','setRuntimeStatus("Scene could not run · see error details", "ready");')
    s=replace(s,'      sourceRestart.dispose();','      sourceRestart.dispose();\n      diagnosticPanel.dispose();')
    return s
edit('web/main.js',main)

def view_model(s):
    anchor='  switch (error.engine?.category) {'
    return replace(s,anchor,'''  if (error.engine === null && diagnostic.phase === "dependencies") {
    title = "Python dependencies could not load";
    hint = "Inspect the failed dependency in the technical details, then retry. This may not require a source change.";
  } else if (error.module === "builtins" && ["SyntaxError", "IndentationError", "TabError"].includes(error.type)) {
    title = "Python syntax error";
    hint = "Correct the syntax at the reported source location, then run again.";
  }
  switch (error.engine?.category) {''')
edit('web/authoring-diagnostic.js',view_model)

def smoke(s):
    s=replace(s,'  assert.match(diagnostics.snapshots.syntaxError.patchText, /Python failed:/);','  assert.match(diagnostics.snapshots.syntaxError.patchText, /Python syntax error/);')
    s=replace(s,'  assert.match(diagnostics.snapshots.runtimeError.patchText, /Python failed:/);','  assert.match(diagnostics.snapshots.runtimeError.patchText, /Python raised NameError/);')
    helper='''async function assertSourceDiagnostic(page, line, selectedText = null) {
  await page.waitForSelector("#authoring-diagnostic:not([hidden])");
  assert.equal(await page.locator("#python-diagnostics").evaluate(node => node.open), false,
    "technical details should be collapsed by default");
  const link = page.locator("#authoring-diagnostic .diagnostic-source");
  assert.equal(await link.textContent(), `Go to scene.py:${line}`);
  await link.click();
  const selection = await page.evaluate(() => {
    const view = document.querySelector("#python-scene-source").editorView;
    const { from, to } = view.state.selection.main;
    return { line: view.state.doc.lineAt(from).number, text: view.state.doc.sliceString(from, to) };
  });
  assert.equal(selection.line, line);
  if (selectedText !== null) assert.equal(selection.text, selectedText);
}

async function assertDiagnosticCleared(page) {
  assert.equal(await page.locator("#authoring-diagnostic").isVisible(), false);
  assert.equal(await page.locator("#python-diagnostics").isVisible(), false);
}

'''
    s=replace(s,'async function assertStoppedPreview(page, observed) {',helper+'async function assertStoppedPreview(page, observed) {')
    s=replace(s,'  assert.match(diagnostics.snapshots.syntaxError.traceback, /SyntaxError/);','  assert.match(diagnostics.snapshots.syntaxError.traceback, /SyntaxError/);\n  await assertSourceDiagnostic(page, 3);')
    s=replace(s,'  assert.match(diagnostics.snapshots.runtimeError.traceback, /MissingShape/);','  assert.match(diagnostics.snapshots.runtimeError.traceback, /MissingShape/);\n  await assertSourceDiagnostic(page, 6, "MissingShape");')
    s=replace(s,'  assert.equal(diagnostics.snapshots.syntaxRecovered.rendererBackend, "WebGL2");','  assert.equal(diagnostics.snapshots.syntaxRecovered.rendererBackend, "WebGL2");\n  await assertDiagnosticCleared(page);')
    s=replace(s,'  assert.equal(diagnostics.snapshots.runtimeRecovered.rendererBackend, "WebGL2");','  assert.equal(diagnostics.snapshots.runtimeRecovered.rendererBackend, "WebGL2");\n  await assertDiagnosticCleared(page);')
    return s
edit('scripts/playground-authoring-error-recovery-smoke.mjs',smoke)

# This helper exists only on the branch-restricted patch-application branch.
# The published product commit has the original PR head as its direct parent.
NAMES = [
    'web/authoring-error.js', 'web/authoring-error.test.mjs',
    'web/playground-diagnostics.js', 'web/playground-diagnostics.css',
    'web/playground-diagnostics.test.mjs', 'web/python/test_noon_worker_diagnostics.py',
]
payload = Path(os.environ['NOON_DIAGNOSTIC_PAYLOAD_DIR'])
for name in NAMES:
    path=ROOT/name
    assert not path.exists(), f'{name} already exists; refusing overwrite'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes((payload/name).read_bytes())
print('Applied #2047 production diagnostic integration to exact checked blobs.')

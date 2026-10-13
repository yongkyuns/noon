from pathlib import Path


def edit(name, old, new):
    path = Path(name)
    source = path.read_text()
    assert source.count(old) == 1, (name, old)
    path.write_text(source.replace(old, new))


edit('web/authoring-client.js', 'filename: "scene.py", revision: `${this.#diagnosticClient}/${requestId}`', 'filename: `scene-${this.#diagnosticClient}-${requestId}.py`, revision: `${this.#diagnosticClient}/${requestId}`')
edit('web/authoring-client.test.mjs', 'assert.equal(worker.messages[0].sourceDocument.filename, "scene.py");', 'assert.match(worker.messages[0].sourceDocument.filename, /^scene-\\d+-\\d+\\.py$/);')
edit('web/python-worker.source.js', 'filename: "scene.py", revision: `request-${requestId}`', 'filename: `scene-request-${requestId}.py`, revision: `request-${requestId}`')
edit('web/authoring-error.test.mjs', '  assert.notEqual(documents[0].revision, documents[1].revision);', '  assert.notEqual(documents[0].revision, documents[1].revision);\n  assert.notEqual(documents[0].filename, documents[1].filename);')
edit('scripts/playground-authoring-error-recovery-smoke.mjs', '  assert.equal(await link.textContent(), `Go to scene.py:${line}`);', '  assert.match(await link.textContent(), new RegExp(`^Go to scene-\\\\d+-\\\\d+\\\\.py:${line}$`));')
# A callback from older compiled code may still be referenced by an imported
# module. Only the new run's callsite can claim this run's source location.
path = Path('web/python/test_noon_worker_diagnostics.py')
s = path.read_text()
marker = '    async def test_typed_engine_failure_keeps_its_metadata(self):\n'
assert s.count(marker) == 1
s = s.replace(marker, '''    async def test_retained_old_function_does_not_claim_new_source_location(self):
        namespace = {}
        exec(compile("def old():\\n    missing\\n", "scene-old.py", "exec"), namespace)
        sys.modules["noon"].old = namespace["old"]
        error, diagnostic, _ = await self.execute("from noon import old\\nold()\\n")
        self.assertIsInstance(error, NameError)
        self.assertEqual(diagnostic["exception"]["location"]["line"], 2)
        self.assertTrue(any(frame["filename"] == "scene-old.py" and not frame["userSource"]
                            for frame in diagnostic["exception"]["frames"]))

''' + marker)
path.write_text(s)

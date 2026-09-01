from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"{path}: expected exactly one patch anchor, found {text.count(old)}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "web/python-worker.source.js",
    '''    if (result.retained_document !== null) {
      const retainedDocumentJson = JSON.stringify(result.retained_document);
      validateRetainedAuthoringDocumentJson(retainedDocumentJson);
      if (result.retained_document.objects.length > 0) {
        result.scene_spec = JSON.parse(
          canonicalRetainedSceneSpecJson(JSON.stringify(result.document), retainedDocumentJson),
        );
      }
    }
''',
    '''    if (result.retained_document !== null) {
      const retainedDocumentJson = JSON.stringify(result.retained_document);
      validateRetainedAuthoringDocumentJson(retainedDocumentJson);
      result.scene_spec = JSON.parse(
        canonicalRetainedSceneSpecJson(JSON.stringify(result.document), retainedDocumentJson),
      );
    }
''',
)

replace_once(
    "web/authoring-client.js",
    '''    const sceneSpec = validateSceneSpec(result.scene_spec);
    const parsed = {
      kind: result.kind,
      document,
      duration: validateSceneDuration(result.duration),
      identities: validateSceneIdentities(result.identities, document),
      callbacks: validateCallbackSession(result.callbacks, document),
    };
    if (retainedDocument !== null) {
      parsed.retainedDocument = retainedDocument;
    }
    if (sceneSpec !== null) {
      parsed.sceneSpec = sceneSpec;
    }
    return parsed;
''',
    '''    const sceneSpec = validateSceneSpec(result.scene_spec);
    if (sceneSpec === null) {
      throw new Error("Python Scene result must include canonical SceneSpec");
    }
    const parsed = {
      kind: result.kind,
      document,
      sceneSpec,
      duration: validateSceneDuration(result.duration),
      identities: validateSceneIdentities(result.identities, document),
      callbacks: validateCallbackSession(result.callbacks, document),
    };
    if (retainedDocument !== null) {
      parsed.retainedDocument = retainedDocument;
    }
    return parsed;
''',
)

replace_once(
    "web/authoring-scene-spec-result.test.mjs",
    '''test("geometry-only scene results remain unchanged while the SceneSpec migration is retained-only", () => {
  const document = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      retained_document: {
        channel: "noon.authoring.retained",
        protocol_version: 2,
        objects: [],
      },
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );

  assert.equal("sceneSpec" in parsed, false);
  assert.equal(parsed.retainedDocument.objects.length, 0);
});
''',
    '''test("geometry-only scene results carry canonical SceneSpec beside the empty compatibility sidecar", () => {
  const document = { version: 1, objects: [], tracks: [] };
  const sceneSpec = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document,
      retained_document: {
        channel: "noon.authoring.retained",
        protocol_version: 2,
        objects: [],
      },
      scene_spec: sceneSpec,
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );

  assert.deepEqual(parsed.sceneSpec, sceneSpec);
  assert.equal(parsed.retainedDocument.objects.length, 0);
});
''',
)

replace_once(
    "web/authoring-scene-spec-result.test.mjs",
    '''  assert.match(
    source,
    /canonicalRetainedSceneSpecJson\\(JSON\\.stringify\\(result\\.document\\), retainedDocumentJson\\)/,
  );
});
''',
    '''  assert.match(
    source,
    /canonicalRetainedSceneSpecJson\\(JSON\\.stringify\\(result\\.document\\), retainedDocumentJson\\)/,
  );
  assert.doesNotMatch(source, /retained_document\\.objects\\.length/);
});
''',
)

client_test = Path("web/authoring-client.test.mjs")
text = client_test.read_text()
old = '''  const retained = retainedDocument();
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({
        kind: "scene_document",
        document: scene,
        retained_document: retained,
        duration: 2.75,
        identities,
        callbacks,
      }),
    }),
  );

  assert.deepEqual(await resultPromise, {
    kind: "scene_document",
    document: scene,
    retainedDocument: retained,
    duration: 2.75,
    identities,
    callbacks,
  });
'''
new = '''  const retained = retainedDocument();
  const sceneSpec = {
    version: 1,
    objects: [
      { id: 0, content: { kind: "geometry" } },
      { id: 2 ** 52, content: { kind: "text" } },
    ],
    tracks: [],
  };
  worker.emit(
    "message",
    workerMessage("result", {
      requestId: 0,
      resultJson: JSON.stringify({
        kind: "scene_document",
        document: scene,
        retained_document: retained,
        scene_spec: sceneSpec,
        duration: 2.75,
        identities,
        callbacks,
      }),
    }),
  );

  assert.deepEqual(await resultPromise, {
    kind: "scene_document",
    document: scene,
    sceneSpec,
    retainedDocument: retained,
    duration: 2.75,
    identities,
    callbacks,
  });
'''
if text.count(old) != 1:
    raise RuntimeError("authoring-client metadata test anchor must match exactly once")
text = text.replace(old, new, 1)
old = '''test("older scene results without a retained sidecar remain compatible", () => {
  const scene = { version: 1, objects: [], tracks: [] };
  const parsed = parseAuthoringResult(
    JSON.stringify({
      kind: "scene_document",
      document: scene,
      duration: 0,
      identities: { objects: [], tracks: [] },
      callbacks: null,
    }),
  );
  assert.equal("retainedDocument" in parsed, false);
});
'''
new = '''test("scene results without canonical SceneSpec are rejected at the current protocol boundary", () => {
  const scene = { version: 1, objects: [], tracks: [] };
  assert.throws(
    () =>
      parseAuthoringResult(
        JSON.stringify({
          kind: "scene_document",
          document: scene,
          duration: 0,
          identities: { objects: [], tracks: [] },
          callbacks: null,
        }),
      ),
    /must include canonical SceneSpec/,
  );
});
'''
if text.count(old) != 1:
    raise RuntimeError("authoring-client legacy-result test anchor must match exactly once")
text = text.replace(old, new, 1)
old = '''  const sceneResult = {
    kind: "scene_document",
    document: { version: 1, objects: [], tracks: [] },
    identities: { objects: [], tracks: [] },
    callbacks: null,
  };
'''
new = '''  const sceneResult = {
    kind: "scene_document",
    document: { version: 1, objects: [], tracks: [] },
    scene_spec: { version: 1, objects: [], tracks: [] },
    identities: { objects: [], tracks: [] },
    callbacks: null,
  };
'''
if text.count(old) != 1:
    raise RuntimeError("authoring-client duration test anchor must match exactly once")
client_test.write_text(text.replace(old, new, 1))

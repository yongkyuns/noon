export class SceneIdentityMap {
  #objects = new IdentityNamespace("object");
  #tracks = new IdentityNamespace("track");

  stabilize(document, identities) {
    const objectIds = this.#objects.resolve(identities.objects);
    const trackIds = this.#tracks.resolve(identities.tracks);
    if (objectIds === null && trackIds === null) {
      return document;
    }

    const objects =
      objectIds === null
        ? document.objects
        : document.objects.map((object) => {
            const id = requiredId(objectIds, object.id, "object");
            return id === object.id ? object : { ...object, id };
          });
    const tracks =
      objectIds === null && trackIds === null
        ? document.tracks
        : document.tracks.map((track) => {
            const id = trackIds === null ? track.id : requiredId(trackIds, track.id, "track");
            const object =
              objectIds === null
                ? track.object
                : requiredId(objectIds, track.object, "track object");
            return id === track.id && object === track.object
              ? track
              : { ...track, id, object };
          });
    const reactive = remapReactiveObjects(document.reactive, objectIds);

    return objects === document.objects && tracks === document.tracks && reactive === document.reactive
      ? document
      : { ...document, objects, tracks, ...(reactive === undefined ? {} : { reactive }) };
  }

  /// Apply the same semantic identity map to the canonical mixed scene.
  ///
  /// Python identity metadata currently describes the legacy geometry objects/tracks
  /// that feed SceneSpec construction. Canonical-only objects such as retained text
  /// are intentionally absent from that metadata and therefore keep their source IDs.
  /// Painter order stays structural while geometry track/camera references follow the
  /// remapped stable object IDs.
  stabilizeSceneSpec(sceneSpec, identities) {
    if (sceneSpec === null || sceneSpec === undefined) {
      return sceneSpec;
    }
    const objectIds = this.#objects.resolve(identities.objects);
    const trackIds = this.#tracks.resolve(identities.tracks);
    if (objectIds === null && trackIds === null) {
      return sceneSpec;
    }

    const objects =
      objectIds === null
        ? sceneSpec.objects
        : sceneSpec.objects.map((object) => {
            const id = optionalId(objectIds, object.id);
            return id === object.id ? object : { ...object, id };
          });
    const tracks =
      objectIds === null && trackIds === null
        ? sceneSpec.tracks
        : sceneSpec.tracks.map((track) => {
            const id = trackIds === null ? track.id : optionalId(trackIds, track.id);
            const object = objectIds === null ? track.object : optionalId(objectIds, track.object);
            return id === track.id && object === track.object
              ? track
              : { ...track, id, object };
          });
    const cameraObject =
      objectIds === null || sceneSpec.camera_object === null || sceneSpec.camera_object === undefined
        ? sceneSpec.camera_object
        : optionalId(objectIds, sceneSpec.camera_object);

    return objects === sceneSpec.objects &&
      tracks === sceneSpec.tracks &&
      cameraObject === sceneSpec.camera_object
      ? sceneSpec
      : { ...sceneSpec, objects, tracks, camera_object: cameraObject };
  }
}

function remapReactiveObjects(reactive, objectIds) {
  if (objectIds === null || reactive === undefined || reactive === null) {
    return reactive;
  }
  if (!Array.isArray(reactive.bindings)) {
    return reactive;
  }
  let changed = false;
  const bindings = reactive.bindings.map((binding) => {
    const object = requiredId(objectIds, binding.object, "reactive binding object");
    if (object === binding.object) {
      return binding;
    }
    changed = true;
    return { ...binding, object };
  });
  return changed ? { ...reactive, bindings } : reactive;
}

class IdentityNamespace {
  #kind;
  #keyToId = new Map();
  #idToKey = new Map();
  #nextId = 0;

  constructor(kind) {
    this.#kind = kind;
  }

  resolve(entries) {
    let localToStable = null;
    for (const { id: localId, key } of entries) {
      let stableId = this.#keyToId.get(key);
      if (stableId === undefined) {
        stableId = this.#claim(localId, key);
      }
      if (stableId !== localId) {
        localToStable ??= new Map();
      }
      localToStable?.set(localId, stableId);
    }
    if (localToStable === null) {
      return null;
    }

    // Once one entry needs remapping, callers still need identity mappings for
    // every other local ID referenced by the same document.
    for (const { id: localId, key } of entries) {
      if (!localToStable.has(localId)) {
        localToStable.set(localId, this.#keyToId.get(key));
      }
    }
    return localToStable;
  }

  #claim(preferredId, key) {
    let id = preferredId;
    if (this.#idToKey.has(id)) {
      id = this.#nextId;
      while (this.#idToKey.has(id)) {
        id += 1;
      }
    }
    if (!Number.isSafeInteger(id)) {
      throw new Error(`No safe ${this.#kind} identity IDs remain`);
    }
    this.#keyToId.set(key, id);
    this.#idToKey.set(id, key);
    this.#nextId = Math.max(this.#nextId, id + 1);
    return id;
  }
}

function requiredId(ids, localId, kind) {
  const stableId = ids.get(localId);
  if (stableId === undefined) {
    throw new Error(`Scene ${kind} ${localId} has no authoring identity`);
  }
  return stableId;
}

function optionalId(ids, localId) {
  return ids.get(localId) ?? localId;
}

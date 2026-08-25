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

    return objects === document.objects && tracks === document.tracks
      ? document
      : { ...document, objects, tracks };
  }
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

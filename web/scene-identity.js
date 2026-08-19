export class SceneIdentityMap {
  #objects = new IdentityNamespace("object");
  #tracks = new IdentityNamespace("track");

  stabilize(document, identities) {
    const objectIds = this.#objects.resolve(identities.objects);
    const trackIds = this.#tracks.resolve(identities.tracks);
    return {
      ...document,
      objects: document.objects.map((object) => ({
        ...object,
        id: requiredId(objectIds, object.id, "object"),
      })),
      tracks: document.tracks.map((track) => ({
        ...track,
        id: requiredId(trackIds, track.id, "track"),
        object: requiredId(objectIds, track.object, "track object"),
      })),
    };
  }
}

export function diffSceneDocuments(current, desired) {
  const currentObjects = byId(current.objects);
  const desiredObjects = byId(desired.objects);
  if (!appendCompatible(current.objects, desired.objects)) {
    return null;
  }
  for (const object of desired.objects) {
    const existing = currentObjects.get(object.id);
    if (existing && !same(existing.geometry, object.geometry)) {
      return null;
    }
  }

  const currentTracks = byId(current.tracks);
  const desiredTracks = byId(desired.tracks);
  if (!appendCompatible(current.tracks, desired.tracks)) {
    return null;
  }
  const removedObjects = new Set(
    current.objects
      .filter(({ id }) => !desiredObjects.has(id))
      .map(({ id }) => id),
  );
  const patches = [];
  for (const track of current.tracks) {
    if (!desiredTracks.has(track.id) && !removedObjects.has(track.object)) {
      patches.push({ remove_track: track.id });
    }
  }
  for (const id of removedObjects) {
    patches.push({ remove_object: id });
  }
  for (const object of desired.objects) {
    const existing = currentObjects.get(object.id);
    if (!existing) {
      patches.push({ create_object: object });
    } else {
      if (!same(existing.transform, object.transform)) {
        patches.push({ set_transform: { object: object.id, transform: object.transform } });
      }
      if (!same(existing.style, object.style)) {
        patches.push({ set_style: { object: object.id, style: object.style } });
      }
    }
  }
  for (const track of desired.tracks) {
    const existing = currentTracks.get(track.id);
    if (!existing) {
      patches.push({ add_track: track });
    } else if (!same(existing, track)) {
      patches.push({ replace_track: track });
    }
  }
  return patches;
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
    const localToStable = new Map();
    for (const { id: localId, key } of entries) {
      let stableId = this.#keyToId.get(key);
      if (stableId === undefined) {
        stableId = this.#claim(localId, key);
      }
      localToStable.set(localId, stableId);
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

function byId(definitions) {
  return new Map(definitions.map((definition) => [definition.id, definition]));
}

function appendCompatible(current, desired) {
  const currentIds = new Set(current.map(({ id }) => id));
  const desiredIds = new Set(desired.map(({ id }) => id));
  const retained = current.filter(({ id }) => desiredIds.has(id)).map(({ id }) => id);
  const desiredExisting = desired
    .filter(({ id }) => currentIds.has(id))
    .map(({ id }) => id);
  return (
    same(retained, desiredExisting) &&
    same(
      desired.slice(0, retained.length).map(({ id }) => id),
      retained,
    )
  );
}

function same(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

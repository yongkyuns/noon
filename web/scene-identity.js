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
  /// Python identity metadata currently describes only the geometry objects/tracks
  /// that feed SceneSpec construction. Canonical-only identities, such as retained
  /// Text tracks, are therefore provisional local IDs rather than stable semantic
  /// claims. Keyed semantic identities permanently own their stable IDs. Unkeyed
  /// canonical identities keep their source IDs when collision-free and are moved to
  /// a temporary free ID when a topology edit would otherwise alias a stable claim.
  /// This preserves one scene-global numeric domain without relying on disjoint ranges.
  stabilizeSceneSpec(sceneSpec, identities) {
    if (sceneSpec === null || sceneSpec === undefined) {
      return sceneSpec;
    }
    const objectIds = this.#objects.resolveCanonical(
      identities.objects,
      sceneSpec.objects.map(({ id }) => id),
    );
    const trackIds = this.#tracks.resolveCanonical(
      identities.tracks,
      sceneSpec.tracks.map(({ id }) => id),
    );
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
    const familyAnimations = remapFamilyAnimationObjects(sceneSpec.family_animations, objectIds);
    const cameraObject =
      objectIds === null || sceneSpec.camera_object === null || sceneSpec.camera_object === undefined
        ? sceneSpec.camera_object
        : optionalId(objectIds, sceneSpec.camera_object);

    return objects === sceneSpec.objects &&
      tracks === sceneSpec.tracks &&
      familyAnimations === sceneSpec.family_animations &&
      cameraObject === sceneSpec.camera_object
      ? sceneSpec
      : {
          ...sceneSpec,
          objects,
          tracks,
          ...(familyAnimations === sceneSpec.family_animations
            ? {}
            : { family_animations: familyAnimations }),
          camera_object: cameraObject,
        };
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

function remapFamilyAnimationObjects(familyAnimations, objectIds) {
  if (objectIds === null || !Array.isArray(familyAnimations)) {
    return familyAnimations;
  }
  let changed = false;
  const remapped = familyAnimations.map((animation) => {
    if (!Array.isArray(animation.bindings)) {
      return animation;
    }
    let animationChanged = false;
    const bindings = animation.bindings.map((binding) => {
      const object = optionalId(objectIds, binding.object);
      if (object === binding.object) {
        return binding;
      }
      animationChanged = true;
      return { ...binding, object };
    });
    if (!animationChanged) {
      return animation;
    }
    changed = true;
    return { ...animation, bindings };
  });
  return changed ? remapped : familyAnimations;
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
    const localToStable = this.#resolveSemantic(entries);
    for (const { id: localId } of entries) {
      if (localToStable.get(localId) !== localId) {
        return localToStable;
      }
    }
    return null;
  }

  /// Resolve a canonical mixed ID domain containing both keyed semantic entries and
  /// unkeyed canonical-only entries. Stable keyed claims win permanently; unkeyed
  /// entries are displaced only when necessary and their temporary IDs are not
  /// persisted as semantic claims.
  resolveCanonical(entries, localIds) {
    const localToStable = this.#resolveSemantic(entries);
    let changed = false;
    for (const { id: localId } of entries) {
      if (localToStable.get(localId) !== localId) {
        changed = true;
      }
    }

    const currentLocalIds = new Set(localIds);
    const usedIds = new Set(localToStable.values());
    for (const localId of localIds) {
      if (localToStable.has(localId)) {
        continue;
      }
      let stableId = localId;
      if (this.#idToKey.has(stableId) || usedIds.has(stableId)) {
        stableId = this.#temporaryId(currentLocalIds, usedIds);
        changed = true;
      }
      localToStable.set(localId, stableId);
      usedIds.add(stableId);
    }

    return changed ? localToStable : null;
  }

  #resolveSemantic(entries) {
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
    this.#assertSafeId(id);
    this.#keyToId.set(key, id);
    this.#idToKey.set(id, key);
    this.#nextId = Math.max(this.#nextId, id + 1);
    return id;
  }

  #temporaryId(currentLocalIds, usedIds) {
    let id = this.#nextId;
    while (this.#idToKey.has(id) || currentLocalIds.has(id) || usedIds.has(id)) {
      id += 1;
    }
    this.#assertSafeId(id);
    return id;
  }

  #assertSafeId(id) {
    if (!Number.isSafeInteger(id)) {
      throw new Error(`No safe ${this.#kind} identity IDs remain`);
    }
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

export class SceneIdentityMap {
  #objects = new IdentityNamespace("object");
  #tracks = new IdentityNamespace("track");

  stabilize(document, identities) {
    const objectIds = this.#objects.resolve(identities.objects);
    const trackIds = this.#tracks.resolve(identities.tracks);
    return {
      ...document,
      objects: document.objects.map((object) => {
        const id = requiredId(objectIds, object.id, "object");
        return id === object.id ? object : { ...object, id };
      }),
      tracks: document.tracks.map((track) => {
        const id = requiredId(trackIds, track.id, "track");
        const object = requiredId(objectIds, track.object, "track object");
        return id === track.id && object === track.object
          ? track
          : { ...track, id, object };
      }),
    };
  }
}

export function diffSceneDocuments(current, desired) {
  const currentObjects = byId(current.objects);
  const desiredObjects = byId(desired.objects);
  if (!appendCompatible(current.objects, desired.objects, desiredObjects)) {
    return null;
  }
  for (const object of desired.objects) {
    const existing = currentObjects.get(object.id);
    if (existing && !sameGeometry(existing.geometry, object.geometry)) {
      return null;
    }
  }

  const currentTracks = byId(current.tracks);
  const desiredTracks = byId(desired.tracks);
  if (!appendCompatible(current.tracks, desired.tracks, desiredTracks)) {
    return null;
  }
  const removedObjects = new Set();
  for (const { id } of current.objects) {
    if (!desiredObjects.has(id)) {
      removedObjects.add(id);
    }
  }
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
      if (!sameTransform(existing.transform, object.transform)) {
        patches.push({ set_transform: { object: object.id, transform: object.transform } });
      }
      if (!sameStyle(existing.style, object.style)) {
        patches.push({ set_style: { object: object.id, style: object.style } });
      }
    }
  }
  for (const track of desired.tracks) {
    const existing = currentTracks.get(track.id);
    if (!existing) {
      patches.push({ add_track: track });
    } else if (!sameTrack(existing, track)) {
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

function appendCompatible(current, desired, desiredById) {
  let retainedIndex = 0;
  for (const { id } of current) {
    if (desiredById.has(id)) {
      if (desired[retainedIndex]?.id !== id) {
        return false;
      }
      retainedIndex += 1;
    }
  }
  return true;
}

function sameGeometry(left, right) {
  if (left === right) {
    return true;
  }
  if (!isRecord(left) || !isRecord(right)) {
    return false;
  }
  if ("circle" in left || "circle" in right) {
    return (
      isRecord(left.circle) &&
      isRecord(right.circle) &&
      left.circle.radius === right.circle.radius
    );
  }
  if ("rectangle" in left || "rectangle" in right) {
    return (
      isRecord(left.rectangle) &&
      isRecord(right.rectangle) &&
      sameVec2(left.rectangle.size, right.rectangle.size)
    );
  }
  if ("line" in left || "line" in right) {
    return (
      isRecord(left.line) &&
      isRecord(right.line) &&
      sameVec2(left.line.start, right.line.start) &&
      sameVec2(left.line.end, right.line.end)
    );
  }
  if ("vector_path" in left || "vector_path" in right) {
    return sameVectorPath(left.vector_path, right.vector_path);
  }
  return "external" in left && "external" in right && left.external === right.external;
}

function sameVectorPath(left, right) {
  if (!isRecord(left) || !isRecord(right)) {
    return false;
  }
  if (!Array.isArray(left.commands) || !Array.isArray(right.commands)) {
    return false;
  }
  return (
    left.commands.length === right.commands.length &&
    left.commands.every((command, index) => samePathCommand(command, right.commands[index]))
  );
}

function samePathCommand(left, right) {
  if (left === right) {
    return true;
  }
  if (!isRecord(left) || !isRecord(right)) {
    return false;
  }
  for (const name of ["move_to", "line_to"]) {
    if (name in left || name in right) {
      return isRecord(left[name]) && isRecord(right[name]) && sameVec2(left[name].to, right[name].to);
    }
  }
  if ("quadratic_to" in left || "quadratic_to" in right) {
    return (
      isRecord(left.quadratic_to) &&
      isRecord(right.quadratic_to) &&
      sameVec2(left.quadratic_to.control, right.quadratic_to.control) &&
      sameVec2(left.quadratic_to.to, right.quadratic_to.to)
    );
  }
  if ("cubic_to" in left || "cubic_to" in right) {
    return (
      isRecord(left.cubic_to) &&
      isRecord(right.cubic_to) &&
      sameVec2(left.cubic_to.control1, right.cubic_to.control1) &&
      sameVec2(left.cubic_to.control2, right.cubic_to.control2) &&
      sameVec2(left.cubic_to.to, right.cubic_to.to)
    );
  }
  return false;
}

function sameTransform(left, right) {
  return (
    left === right ||
    (isRecord(left) &&
      isRecord(right) &&
      sameVec2(left.translation, right.translation) &&
      left.rotation === right.rotation &&
      sameVec2(left.scale, right.scale))
  );
}

function sameStyle(left, right) {
  return (
    left === right ||
    (isRecord(left) &&
      isRecord(right) &&
      sameColor(left.fill, right.fill) &&
      sameColor(left.stroke, right.stroke) &&
      left.stroke_width === right.stroke_width &&
      left.opacity === right.opacity)
  );
}

function sameTrack(left, right) {
  return (
    left === right ||
    (isRecord(left) &&
      isRecord(right) &&
      isRecord(left.timing) &&
      isRecord(right.timing) &&
      left.id === right.id &&
      left.object === right.object &&
      left.property === right.property &&
      sameTrackValues(left.values, right.values) &&
      left.timing.start_time === right.timing.start_time &&
      left.timing.duration === right.timing.duration &&
      left.timing.easing === right.timing.easing)
  );
}

function sameTrackValues(left, right) {
  if (left === right) {
    return true;
  }
  if (!isRecord(left) || !isRecord(right)) {
    return false;
  }
  if ("scalar" in left || "scalar" in right) {
    return (
      isRecord(left.scalar) &&
      isRecord(right.scalar) &&
      left.scalar.from === right.scalar.from &&
      left.scalar.to === right.scalar.to
    );
  }
  return (
    isRecord(left.vec2) &&
    isRecord(right.vec2) &&
    sameVec2(left.vec2.from, right.vec2.from) &&
    sameVec2(left.vec2.to, right.vec2.to)
  );
}

function sameVec2(left, right) {
  return (
    left === right ||
    (isRecord(left) && isRecord(right) && left.x === right.x && left.y === right.y)
  );
}

function sameColor(left, right) {
  return (
    left === right ||
    (isRecord(left) &&
      isRecord(right) &&
      left.red === right.red &&
      left.green === right.green &&
      left.blue === right.blue &&
      left.alpha === right.alpha)
  );
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

// Temporary compatibility projection for the legacy EngineScenePlayer path.
//
// Native reactive scenes remain authoritative and valid: this module only derives
// ordinary position tracks from the narrow ValueTracker position expression emitted
// by the Python compatibility layer immediately before a scene is sent to the legacy
// execution worker. Remove this projection once EngineScenePlayer consumes
// TimedSemanticScene / signal_tracks directly.

export function projectLegacyReactiveSceneJson(sceneJson) {
  let document;
  try {
    document = JSON.parse(sceneJson);
  } catch {
    // Keep validation/error ownership with the engine for malformed scene JSON.
    return sceneJson;
  }
  const projected = projectLegacyReactiveScene(document);
  return projected === document ? sceneJson : JSON.stringify(projected);
}

export function projectLegacyReactiveScene(document) {
  if (!isRecord(document) || !Array.isArray(document.objects) || !Array.isArray(document.tracks)) {
    return document;
  }
  const reactive = document.reactive;
  const signalTracks = document.signal_tracks;
  if (
    !isRecord(reactive) ||
    !Array.isArray(reactive.signals) ||
    !Array.isArray(reactive.bindings) ||
    !Array.isArray(signalTracks)
  ) {
    return document;
  }

  const signals = new Map(reactive.signals.map((signal) => [signal.id, signal]));
  let objects = document.objects;
  let tracks = document.tracks;
  let changed = false;
  let nextTrackId = nextSafeTrackId(tracks);

  for (const binding of reactive.bindings) {
    if (binding?.property !== "position") {
      continue;
    }
    // A semantic scene that already drives Position on the ordinary timeline is
    // intentionally left alone. The native compiler rejects duplicate drivers;
    // this also makes the projection idempotent across execution-client restarts.
    if (tracks.some((track) => track.object === binding.object && track.property === "position")) {
      continue;
    }

    const position = decodeValueTrackerPosition(signals, binding.signal);
    if (position === null) {
      continue;
    }

    const authoredTracks = signalTracks.filter((track) => track.signal === position.trackerSignal);
    const initial = evaluatePosition(position.offset, position.direction, position.initialValue);
    const objectIndex = objects.findIndex((object) => object.id === binding.object);
    if (objectIndex < 0) {
      continue;
    }

    if (!changed) {
      objects = [...objects];
      tracks = [...tracks];
      changed = true;
    }
    const object = objects[objectIndex];
    objects[objectIndex] = {
      ...object,
      transform: {
        ...object.transform,
        translation: initial,
      },
    };

    for (const signalTrack of authoredTracks) {
      if (!isFiniteNumber(signalTrack.from) || !isFiniteNumber(signalTrack.to)) {
        continue;
      }
      tracks.push({
        id: nextTrackId,
        object: binding.object,
        property: "position",
        values: {
          vec2: {
            from: evaluatePosition(position.offset, position.direction, signalTrack.from),
            to: evaluatePosition(position.offset, position.direction, signalTrack.to),
          },
        },
        timing: signalTrack.timing,
      });
      nextTrackId += 1;
      if (!Number.isSafeInteger(nextTrackId)) {
        throw new Error("legacy reactive projection exhausted safe track IDs");
      }
    }
  }

  return changed ? { ...document, objects, tracks } : document;
}

function decodeValueTrackerPosition(signals, derivedSignalId) {
  const derived = signals.get(derivedSignalId)?.source?.derived;
  const terms = derived?.add;
  if (!Array.isArray(terms) || terms.length !== 2) {
    return null;
  }
  const offset = constantVec2(terms[0]);
  const factors = terms[1]?.mul;
  if (offset === null || !Array.isArray(factors) || factors.length !== 2) {
    return null;
  }
  const trackerSignal = factors[0]?.signal;
  const direction = constantVec2(factors[1]);
  const initialValue = signals.get(trackerSignal)?.source?.input?.scalar;
  if (
    !Number.isSafeInteger(trackerSignal) ||
    direction === null ||
    !isFiniteNumber(initialValue)
  ) {
    return null;
  }
  return { trackerSignal, offset, direction, initialValue };
}

function constantVec2(expression) {
  const value = expression?.constant?.vec2;
  if (!isRecord(value) || !isFiniteNumber(value.x) || !isFiniteNumber(value.y)) {
    return null;
  }
  return { x: value.x, y: value.y };
}

function evaluatePosition(offset, direction, value) {
  return {
    x: offset.x + value * direction.x,
    y: offset.y + value * direction.y,
  };
}

function nextSafeTrackId(tracks) {
  let maximum = -1;
  for (const track of tracks) {
    if (Number.isSafeInteger(track?.id) && track.id >= 0) {
      maximum = Math.max(maximum, track.id);
    }
  }
  if (maximum >= Number.MAX_SAFE_INTEGER) {
    throw new Error("legacy reactive projection exhausted safe track IDs");
  }
  return maximum + 1;
}

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

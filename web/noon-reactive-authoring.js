import { ReactiveAuthoringSceneCore } from "./pkg/noon_web.js";
import { ORIGIN, RIGHT, Vec2, linear, smooth } from "./noon-authoring.js";

export { initNoon } from "./noon-authoring.js";
export { ORIGIN, RIGHT, Vec2, linear, smooth };

function asVec2(value) {
  if (value instanceof Vec2) return value;
  if (Array.isArray(value) && value.length === 2) {
    return new Vec2(value[0], value[1]);
  }
  throw new TypeError("expected Vec2 or [x, y]");
}

const valueAnimationMarker = Symbol("noonValueAnimation");

export class ReactiveMobject {
  constructor(scene, handle) {
    this._scene = scene;
    this._handle = handle;
  }
}

export class ValueTracker {
  constructor(scene, handle, value) {
    this._scene = scene;
    this._handle = handle;
    this._value = Number(value);
  }

  getValue() {
    return this._value;
  }

  animate() {
    return new ValueTrackerAnimation(this);
  }
}

export class ValueTrackerAnimation {
  constructor(tracker) {
    this[valueAnimationMarker] = true;
    this.tracker = tracker;
    this.targetValue = null;
  }

  setValue(value) {
    this.targetValue = Number(value);
    return this;
  }
}

export class ReactiveScene {
  constructor() {
    this._core = new ReactiveAuthoringSceneCore();
  }

  addCircle(radius = 1) {
    return new ReactiveMobject(this, this._core.addCircle(Number(radius)));
  }

  valueTracker(value = 0) {
    const numeric = Number(value);
    return new ValueTracker(this, this._core.valueTracker(numeric), numeric);
  }

  bindPosition(mobject, tracker, direction = RIGHT, offset = ORIGIN) {
    if (!(mobject instanceof ReactiveMobject) || mobject._scene !== this) {
      throw new Error("bindPosition mobject must belong to this ReactiveScene");
    }
    if (!(tracker instanceof ValueTracker) || tracker._scene !== this) {
      throw new Error("bindPosition tracker must belong to this ReactiveScene");
    }
    const axis = asVec2(direction);
    const origin = asVec2(offset);
    this._core.bindPositionFromTracker(
      mobject._handle,
      tracker._handle,
      axis.x,
      axis.y,
      origin.x,
      origin.y,
    );
    return this;
  }

  play(animation, options = {}) {
    if (!animation?.[valueAnimationMarker]) {
      throw new TypeError(
        "ReactiveScene.play currently supports one ValueTracker animation",
      );
    }
    if (animation.tracker._scene !== this) {
      throw new Error("ValueTracker animation belongs to a different ReactiveScene");
    }
    if (animation.targetValue === null) {
      throw new Error("ValueTracker.animate() must call setValue() before play()");
    }
    const runTime = options.runTime ?? options.run_time ?? 1;
    const rateFunc = options.rateFunc ?? options.rate_func ?? smooth;
    this._core.playValue(
      animation.tracker._handle,
      animation.targetValue,
      Number(runTime),
      String(rateFunc),
    );
    animation.tracker._value = animation.targetValue;
    return this;
  }

  wait(duration = 1) {
    this._core.wait(Number(duration));
    return this;
  }

  get time() {
    return this._core.time;
  }

  timedSceneJson() {
    return this._core.timedSceneJson();
  }

  toJSON() {
    return JSON.parse(this.timedSceneJson());
  }
}

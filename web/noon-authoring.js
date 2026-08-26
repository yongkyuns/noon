import initWasm, {
  AuthoringSceneCore,
  authoringCircle,
  authoringLine,
  authoringRectangle,
  authoringSquare,
} from "./pkg/noon_web.js";

export const initNoon = initWasm;

export class Vec2 {
  constructor(x = 0, y = 0) {
    this.x = Number(x);
    this.y = Number(y);
  }

  add(other) {
    return new Vec2(this.x + other.x, this.y + other.y);
  }

  sub(other) {
    return new Vec2(this.x - other.x, this.y - other.y);
  }

  mul(scalar) {
    const factor = Number(scalar);
    return new Vec2(this.x * factor, this.y * factor);
  }
}

export const ORIGIN = new Vec2(0, 0);
export const UP = new Vec2(0, 1);
export const DOWN = new Vec2(0, -1);
export const LEFT = new Vec2(-1, 0);
export const RIGHT = new Vec2(1, 0);
export const UL = UP.add(LEFT);
export const UR = UP.add(RIGHT);
export const DL = DOWN.add(LEFT);
export const DR = DOWN.add(RIGHT);

export const PI = Math.PI;
export const TAU = Math.PI * 2;
export const DEGREES = TAU / 360;
export const SMALL_BUFF = 0.1;
export const MED_SMALL_BUFF = 0.25;
export const MED_LARGE_BUFF = 0.5;
export const LARGE_BUFF = 1.0;
export const DEFAULT_MOBJECT_TO_EDGE_BUFFER = MED_LARGE_BUFF;
export const DEFAULT_MOBJECT_TO_MOBJECT_BUFFER = MED_SMALL_BUFF;

export class Color {
  constructor(red, green, blue, alpha = 1) {
    this.red = Number(red);
    this.green = Number(green);
    this.blue = Number(blue);
    this.alpha = Number(alpha);
  }

  static fromHex(hex) {
    const value = typeof hex === "string"
      ? Number.parseInt(hex.replace(/^#/, ""), 16)
      : Number(hex);
    if (!Number.isInteger(value) || value < 0 || value > 0xffffff) {
      throw new RangeError("hex color must be between 0x000000 and 0xFFFFFF");
    }
    return new Color(
      ((value >> 16) & 0xff) / 255,
      ((value >> 8) & 0xff) / 255,
      (value & 0xff) / 255,
      1,
    );
  }
}

export const WHITE = Color.fromHex(0xffffff);
export const BLACK = Color.fromHex(0x000000);
export const BLUE = Color.fromHex(0x58c4dd);
export const TEAL = Color.fromHex(0x5cd0b3);
export const GREEN = Color.fromHex(0x83c167);
export const YELLOW = Color.fromHex(0xf7d96f);
export const GOLD = Color.fromHex(0xf0ac5f);
export const RED = Color.fromHex(0xfc6255);
export const MAROON = Color.fromHex(0xc55f73);
export const PURPLE = Color.fromHex(0x9a72ac);
export const ORANGE = Color.fromHex(0xff862f);
export const PINK = Color.fromHex(0xd147bd);
export const LIGHT_PINK = Color.fromHex(0xdc75cd);
export const GRAY = Color.fromHex(0x888888);
export const GREY = GRAY;

export const linear = "linear";
export const smooth = "smooth";
export const rushInto = "rush_into";
export const rushFrom = "rush_from";
export const thereAndBack = "there_and_back";

function asVec2(value) {
  if (value instanceof Vec2) return value;
  if (Array.isArray(value) && value.length === 2) {
    return new Vec2(value[0], value[1]);
  }
  throw new TypeError("expected Vec2 or [x, y]");
}

function asColor(value) {
  if (value instanceof Color) return value;
  if (typeof value === "string" || Number.isInteger(value)) {
    return Color.fromHex(value);
  }
  throw new TypeError("expected Color or hex color");
}

export class Mobject {
  constructor(core) {
    this._core = core;
    this._scene = null;
    this._handle = null;
  }

  get isBound() {
    return this._scene !== null;
  }

  _bind(scene, handle) {
    if (this._scene !== null && this._scene !== scene) {
      throw new Error("Mobject already belongs to another Scene");
    }
    if (this._scene === scene) {
      throw new Error("Mobject is already bound to this Scene");
    }
    this._scene = scene;
    this._handle = handle;
  }

  _requireBound() {
    if (this._scene === null || this._handle === null) {
      throw new Error("Mobject must be added to a Scene first");
    }
  }

  _requireDetached() {
    if (this._scene !== null) {
      throw new Error("operation requires a detached Mobject");
    }
  }

  copy() {
    this._requireDetached();
    return new Mobject(this._core.cloneHandle());
  }

  shift(direction) {
    const value = asVec2(direction);
    if (this.isBound) {
      this._scene._core.shift(this._handle, value.x, value.y);
    } else {
      this._core.shift(value.x, value.y);
    }
    return this;
  }

  moveTo(point) {
    const value = asVec2(point);
    if (this.isBound) {
      this._scene._core.moveTo(this._handle, value.x, value.y);
    } else {
      this._core.moveTo(value.x, value.y);
    }
    return this;
  }

  scale(factor) {
    const value = Number(factor);
    if (this.isBound) {
      this._scene._core.scale(this._handle, value);
    } else {
      this._core.scale(value);
    }
    return this;
  }

  rotate(angle) {
    const value = Number(angle);
    if (this.isBound) {
      this._scene._core.rotate(this._handle, value);
    } else {
      this._core.rotate(value);
    }
    return this;
  }

  setColor(color) {
    const value = asColor(color);
    if (this.isBound) {
      this._scene._core.setColor(
        this._handle,
        value.red,
        value.green,
        value.blue,
        value.alpha,
      );
    } else {
      this._core.setColor(value.red, value.green, value.blue, value.alpha);
    }
    return this;
  }

  color(color) {
    return this.setColor(color);
  }

  setFill(color, opacity = 1) {
    this._requireDetached();
    const value = asColor(color);
    this._core.setFill(value.red, value.green, value.blue, Number(opacity));
    return this;
  }

  setOpacity(opacity) {
    const value = Number(opacity);
    if (this.isBound) {
      this._scene._core.setOpacity(this._handle, value);
    } else {
      this._core.setOpacity(value);
    }
    return this;
  }

  nextTo(other, direction = RIGHT, buff = DEFAULT_MOBJECT_TO_MOBJECT_BUFFER) {
    const axis = asVec2(direction);
    if (this.isBound || other.isBound) {
      this._requireBound();
      other._requireBound();
      if (this._scene !== other._scene) {
        throw new Error("nextTo objects must belong to the same Scene");
      }
      this._scene._core.nextTo(
        this._handle,
        other._handle,
        axis.x,
        axis.y,
        Number(buff),
      );
    } else {
      this._core.nextTo(other._core, axis.x, axis.y, Number(buff));
    }
    return this;
  }

  animate() {
    this._requireBound();
    return new AnimateBuilder(this, this._scene._core.animate(this._handle));
  }

  get center() {
    this._requireDetached();
    return new Vec2(this._core.centerX, this._core.centerY);
  }

  get width() {
    this._requireDetached();
    return this._core.width;
  }

  get height() {
    this._requireDetached();
    return this._core.height;
  }
}

export class Circle extends Mobject {
  constructor(radius = 1) {
    super(authoringCircle(Number(radius)));
  }
}

export class Square extends Mobject {
  constructor(sideLength = 2) {
    super(authoringSquare(Number(sideLength)));
  }
}

export class Rectangle extends Mobject {
  constructor(width, height) {
    super(authoringRectangle(Number(width), Number(height)));
  }
}

export class Line extends Mobject {
  constructor(start = LEFT, end = RIGHT) {
    const a = asVec2(start);
    const b = asVec2(end);
    super(authoringLine(a.x, a.y, b.x, b.y));
  }
}

const animationMarker = Symbol("noonAnimation");

class AnimationBase {
  constructor() {
    this[animationMarker] = true;
  }
}

export class AnimateBuilder extends AnimationBase {
  constructor(mobject, core) {
    super();
    this.mobject = mobject;
    this._core = core;
  }

  shift(direction) {
    const value = asVec2(direction);
    this._core.shift(value.x, value.y);
    return this;
  }

  moveTo(point) {
    const value = asVec2(point);
    this._core.moveTo(value.x, value.y);
    return this;
  }

  scale(factor) {
    this._core.scale(Number(factor));
    return this;
  }

  rotate(angle) {
    this._core.rotate(Number(angle));
    return this;
  }

  setColor(color) {
    const value = asColor(color);
    this._core.setColor(value.red, value.green, value.blue, value.alpha);
    return this;
  }

  setFill(color, opacity = 1) {
    const value = asColor(color);
    this._core.setFill(value.red, value.green, value.blue, Number(opacity));
    return this;
  }

  setOpacity(opacity) {
    this._core.setOpacity(Number(opacity));
    return this;
  }

  _append(scene, batch) {
    if (scene !== this.mobject._scene) {
      throw new Error("animation belongs to a different Scene");
    }
    scene._core.appendAnimate(batch, this._core);
  }
}

class MobjectAnimation extends AnimationBase {
  constructor(mobject) {
    super();
    mobject._requireBound();
    this.mobject = mobject;
  }
}

class CreateAnimation extends MobjectAnimation {
  _append(scene, batch) {
    scene._assertOwns(this.mobject);
    scene._core.appendCreate(batch, this.mobject._handle);
  }
}

class FadeOutAnimation extends MobjectAnimation {
  _append(scene, batch) {
    scene._assertOwns(this.mobject);
    scene._core.appendFadeOut(batch, this.mobject._handle);
  }
}

class FadeInAnimation extends MobjectAnimation {
  _append(scene, batch) {
    scene._assertOwns(this.mobject);
    scene._core.appendFadeIn(batch, this.mobject._handle);
  }
}

class TransformAnimation extends MobjectAnimation {
  constructor(source, target) {
    super(source);
    target._requireDetached();
    this.target = target;
  }

  _append(scene, batch) {
    scene._assertOwns(this.mobject);
    scene._core.appendTransform(batch, this.mobject._handle, this.target._core);
  }
}

class RotateAnimation extends MobjectAnimation {
  constructor(mobject, angle) {
    super(mobject);
    this.angle = Number(angle);
  }

  _append(scene, batch) {
    scene._assertOwns(this.mobject);
    scene._core.appendRotate(batch, this.mobject._handle, this.angle);
  }
}

export function Create(mobject) {
  return new CreateAnimation(mobject);
}

export function FadeOut(mobject) {
  return new FadeOutAnimation(mobject);
}

export function FadeIn(mobject) {
  return new FadeInAnimation(mobject);
}

export function Transform(source, target) {
  return new TransformAnimation(source, target);
}

export function Rotate(mobject, angle = PI) {
  return new RotateAnimation(mobject, angle);
}

export class Scene {
  constructor() {
    this._core = new AuthoringSceneCore();
  }

  _assertOwns(mobject) {
    mobject._requireBound();
    if (mobject._scene !== this) {
      throw new Error("Mobject belongs to a different Scene");
    }
  }

  add(...mobjects) {
    for (const mobject of mobjects) {
      if (!(mobject instanceof Mobject)) {
        throw new TypeError("Scene.add expects Mobject instances");
      }
      mobject._requireDetached();
      mobject._bind(this, this._core.add(mobject._core));
    }
    return this;
  }

  play(...items) {
    let options = {};
    if (
      items.length > 0
      && !(items.at(-1)?.[animationMarker])
      && typeof items.at(-1) === "object"
      && items.at(-1) !== null
    ) {
      options = items.pop();
    }
    if (items.length === 0) {
      throw new Error("Scene.play requires at least one animation");
    }
    for (const animation of items) {
      if (!animation?.[animationMarker]) {
        throw new TypeError("Scene.play expects animation objects followed by optional options");
      }
    }

    const batch = this._core.createPlayBatch();
    try {
      for (const animation of items) {
        animation._append(this, batch);
      }
      const runTime = options.runTime ?? options.run_time ?? 1;
      const rateFunc = options.rateFunc ?? options.rate_func ?? "";
      this._core.playBatch(batch, Number(runTime), String(rateFunc));
    } finally {
      batch.free();
    }
    return this;
  }

  wait(duration = 1) {
    this._core.wait(Number(duration));
    return this;
  }

  get time() {
    return this._core.time;
  }

  sceneJson() {
    return this._core.sceneJson();
  }

  toJSON() {
    return JSON.parse(this.sceneJson());
  }
}

export function initNoon(moduleOrPath?: unknown): Promise<unknown>;

export class Vec2 {
  constructor(x?: number, y?: number);
  readonly x: number;
  readonly y: number;
  add(other: Vec2): Vec2;
  sub(other: Vec2): Vec2;
  mul(scalar: number): Vec2;
}

export class Color {
  constructor(red: number, green: number, blue: number, alpha?: number);
  readonly red: number;
  readonly green: number;
  readonly blue: number;
  readonly alpha: number;
  static fromHex(hex: string | number): Color;
}

export const ORIGIN: Vec2;
export const UP: Vec2;
export const DOWN: Vec2;
export const LEFT: Vec2;
export const RIGHT: Vec2;
export const UL: Vec2;
export const UR: Vec2;
export const DL: Vec2;
export const DR: Vec2;

export const PI: number;
export const TAU: number;
export const DEGREES: number;
export const SMALL_BUFF: number;
export const MED_SMALL_BUFF: number;
export const MED_LARGE_BUFF: number;
export const LARGE_BUFF: number;
export const DEFAULT_MOBJECT_TO_EDGE_BUFFER: number;
export const DEFAULT_MOBJECT_TO_MOBJECT_BUFFER: number;

export const WHITE: Color;
export const BLACK: Color;
export const BLUE: Color;
export const TEAL: Color;
export const GREEN: Color;
export const YELLOW: Color;
export const GOLD: Color;
export const RED: Color;
export const MAROON: Color;
export const PURPLE: Color;
export const ORANGE: Color;
export const PINK: Color;
export const LIGHT_PINK: Color;
export const GRAY: Color;
export const GREY: Color;

export const linear: "linear";
export const smooth: "smooth";
export const rushInto: "rush_into";
export const rushFrom: "rush_from";
export const thereAndBack: "there_and_back";

export type VectorLike = Vec2 | readonly [number, number];
export type ColorLike = Color | string | number;
export type RateFunction =
  | "linear"
  | "smooth"
  | "rush_into"
  | "rush_from"
  | "there_and_back";

export interface PlayOptions {
  runTime?: number;
  run_time?: number;
  rateFunc?: RateFunction;
  rate_func?: RateFunction;
}

export class Mobject {
  readonly isBound: boolean;
  copy(): Mobject;
  shift(direction: VectorLike): this;
  moveTo(point: VectorLike): this;
  scale(factor: number): this;
  rotate(angle: number): this;
  setColor(color: ColorLike): this;
  color(color: ColorLike): this;
  setFill(color: ColorLike, opacity?: number): this;
  setOpacity(opacity: number): this;
  nextTo(other: Mobject, direction?: VectorLike, buff?: number): this;
  animate(): AnimateBuilder;
  readonly center: Vec2;
  readonly width: number;
  readonly height: number;
}

export class Circle extends Mobject {
  constructor(radius?: number);
}

export class Square extends Mobject {
  constructor(sideLength?: number);
}

export class Rectangle extends Mobject {
  constructor(width: number, height: number);
}

export class Line extends Mobject {
  constructor(start?: VectorLike, end?: VectorLike);
}

export interface Animation {
  readonly mobject?: Mobject;
}

export class AnimateBuilder implements Animation {
  readonly mobject: Mobject;
  shift(direction: VectorLike): this;
  moveTo(point: VectorLike): this;
  scale(factor: number): this;
  rotate(angle: number): this;
  setColor(color: ColorLike): this;
  setFill(color: ColorLike, opacity?: number): this;
  setOpacity(opacity: number): this;
}

export function Create(mobject: Mobject): Animation;
export function FadeOut(mobject: Mobject): Animation;
export function FadeIn(mobject: Mobject): Animation;
export function Transform(source: Mobject, target: Mobject): Animation;

export class Scene {
  constructor();
  add(...mobjects: Mobject[]): this;
  play(...items: Array<Animation | PlayOptions>): this;
  wait(duration?: number): this;
  readonly time: number;
  sceneJson(): string;
  toJSON(): Record<string, unknown>;
}

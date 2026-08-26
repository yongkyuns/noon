import type { Vec2, VectorLike, RateFunction } from "./noon-authoring.js";

export { initNoon } from "./noon-authoring.js";
export { ORIGIN, RIGHT, Vec2, linear, smooth } from "./noon-authoring.js";

export interface ReactivePlayOptions {
  runTime?: number;
  run_time?: number;
  rateFunc?: RateFunction;
  rate_func?: RateFunction;
}

export class ReactiveMobject {
  private constructor();
}

export class ValueTracker {
  private constructor();
  getValue(): number;
  animate(): ValueTrackerAnimation;
}

export class ValueTrackerAnimation {
  private constructor();
  readonly tracker: ValueTracker;
  readonly targetValue: number | null;
  setValue(value: number): this;
}

export class ReactiveScene {
  constructor();
  addCircle(radius?: number): ReactiveMobject;
  valueTracker(value?: number): ValueTracker;
  bindPosition(
    mobject: ReactiveMobject,
    tracker: ValueTracker,
    direction?: VectorLike,
    offset?: VectorLike,
  ): this;
  play(animation: ValueTrackerAnimation, options?: ReactivePlayOptions): this;
  wait(duration?: number): this;
  readonly time: number;
  timedSceneJson(): string;
  toJSON(): Record<string, unknown>;
}

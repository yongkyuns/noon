import {
  BLUE,
  GREEN,
  LEFT,
  PINK,
  PI,
  RIGHT,
  Circle,
  Create,
  FadeOut,
  Rotate,
  Scene,
  Square,
  Transform,
} from "../../noon-authoring.js";

export function createCircle() {
  const scene = new Scene();
  const circle = new Circle().setFill(PINK, 0.5);
  scene.add(circle);
  scene.play(Create(circle));
  return scene;
}

export function squareToCircle() {
  const scene = new Scene();
  const circle = new Circle().setFill(PINK, 0.5);
  const square = new Square().rotate(PI / 4);
  scene.add(square);
  scene.play(Create(square));
  scene.play(Transform(square, circle));
  scene.play(FadeOut(square));
  return scene;
}

export function squareAndCircle() {
  const scene = new Scene();
  const circle = new Circle().setFill(PINK, 0.5);
  const square = new Square().setFill(BLUE, 0.5);
  scene.add(circle, square);
  square.nextTo(circle, RIGHT, 0.5);
  scene.play(Create(circle), Create(square));
  return scene;
}

export function animatedSquareToCircle() {
  const scene = new Scene();
  const circle = new Circle();
  const square = new Square();
  scene.add(square);
  scene.play(Create(square));
  scene.play(square.animate().rotate(PI / 4));
  scene.play(Transform(square, circle));
  scene.play(square.animate().setFill(PINK, 0.5));
  return scene;
}

export function differentRotations() {
  const scene = new Scene();
  const leftSquare = new Square()
    .setColor(BLUE)
    .setFill(BLUE, 0.7)
    .shift(LEFT.mul(2));
  const rightSquare = new Square()
    .setColor(GREEN)
    .setFill(GREEN, 0.7)
    .shift(RIGHT.mul(2));
  scene.add(leftSquare, rightSquare);
  scene.play(
    leftSquare.animate().rotate(PI),
    Rotate(rightSquare, PI),
    { runTime: 2 },
  );
  scene.wait();
  return scene;
}

export const quickstartEquivalents = Object.freeze({
  CreateCircle: createCircle,
  SquareToCircle: squareToCircle,
  SquareAndCircle: squareAndCircle,
  AnimatedSquareToCircle: animatedSquareToCircle,
  DifferentRotations: differentRotations,
});

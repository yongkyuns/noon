import {
  BLUE,
  LEFT,
  ORANGE,
  Scene,
  Square,
} from "../../noon-authoring.js";

export function movingAround() {
  const scene = new Scene();
  const square = new Square().setColor(BLUE).setFill(BLUE, 1);
  scene.add(square);

  scene.play(square.animate().shift(LEFT));
  scene.play(square.animate().setFill(ORANGE));
  scene.play(square.animate().scale(0.3));
  scene.play(square.animate().rotate(0.4));
  return scene;
}

export const galleryMovingAround = {
  MovingAround: movingAround,
};

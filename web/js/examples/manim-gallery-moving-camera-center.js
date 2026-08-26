import {
  GREEN,
  LEFT,
  RED,
  RIGHT,
  Square,
} from "../../noon-authoring.js";
import { MovingCameraScene } from "../../noon-moving-camera.js";

/**
 * Browser-language equivalent of ManimCE's MovingCameraCenter camera semantics.
 *
 * The JS authoring surface does not yet expose a Path/Triangle constructor, so the
 * right-hand marker remains a square here. The camera frame timing and target centers
 * are identical to the upstream example and lower through the same Rust/WASM tracks.
 */
export function MovingCameraCenter() {
  const scene = new MovingCameraScene();
  const left = new Square()
    .setColor(RED)
    .setFill(RED, 0.5)
    .moveTo(LEFT.mul(2));
  const right = new Square()
    .setColor(GREEN)
    .setFill(GREEN, 0.5)
    .moveTo(RIGHT.mul(2));

  scene.wait(0.3);
  scene.add(left, right);
  scene.play(scene.camera.frame.animate().moveTo(LEFT.mul(2)));
  scene.wait(0.3);
  scene.play(scene.camera.frame.animate().moveTo(RIGHT.mul(2)));
  return scene;
}

export const galleryMovingCameraCenter = Object.freeze({ MovingCameraCenter });

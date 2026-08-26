import { Rectangle, Scene } from "./noon-authoring.js";

export const DEFAULT_FRAME_HEIGHT = 8.0;
export const DEFAULT_FRAME_WIDTH = DEFAULT_FRAME_HEIGHT * 16.0 / 9.0;

/**
 * Thin browser-language adapter for Noon's shared semantic 2D camera role.
 *
 * Camera motion still lowers through the ordinary Rust/WASM Mobject animation path.
 * This wrapper only marks the hidden frame object as the camera in the serialized
 * scene document, matching the Python compatibility adapter.
 */
export class MovingCameraScene extends Scene {
  constructor() {
    super();
    const frame = new Rectangle(DEFAULT_FRAME_WIDTH, DEFAULT_FRAME_HEIGHT);
    this.add(frame);
    // Hide the semantic camera frame through the bound Rust scene editor so its
    // canonical object opacity matches Rust/Python and is preserved by transforms.
    frame.setOpacity(0.0);
    this.camera = Object.freeze({ frame });
  }

  sceneJson() {
    const document = JSON.parse(super.sceneJson());
    document.camera_object = this.camera.frame._handle;
    return JSON.stringify(document);
  }
}

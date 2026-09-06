// These browser smokes exercise explicit serialized scene entry points, including
// a real worker boundary. Build their codec fixture through the current semantic
// authoring facade instead of restoring a production demo/export API.
export function createExplicitTransportSceneJson(wasm) {
  const scene = new wasm.AuthoringSceneCore();
  const circle = scene.add(wasm.authoringCircle(0.65));
  const rectangle = scene.add(wasm.authoringRectangle(1.5, 0.9));
  const line = scene.add(wasm.authoringLine(-1.2, 0, 1.2, 0));
  const square = scene.add(wasm.authoringSquare(0.8));

  scene.moveTo(circle, -2.0, 0.6);
  scene.moveTo(rectangle, 2.0, 0.6);
  scene.moveTo(line, -1.5, -1.4);
  scene.moveTo(square, 1.5, -1.4);

  // These worker fixtures assert active playback and repeated presentation, so
  // give the otherwise static transport scene a real authored semantic track.
  const circleTarget = circle.targetEditor();
  circleTarget.moveTo(-0.4, 0.6);
  scene.ordinaryPlayTransformTo(circle, circleTarget, 4.0, "linear");
  return scene.sceneJson();
}

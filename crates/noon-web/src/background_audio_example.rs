//! Direct typed scene bootstrap for the original background-music example.

use wasm_bindgen::prelude::*;

/// The browser supplies only a canvas. Scene, animation, PCM preparation,
/// lowering, execution, and output instructions stay in this Rust context.
#[wasm_bindgen(js_name = createBackgroundAudioRenderer)]
pub async fn create_background_audio_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let (scene, animation) = noon::example_scenes::background_audio::scene()
        .map_err(|error| JsValue::from_str(&error))?;
    let navigation = scene.section_navigation();
    let session = scene
        .into_execution_with_animation_root(&animation)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let mut renderer =
        crate::WasmExecutionCanvasRenderer::create_from_scene_execution(canvas, session).await?;
    renderer.set_section_navigation(navigation);
    Ok(renderer)
}
